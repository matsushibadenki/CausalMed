"""Chunked, read-only expression QC. Only bounded aggregates leave Python."""
import math

if 'iter_annotation_column' not in globals():
    from causalmed_worker.backed import iter_annotation_column, iter_frame_index, scratch_vector


def qc_options(options=None):
    defaults = {'matrix': 'X', 'geneColumn': '_index', 'mitoPrefix': 'MT-',
                'donorColumn': '', 'minGenes': 200, 'maxGenes': 6000,
                'minCounts': 0, 'maxCounts': None, 'maxMitoPct': 20}
    options = options or {}
    if not isinstance(options, dict) or set(options) - set(defaults):
        raise ValueError('invalid_qc_options')
    result = {**defaults, **options}
    for key in ('matrix', 'geneColumn', 'mitoPrefix', 'donorColumn'):
        if not isinstance(result[key], str) or len(result[key]) > 200:
            raise ValueError('invalid_qc_options')
    if not result['mitoPrefix'].strip():
        raise ValueError('invalid_qc_options')
    for key in ('minGenes', 'maxGenes', 'minCounts', 'maxCounts', 'maxMitoPct'):
        value = result[key]
        if value is None and key in ('maxGenes', 'maxCounts', 'maxMitoPct'):
            continue
        if type(value) not in (int, float) or not math.isfinite(value) or value < 0:
            raise ValueError('invalid_qc_options')
        if key in ('minGenes', 'maxGenes') and value != int(value):
            raise ValueError('invalid_qc_options')
    if result['maxMitoPct'] is not None and result['maxMitoPct'] > 100:
        raise ValueError('invalid_qc_options')
    for low, high in (('minGenes', 'maxGenes'), ('minCounts', 'maxCounts')):
        if result[high] is not None and result[low] > result[high]:
            raise ValueError('invalid_qc_options')
    return result


def _histogram(values, cutoff_min=None, cutoff_max=None, log=False, included=None):
    import tempfile
    import numpy as np

    def blocks():
        for start in range(0, len(values), 8192):
            block = values[start:start + 8192]
            if included is not None:
                selection = slice(start, start + len(block))
                mask = included(selection) if callable(included) else included[selection]
                block = block[mask]
            if len(block):
                yield block

    size = len(values) if included is None else sum(len(block) for block in blocks())
    minimum = min(block.min() for block in blocks())
    maximum = max(block.max() for block in blocks())
    transformed_max = np.log10(np.asarray([maximum], dtype=values.dtype) + 1)[0] if log else maximum
    bounds = (0, max(float(transformed_max), 1.0))
    counts, edges = np.histogram([], bins=32, range=bounds)
    with tempfile.TemporaryFile() as backing:
        middle = np.memmap(backing, dtype=values.dtype, mode='w+', shape=(size,))
        offset = 0
        for block in blocks():
            middle[offset:offset + len(block)] = block
            offset += len(block)
            transformed = np.log10(block + 1) if log else block
            counts += np.histogram(transformed, bins=32, range=bounds)[0]
        median = float(np.median(middle, overwrite_input=True))
        del middle
    actual_edges = (10**edges - 1) if log else edges
    return {'counts': counts.tolist(), 'edges': actual_edges.tolist(), 'log': log,
            'median': median, 'min': float(minimum), 'max': float(maximum),
            'lower': cutoff_min, 'upper': cutoff_max}


def mitochondrial_mask(frame, column, prefix, block_rows=8192):
    """Convert feature labels in blocks and retain only the Boolean mask."""
    import numpy as np
    if column == '_index':
        blocks = iter_frame_index(frame, block_rows)
    elif column in frame:
        blocks = iter_annotation_column(frame, column, block_rows, include_index=False)
    else:
        raise ValueError('invalid_gene_column')

    mask = np.empty(len(frame), dtype=bool)
    prefix = prefix.upper()
    for start, block in blocks:
        matches = block.astype('string').str.upper().str.startswith(prefix, na=False)
        mask[start:start + len(block)] = matches.to_numpy(dtype=bool)
    return mask


CSC_ENTRY_LIMIT = 262144


def invalid_counts(values):
    import numpy as np

    if not np.issubdtype(values.dtype, np.number) or np.iscomplexobj(values):
        raise ValueError('invalid_matrix')
    finite = np.isfinite(values)
    finite_values = values[finite]
    return {
        'nonfinite': int(np.count_nonzero(~finite)),
        'negative': int(np.count_nonzero(finite & (values < 0))),
        'fractional': int(np.count_nonzero(np.abs(finite_values - np.rint(finite_values)) > 1e-6)),
    }


