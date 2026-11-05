"""Chunked, read-only expression QC. Only bounded aggregates leave Python."""
import math


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


def _histogram(values, cutoff_min=None, cutoff_max=None, log=False):
    import numpy as np
    transformed = np.log10(values + 1) if log else values
    counts, edges = np.histogram(transformed, bins=32, range=(0, max(float(transformed.max()), 1.0)))
    actual_edges = (10**edges - 1) if log else edges
    return {'counts': counts.tolist(), 'edges': actual_edges.tolist(), 'log': log,
            'median': float(np.median(values)), 'min': float(values.min()), 'max': float(values.max()),
            'lower': cutoff_min, 'upper': cutoff_max}


def quality_control(data, options, progress=lambda percent, phase: None, selection_callback=None):
    import numpy as np
    import pandas as pd
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
    if o['geneColumn'] == '_index':
        symbols = pd.Series(var.index, dtype='string')
    elif o['geneColumn'] in var:
        symbols = var[o['geneColumn']].astype('string')
    else:
        raise ValueError('invalid_gene_column')
    mito = symbols.str.upper().str.startswith(o['mitoPrefix'].upper(), na=False).to_numpy(dtype=bool)
    if matrix is None:
        raise ValueError('invalid_matrix')
    n, g = matrix.shape
    if n != data.n_obs or g != len(var):
        raise ValueError('invalid_matrix')
    totals, detected, mito_totals = np.zeros(n), np.zeros(n, dtype=np.int64), np.zeros(n)
    invalid = {'nonfinite': 0, 'negative': 0, 'fractional': 0}
    # About 8 MiB for a float64 dense block; sparse blocks remain sparse.
    by_column = getattr(matrix, 'format', None) == 'csc'
    length = g if by_column else n
    chunk = max(1, min(1024, 1_000_000 // max(n if by_column else g, 1)))
    last_percent = -1
    for start in range(0, length, chunk):
        end = min(start + chunk, length)
        block = matrix[:, start:end] if by_column else matrix[start:end, :]
        if sparse.issparse(block):
            block = block.tocsr(copy=True)
            values = block.data
        else:
            block = np.asarray(block)
            values = block.ravel()
        if not np.issubdtype(values.dtype, np.number) or np.iscomplexobj(values):
            raise ValueError('invalid_matrix')
        finite = np.isfinite(values)
        invalid['nonfinite'] += int(np.count_nonzero(~finite))
        invalid['negative'] += int(np.count_nonzero(finite & (values < 0)))
        finite_values = values[finite]
        invalid['fractional'] += int(np.count_nonzero(np.abs(finite_values - np.rint(finite_values)) > 1e-6))
        if not any(invalid.values()):
            if sparse.issparse(block):
                block.sum_duplicates()
            target = slice(None) if by_column else slice(start, end)
            totals[target] += np.asarray(block.sum(axis=1, dtype=np.float64)).ravel()
            detected[target] += np.asarray((block > 0).sum(axis=1)).ravel()
            block_mito = mito[start:end] if by_column else mito
            if block_mito.any():
                mito_totals[target] += np.asarray(block[:, block_mito].sum(axis=1, dtype=np.float64)).ravel()
        percent = 72 + int(20 * end / length)
        if percent != last_percent:
            progress(percent, 'expression')
            last_percent = percent
    valid = not any(invalid.values()) and bool(np.isfinite(totals).all())
    available_mito = bool(mito.any())
    passed = np.zeros(n, dtype=bool)
    metrics = {}
    failures = {}
    if valid:
        zero = totals <= 0
        percent = np.divide(mito_totals * 100, totals, out=np.zeros(n), where=~zero)
        masks = {
            'zeroCounts': zero,
            'lowGenes': detected < o['minGenes'],
            'highGenes': detected > o['maxGenes'] if o['maxGenes'] is not None else np.zeros(n, bool),
            'lowCounts': totals < o['minCounts'],
            'highCounts': totals > o['maxCounts'] if o['maxCounts'] is not None else np.zeros(n, bool),
            'highMito': percent > o['maxMitoPct'] if available_mito and o['maxMitoPct'] is not None else np.zeros(n, bool),
        }
        passed = ~np.logical_or.reduce(list(masks.values()))
        failures = {key: int(mask.sum()) for key, mask in masks.items()}
        metrics = {'counts': _histogram(totals, o['minCounts'], o['maxCounts'], True),
                   'genes': _histogram(detected, o['minGenes'], o['maxGenes'], True)}
        # Zero-library cells have undefined percentages, not 0% mitochondrial expression.
        if available_mito and (~zero).any():
            metrics['mito'] = _histogram(percent[~zero], None, o['maxMitoPct'])
    donor = {'column': o['donorColumn'], 'distinct': 0, 'missing': n, 'missingRetained': None,
             'groups': [], 'otherCells': 0, 'identityVerified': False}
    if o['donorColumn']:
        labels = data.obs[o['donorColumn']].astype('string')
        absent = labels.isna() | labels.str.strip().eq('').fillna(True)
        labels = labels.mask(absent)
        counts = labels.value_counts()
        retained = labels.iloc[np.flatnonzero(passed)].value_counts() if valid else None
        donor.update({'distinct': len(counts), 'missing': int(absent.sum()),
                      'missingRetained': int((absent.to_numpy() & passed).sum()) if valid else None,
                      'otherCells': int(counts.iloc[30:].sum()),
                      'groups': [{'label': str(key)[:200], 'cells': int(value),
                                  'retained': int(retained.get(key, 0)) if valid else None}
                                 for key, value in counts.iloc[:30].items()]})
    if valid and selection_callback is not None:
        bits = np.zeros(n, dtype=np.uint8)
        for index, mask in enumerate(masks.values()):
            bits |= mask.astype(np.uint8) << index
        donor_ids = np.full(n, '', dtype=str)
        donor_missing = np.ones(n, dtype=bool)
        if o['donorColumn']:
            donor_ids = data.obs[o['donorColumn']].astype('string').fillna('').to_numpy(dtype=str)
            donor_missing = np.array([not value.strip() for value in donor_ids], dtype=bool)
        mito_percent = np.full(n, np.nan)
        if available_mito:
            np.divide(mito_totals * 100, totals, out=mito_percent, where=totals > 0)
        selection_callback({'source_row': np.arange(n), 'cell_id': np.asarray(data.obs_names, dtype=str),
                            'retained': passed, 'reason_bits': bits, 'total_counts': totals,
                            'detected_genes': detected, 'mito_percent': mito_percent,
                            'donor_id': donor_ids, 'donor_missing': donor_missing})
    return {'version': 1, 'status': ('ready' if available_mito else 'partial') if valid else 'invalid_counts',
            'options': o, 'features': g, 'mitoFeatures': int(mito.sum()), 'invalidValues': invalid,
            'cells': n, 'retained': int(passed.sum()) if valid else None,
            'excluded': int(n - passed.sum()) if valid else None,
            'zeroCells': failures.get('zeroCounts'), 'failures': failures, 'metrics': metrics,
            'donor': donor, 'filterAppliedToFile': False}