def compact_csc(group, start, stop, rows):
    """Read a column block with tall row axes compressed to occupied rows."""
    import numpy as np
    from scipy import sparse

    pointers = group['indptr'][start:stop + 1].astype(np.int64)
    first, last = int(pointers[0]), int(pointers[-1])
    if first < 0 or last > len(group['data']) or last > len(group['indices']) or np.any(pointers[1:] < pointers[:-1]):
        raise ValueError('invalid_matrix')
    if last - first > CSC_ENTRY_LIMIT:
        return None, None
    indices = group['indices'][first:last]
    if np.any(indices < 0) or np.any(indices >= rows):
        raise ValueError('invalid_matrix')
    occupied, local = np.unique(indices, return_inverse=True)
    values = group['data'][first:last]
    block = sparse.csc_matrix((values, local, pointers - first), shape=(len(occupied), stop - start))
    return block.tocsr(), occupied


def accumulate_long_csc_column(group, column, totals, detected, mito_totals, invalid):
    """Stream a long strictly ordered column, falling back for duplicate/unsorted rows."""
    import numpy as np

    first, last = map(int, group['indptr'][column:column + 2])
    previous = -1
    findings = dict.fromkeys(invalid, 0)
    for start in range(first, last, CSC_ENTRY_LIMIT):
        stop = min(start + CSC_ENTRY_LIMIT, last)
        indices = group['indices'][start:stop]
        if indices[0] <= previous or np.any(indices[1:] <= indices[:-1]):
            return False
        previous = int(indices[-1])
        if previous >= len(totals):
            raise ValueError('invalid_matrix')
        for key, count in invalid_counts(group['data'][start:stop]).items():
            findings[key] += count
    for key, count in findings.items():
        invalid[key] += count
    if not any(invalid.values()):
        for start in range(first, last, CSC_ENTRY_LIMIT):
            stop = min(start + CSC_ENTRY_LIMIT, last)
            indices = group['indices'][start:stop]
            values = group['data'][start:stop]
            totals[indices] += values
            detected[indices] += values > 0
            if mito_totals is not None:
                mito_totals[indices] += values
    return True


def ordered_csc_tiles(group, first_column, last_column, row_count, invalid):
    """Stage an oversized raw block, then emit canonical CSR tiles in row order."""
    import sqlite3
    from contextlib import closing
    from itertools import groupby

    import numpy as np
    from scipy import sparse

    dtype = group['data'].dtype
    pointers = group['indptr'][first_column:last_column + 1]
    with closing(sqlite3.connect('')) as database:
        database.execute('PRAGMA cache_size=-1024')
        database.execute('CREATE TABLE entries (row_id INTEGER, col INTEGER, ordinal INTEGER, '
                         'value BLOB, PRIMARY KEY (row_id, col, ordinal)) WITHOUT ROWID')
        for column, (first, last) in enumerate(zip(pointers, pointers[1:])):
            for start in range(int(first), int(last), CSC_ENTRY_LIMIT):
                stop = min(start + CSC_ENTRY_LIMIT, int(last))
                rows = group['indices'][start:stop]
                if rows.dtype.kind not in 'iu' or np.any(rows < 0) or np.any(rows >= row_count):
                    raise ValueError('invalid_matrix')
                values = group['data'][start:stop]
                for key, count in invalid_counts(values).items():
                    invalid[key] += count
                if not any(invalid.values()):
                    # BLOBs preserve the input dtype, including integer overflow
                    # behavior when duplicate coordinates are summed below.
                    raw = values.view(np.uint8).reshape(len(values), dtype.itemsize)
                    database.executemany(
                        'INSERT INTO entries VALUES (?, ?, ?, ?)',
                        ((int(row), column, start + offset, raw[offset].tobytes())
                         for offset, row in enumerate(rows)),
                    )
        if any(invalid.values()):
            return

        width = last_column - first_column
        rows, columns, values, indptr = [], [], [], [0]

        def tile():
            return (sparse.csr_matrix((np.asarray(values, dtype=dtype), columns, indptr),
                                      shape=(len(rows), width)), np.asarray(rows, dtype=np.intp))

        entries = database.execute('SELECT row_id, col, value FROM entries ORDER BY row_id, col, ordinal')
        for row, row_entries in groupby(entries, key=lambda entry: entry[0]):
            rows.append(row)
            for column, duplicates in groupby(row_entries, key=lambda entry: entry[1]):
                value = np.frombuffer(next(duplicates)[2], dtype=dtype)[0]
                for _, _, raw_value in duplicates:
                    value = np.add(value, np.frombuffer(raw_value, dtype=dtype)[0], dtype=dtype.type)
                columns.append(column)
                values.append(value)
            indptr.append(len(values))
            if len(rows) == 8192 or len(values) >= CSC_ENTRY_LIMIT:
                yield tile()
                rows, columns, values, indptr = [], [], [], [0]
        if rows:
            yield tile()


def accumulate_qc_block(block, occupied, row_origin, mito_columns, totals, detected, mito_totals):
    """Apply row reductions with the same CSR grouping and arithmetic as QC."""
    import numpy as np

    has_mito = bool(mito_columns.any())
    for start in range(0, block.shape[0], 8192):
        part = block[start:start + 8192]
        stop = start + part.shape[0]
        target = occupied[start:stop] if occupied is not None else slice(row_origin + start, row_origin + stop)
        totals[target] += np.asarray(part.sum(axis=1, dtype=np.float64)).ravel()
        detected[target] += np.asarray((part > 0).sum(axis=1)).ravel()
        if has_mito:
            mito_totals[target] += np.asarray(part[:, mito_columns].sum(axis=1, dtype=np.float64)).ravel()


def quality_control(data, options, progress=lambda percent, phase: None, selection_callback=None, *, stream_selection=False):
    import numpy as np
    from scipy import sparse
    o = qc_options(options)
    if o['donorColumn'] and o['donorColumn'] not in data.obs:
        raise ValueError('invalid_donor_column')
    if o['matrix'] == 'X':
        matrix, var = data.X, data.var
    elif o['matrix'] == 'raw' and data.raw is not None:
        matrix, var = data.raw.X, data.raw.var
    elif o['matrix'].startswith('layer:') and o['matrix'][6:] in data.layers:
        matrix, var = data.layers[o['matrix'][6:]], data.var
    else:
        raise ValueError('invalid_matrix')
    if matrix is None:
        raise ValueError('invalid_matrix')
    n, g = matrix.shape
    if n != data.n_obs or g != len(var):
        raise ValueError('invalid_matrix')
    mito = mitochondrial_mask(var, o['geneColumn'], o['mitoPrefix'])
    available_mito = bool(mito.any())
    totals = scratch_vector(n, np.float64)
    detected = scratch_vector(n, np.int64)
    mito_totals = scratch_vector(n, np.float64) if available_mito else None
    invalid = {'nonfinite': 0, 'negative': 0, 'fractional': 0}
    # About 8 MiB for a float64 dense block; sparse blocks remain sparse.
    by_column = getattr(matrix, 'format', None) == 'csc'
    length = g if by_column else n
    chunk = max(1, min(1024, 1_000_000 // max(n if by_column else g, 1)))
    last_percent = -1
    for start in range(0, length, chunk):
        end = min(start + chunk, length)
        group = getattr(matrix, 'group', None) if by_column else None
        occupied = None
        block_mito = mito[start:end] if by_column else mito
        if group is not None:
            block, occupied = compact_csc(group, start, end, n)
        else:
            block = matrix[:, start:end] if by_column else matrix[start:end, :]
            block = block.tocsr(copy=True) if sparse.issparse(block) else np.asarray(block)

        if block is None:
            streamed = end == start + 1 and accumulate_long_csc_column(
                group, start, totals, detected, mito_totals if mito[start] else None, invalid
            )
            if not streamed:
                for tile, rows in ordered_csc_tiles(group, start, end, n, invalid):
                    accumulate_qc_block(tile, rows, 0, block_mito, totals, detected, mito_totals)
        else:
            values = block.data if sparse.issparse(block) else block.ravel()
            for key, count in invalid_counts(values).items():
                invalid[key] += count
            if not any(invalid.values()):
                if sparse.issparse(block):
                    block.sum_duplicates()
                accumulate_qc_block(block, occupied, 0 if by_column else start,
                                    block_mito, totals, detected, mito_totals)

        percent = 72 + int(20 * end / length)
        if percent != last_percent:
            progress(percent, 'expression')
            last_percent = percent
    valid = not any(invalid.values()) and all(
        np.isfinite(totals[start:start + 8192]).all() for start in range(0, n, 8192)
    )
    passed = scratch_vector(n, np.bool_)
    metrics = {}
    failures = {}
    if valid:
        reason_bits = scratch_vector(n, np.uint8)
        failures = dict.fromkeys(('zeroCounts', 'lowGenes', 'highGenes', 'lowCounts', 'highCounts', 'highMito'), 0)
        for start in range(0, n, 8192):
            block = slice(start, start + 8192)
            counts, genes = totals[block], detected[block]
            nonzero = counts > 0
            if available_mito:
                mito_values = mito_totals[block]
                np.multiply(mito_values, 100, out=mito_values)
                np.divide(mito_values, counts, out=mito_values, where=nonzero)
                mito_values[~nonzero] = np.nan
            masks = (
                ~nonzero,
                genes < o['minGenes'],
                genes > o['maxGenes'] if o['maxGenes'] is not None else None,
                counts < o['minCounts'],
                counts > o['maxCounts'] if o['maxCounts'] is not None else None,
                mito_values > o['maxMitoPct'] if available_mito and o['maxMitoPct'] is not None else None,
            )
            for bit, (key, mask) in enumerate(zip(failures, masks)):
                if mask is not None:
                    failures[key] += int(mask.sum())
                    reason_bits[block] |= mask.astype(np.uint8) << bit
            passed[block] = reason_bits[block] == 0
        metrics = {'counts': _histogram(totals, o['minCounts'], o['maxCounts'], True),
                   'genes': _histogram(detected, o['minGenes'], o['maxGenes'], True)}
        if available_mito and failures['zeroCounts'] < n:
            metrics['mito'] = _histogram(mito_totals, None, o['maxMitoPct'], included=lambda block: totals[block] > 0)
    donor = {'column': o['donorColumn'], 'distinct': 0, 'missing': n, 'missingRetained': None,
             'groups': [], 'otherCells': 0, 'identityVerified': False, 'inventoryComplete': False}
    if o['donorColumn']:
        donor.update(donor_summary(data.obs, o['donorColumn'], passed, valid))
    if valid and selection_callback is not None:
        def missing_mito_blocks():
            for start in range(0, n, 8192):
                yield np.full(min(n - start, 8192), np.nan)

        def donor_blocks():
            if o['donorColumn']:
                for _, block in iter_annotation_column(data.obs, o['donorColumn'], include_index=False):
                    yield block.astype('string').fillna('')
            else:
                for start in range(0, n, 8192):
                    yield np.full(min(n - start, 8192), '', dtype=str)

        def cell_blocks():
            for _, block in iter_frame_index(data.obs):
                yield block

        arrays = {
            'source_row': QcArray(n, np.int_, lambda: (
                np.arange(start, min(start + 8192, n)) for start in range(0, n, 8192)
            )),
            'cell_id': qc_text_array(n, cell_blocks),
            'retained': passed,
            'reason_bits': reason_bits,
            'total_counts': totals,
            'detected_genes': detected,
            'mito_percent': mito_totals if available_mito else QcArray(n, np.float64, missing_mito_blocks),
            'donor_id': qc_text_array(n, donor_blocks),
            'donor_missing': QcArray(n, np.bool_, lambda: (
                np.fromiter((not value.strip() for value in block),
                            dtype=bool, count=len(block))
                for block in donor_blocks()
            )),
        }
        if not stream_selection:
            arrays = {key: value.materialize() if isinstance(value, QcArray) else value
                      for key, value in arrays.items()}
        selection_callback(arrays)
    return {'version': 1, 'status': ('ready' if available_mito else 'partial') if valid else 'invalid_counts',
            'options': o, 'features': g, 'mitoFeatures': int(mito.sum()), 'invalidValues': invalid,
            'cells': n, 'retained': int(passed.sum()) if valid else None,
            'excluded': int(n - passed.sum()) if valid else None,
            'zeroCells': failures.get('zeroCounts'), 'failures': failures, 'metrics': metrics,
            'donor': donor, 'filterAppliedToFile': False}


class QcArray:
    """Deferred one-dimensional NPY payload with bounded row blocks."""

    def __init__(self, length, dtype, blocks):
        import numpy as np
        self.length = length
        self.dtype = np.dtype(dtype)
        self.blocks = blocks

    def materialize(self):
        import numpy as np
        result = np.empty(self.length, dtype=self.dtype)
        start = 0
        for block in self.blocks():
            result[start:start + len(block)] = block
            start += len(block)
        if start != self.length:
            raise ValueError('invalid_h5ad')
        return result

    def write(self, stream):
        import numpy as np
        np.lib.format.write_array_header_2_0(stream, {
            'descr': np.lib.format.dtype_to_descr(self.dtype),
            'fortran_order': False,
            'shape': (self.length,),
        })
        written = 0
        for block in self.blocks():
            rows_per_write = max(1, (1024 * 1024) // self.dtype.itemsize)
            for start in range(0, len(block), rows_per_write):
                stream.write(np.asarray(block[start:start + rows_per_write], dtype=self.dtype).tobytes())
            written += len(block)
        if written != self.length:
            raise ValueError('invalid_h5ad')


def qc_text_array(length, blocks):
    import numpy as np
    width = 1
    for block in blocks():
        if block.dtype.kind in ('O', 'U', 'T'):
            width = max(width, max((len(str(value)) for value in block), default=0))
        else:
            width = max(width, np.asarray(block, dtype=str).dtype.itemsize // 4)
    return QcArray(length, f'U{width}', blocks)


def donor_summary(frame, key, passed, valid=True, block_rows=8192):
    """Exact donor totals/retention, with disk-backed distinct-label counts.

    Only missing/blank values are absent; nonblank labels retain whitespace.
    Ties use source-first order, independent of processing block size.
    """
    import sqlite3
    from contextlib import closing
    import numpy as np
    passed = np.asarray(passed)
    if passed.shape != (len(frame),) or passed.dtype != np.dtype(bool):
        raise ValueError('invalid_h5ad')
    missing = missing_retained = 0
    labels_fit = True
    with closing(sqlite3.connect('')) as database:
        database.execute('PRAGMA cache_size=-1024')
        database.execute('PRAGMA temp_store=FILE')
        database.execute('CREATE TABLE counts (label TEXT PRIMARY KEY, cells INTEGER NOT NULL, retained INTEGER NOT NULL, first_row INTEGER NOT NULL) WITHOUT ROWID')
        for start, block in iter_annotation_column(frame, key, block_rows, include_index=False):
            labels = block.astype('string')
            absent = labels.isna() | labels.str.strip().eq('').fillna(True)
            retained_mask = passed[start:start + len(block)]
            missing += int(absent.sum())
            if valid:
                missing_retained += int((absent.to_numpy(dtype=bool) & retained_mask).sum())
            labels = labels.mask(absent)
            counts = labels.value_counts(sort=False)
            retained = labels.iloc[retained_mask].value_counts(sort=False) if valid else None
            rows = []
            for ordinal, (label, count) in enumerate(counts.items()):
                label = str(label)
                labels_fit &= len(label.encode('utf-8')) <= 200
                rows.append((label, int(count), int(retained.get(label, 0)) if valid else 0, start + ordinal))
            database.executemany('INSERT INTO counts VALUES (?,?,?,?) ON CONFLICT(label) DO UPDATE SET cells=cells+excluded.cells, retained=retained+excluded.retained', rows)
        distinct, total = database.execute('SELECT count(*), coalesce(sum(cells),0) FROM counts').fetchone()
        top = database.execute('SELECT label,cells,retained FROM counts ORDER BY cells DESC,first_row ASC LIMIT 30').fetchall()
    return {'distinct': distinct, 'missing': missing,
            'missingRetained': missing_retained if valid else None,
            'otherCells': total - sum(count for _, count, _ in top),
            'inventoryComplete': distinct <= 30 and labels_fit,
            'groups': [{'label': label[:200], 'cells': count, 'retained': retained if valid else None}
                       for label, count, retained in top]}


def donor_selection(frame, key, block_rows=8192):
    """Fill final immutable donor arrays without a full decoded column copy.

    Two passes determine Unicode width then fill blocks. Final NPZ arrays are
    still O(cells * longest label); this is not a total-memory or byte cap.
    """
    import numpy as np
    width = 1
    for _, block in iter_annotation_column(frame, key, block_rows, include_index=False):
        labels = block.astype('string').fillna('')
        if len(labels):
            width = max(width, int(labels.str.len().max()))
    donor_ids = np.empty(len(frame), dtype=f'U{width}')
    missing = np.empty(len(frame), dtype=bool)
    for start, block in iter_annotation_column(frame, key, block_rows, include_index=False):
        labels = block.astype('string').fillna('')
        stop = start + len(labels)
        donor_ids[start:stop] = labels.to_numpy(dtype=str)
        missing[start:stop] = labels.str.strip().eq('').to_numpy(dtype=bool)
    return donor_ids, missing
