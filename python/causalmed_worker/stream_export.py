"""Selected-row H5AD copy without materializing retained expression matrices.
The budget bounds explicit numeric read blocks, not total process RSS: cell/QC
vectors, annotations, HDF5 compression caches and library imports remain separate.
"""

if 'index_matches' not in globals():
    from causalmed_worker.backed import (index_matches, read_research_frame, scratch_vector, LegacyFrame, H5Frame,
                                         configure_legacy_categories, string_block_rows, is_vlen_string,
                                         read_bounded_strings, projected_category_dictionary, CategoryDictionary, split_vlen_batch)


import math


class DonorExportColumn:
    """Project frozen donor mappings only for rows requested by the writer."""

    ndim = 1

    def __init__(self, donors, mappings, reviewed=False):
        import numpy as np

        self.donors = donors
        key, self.default = ('reviewed', False) if reviewed else ('canonicalId', '')
        self.values = {label: entry.get(key, self.default) for label, entry in mappings.items()}
        width = max((len(value) for value in self.values.values()), default=0) if not reviewed else 0
        self.dtype = np.dtype(bool if reviewed else f'U{max(1, width)}')

    def __getitem__(self, rows):
        import numpy as np

        return np.asarray([self.values.get(label, self.default) for label in self.donors[rows]],
                          dtype=self.dtype)


class H5adStreamCopy:
    def __init__(self, block_bytes=8 * 1024 * 1024):
        self.budget = max(1024, int(block_bytes))
        self.max_read_bytes = 0
        self.blocks = 0
        self.max_generated_bytes = 0
        self.generated_blocks = 0

    def read(self, node, key):
        if getattr(node, 'ndim', None) == 1 and is_vlen_string(node.dtype):
            value = read_bounded_strings(node, key, decode=False)
        else:
            value = node[key]
        self.max_read_bytes = max(self.max_read_bytes, value.nbytes)
        self.blocks += 1
        return value

    @staticmethod
    def attrs(source, target):
        for key, value in source.attrs.items():
            target.attrs[key] = value

    def copy(self, source, parent, name, rows, columns=None, tick=lambda value: None):
        import h5py
        import numpy as np
        if isinstance(source, h5py.Dataset):
            if not source.shape or source.shape[0] == 0:
                source.file.copy(source, parent, name=name)
                return
            if columns is not None and source.ndim != 2:
                raise ValueError('unsupported_export_encoding')
            shape = (len(rows), *source.shape[1:])
            if columns is not None:
                shape = (len(rows), len(columns))
            target = parent.create_dataset(name, shape=shape, dtype=source.dtype, compression='gzip' if all(shape) else None)
            self.attrs(source, target)
            if source.ndim == 1:
                step = string_block_rows(source.dtype, max(1, self.budget // max(16, source.dtype.itemsize)))
                for start in range(0, len(rows), step):
                    stop = min(start + step, len(rows))
                    for offset, block in split_vlen_batch(start, stop, lambda a, b: self.read(source, rows[a:b])):
                        target[offset:offset + len(block)] = block
                    tick(stop / max(1, len(rows)))
            elif source.ndim == 2:
                # Tile wide matrices too: even a single very wide row is bounded.
                width = max(1, min(source.shape[1], self.budget // max(1, source.dtype.itemsize)))
                height = max(1, self.budget // (width * max(1, source.dtype.itemsize)))
                for start in range(0, len(rows), height):
                    rr = rows[start:start + height]
                    for left in range(0, source.shape[1], width):
                        right = min(source.shape[1], left + width)
                        if columns is None:
                            cc, dst = None, slice(left, right)
                        else:
                            lo, hi = np.searchsorted(columns, [left, right])
                            if lo == hi:
                                continue
                            cc, dst = columns[lo:hi] - left, slice(lo, hi)
                        block = self.read(source, (rr, slice(left, right)))
                        target[start:start + len(rr), dst] = block if cc is None else block[:, cc]
                    tick((start + len(rr)) / max(1, len(rows)))
            else:
                # Higher dimensional embeddings use row blocks only when a row fits.
                row_bytes = math.prod(source.shape[1:]) * max(1, source.dtype.itemsize)
                if row_bytes > self.budget:
                    raise ValueError('unsupported_export_encoding')
                step = max(1, self.budget // max(1, row_bytes))
                for start in range(0, len(rows), step):
                    target[start:start + step] = self.read(source, rows[start:start + step])
                    tick(min(1, (start + step) / max(1, len(rows))))
            return
        encoding = source.attrs.get('encoding-type', '')
        if encoding in ('csr_matrix', 'csc_matrix') or 'h5sparse_format' in source.attrs:
            self.sparse(source, parent, name, rows, columns, tick)
            return
        if encoding not in ('dataframe', 'categorical', 'nullable-integer', 'nullable-boolean', 'nullable-string-array') or columns is not None:
            raise ValueError('unsupported_export_encoding')
        if encoding == 'categorical':
            # The former DataFrame round trip rejected invalid category codes.
            # Preserve that validation without decoding a full column/table.
            codes, categories = source['codes'], source['categories']
            if codes.ndim != 1 or codes.dtype.kind not in 'iu':
                raise ValueError('unsupported_export_encoding')
            import pandas as pd
            from anndata.io import read_elem
            try:
                dictionary = projected_category_dictionary(categories)
                if dictionary is None:
                    count = len(pd.CategoricalDtype(read_elem(categories)).categories)
                else:
                    dictionary.validate()
                    count = len(dictionary)
            except (ValueError, TypeError) as error:
                if str(error) == 'vlen_budget_exceeded':
                    raise
                raise ValueError('unsupported_export_encoding') from None
            step = max(1, self.budget // max(16, codes.dtype.itemsize))
            for start in range(0, len(rows), step):
                block = self.read(codes, rows[start:start + step])
                if np.any(block < -1) or np.any(block >= count):
                    raise ValueError('unsupported_export_encoding')
        target = parent.create_group(name)
        self.attrs(source, target)
        for key in source:
            if encoding == 'categorical' and key == 'categories':
                source.file.copy(source[key], target, name=key)
            else:
                self.copy(source[key], target, key, rows, tick=tick)

    def generated_column(self, values, parent, name, rows):
        """Write selected QC values without a complete retained-column copy."""
        import h5py
        import numpy as np
        if not isinstance(values, DonorExportColumn):
            values = np.asarray(values)
        if values.ndim != 1 or values.dtype.kind not in 'biufU':
            raise ValueError('invalid_export')
        row_bytes = max(1, values.dtype.itemsize)
        if row_bytes > self.budget:
            raise ValueError('unsupported_export_encoding')
        text = values.dtype.kind == 'U'
        target = parent.create_dataset(name, (len(rows),), dtype=h5py.string_dtype('utf-8') if text else values.dtype,
                                       compression='gzip' if len(rows) else None)
        target.attrs['encoding-type'] = 'string-array' if text else 'array'
        target.attrs['encoding-version'] = '0.2.0'
        step = max(1, min(8192, self.budget // max(16, row_bytes)))
        for start in range(0, len(rows), step):
            selected = values[rows[start:start + step]]
            self.max_generated_bytes = max(self.max_generated_bytes, selected.nbytes)
            self.generated_blocks += 1
            target[start:start + len(selected)] = selected.astype(object) if text else selected

    def legacy_frame(self, frame, parent, name, rows):
        """Convert one projected legacy field at a time to modern H5AD."""
        import h5py
        from anndata.io import write_elem

        group = parent.create_group(name)
        group.attrs['encoding-type'] = 'dataframe'
        group.attrs['encoding-version'] = '0.2.0'
        group.attrs['_index'] = frame.index_key
        group.attrs['column-order'] = list(frame.columns)
        for key in (frame.index_key, *frame.columns):
            dtype = frame.dtype(key)
            category = frame.categories.get(key)
            output = group
            field = key
            text = h5py.check_string_dtype(dtype) is not None
            if category is not None:
                output = group.create_group(key)
                output.attrs['encoding-type'] = 'categorical'
                output.attrs['encoding-version'] = '0.2.0'
                if isinstance(category, CategoryDictionary):
                    output.attrs['ordered'] = False
                    if category.mask is not None:
                        category.node.file.copy(category.node, output, name='categories')
                    else:
                        labels = output.create_dataset(
                            'categories', (len(category),),
                            dtype=h5py.string_dtype('utf-8') if category.text else category.dtype,
                        )
                        labels.attrs['encoding-type'] = 'string-array' if category.text else 'array'
                        labels.attrs['encoding-version'] = '0.2.0'
                        for start, block in category.blocks():
                            labels[start:start + len(block)] = block
                else:
                    output.attrs['ordered'] = category.ordered
                    write_elem(output, 'categories', category.categories.to_numpy())
                field = 'codes'
            target = output.create_dataset(field, (len(rows),),
                                           dtype=h5py.string_dtype('utf-8') if text else dtype,
                                           compression='gzip' if len(rows) else None)
            target.attrs['encoding-type'] = 'string-array' if text else 'array'
            target.attrs['encoding-version'] = '0.2.0'
            step = string_block_rows(dtype, max(1, min(8192, self.budget // max(1, dtype.itemsize))))
            def read_rows(start, stop):
                selected = rows[start:stop]
                first, last = int(selected[0]), int(selected[-1])
                # Dense selections avoid HDF5 point selection overhead.
                selection = slice(first, last + 1) if last - first + 1 == len(selected) else selected
                return frame.read(key, selection)

            for start in range(0, len(rows), step):
                stop = min(start + step, len(rows))
                for offset, values in split_vlen_batch(start, stop, read_rows):
                    target[offset:offset + len(values)] = values.astype(object) if text else values

    def legacy_mapping(self, source, parent, name, rows):
        group = parent.create_group(name)
        group.attrs['encoding-type'] = 'dict'; group.attrs['encoding-version'] = '0.1.0'
        if not source.dtype.names:
            if not source.size or (source.ndim > 1 and source.shape[1] == 0): return
            raise ValueError('unsupported_export_encoding')
        for key in source.dtype.names:
            dtype, dims = source.dtype[key].subdtype or (source.dtype[key], ())
            row_bytes = math.prod(dims) * dtype.itemsize
            if row_bytes > self.budget: raise ValueError('unsupported_export_encoding')
            out = group.create_dataset(key, (len(rows), *dims), dtype=dtype, compression='gzip')
            out.attrs['encoding-type'] = 'array'; out.attrs['encoding-version'] = '0.2.0'
            step = max(1, self.budget // max(1, row_bytes))
            for start in range(0, len(rows), step): out[start:start+step] = self.read(source.fields(key), rows[start:start+step])

    def sparse(self, source, parent, name, rows, columns, tick):
        import numpy as np
        csr = source.attrs.get('encoding-type', '') == 'csr_matrix' or source.attrs.get('h5sparse_format', '') == 'csr'
        source_shape = tuple(source.attrs.get('shape', source.attrs.get('h5sparse_shape')))
        major = rows if csr else (range(source_shape[1]) if columns is None else columns)
        minor = columns if csr else rows
        target = parent.create_group(name)
        self.attrs(source, target)
        target.attrs['shape'] = (len(rows), source_shape[1] if columns is None else len(columns))
        target.attrs['encoding-type'] = 'csr_matrix' if csr else 'csc_matrix'
        target.attrs['encoding-version'] = '0.1.0'
        for key in ('h5sparse_shape', 'h5sparse_format'):
            if key in target.attrs: del target.attrs[key]
        values = target.create_dataset('data', (0,), maxshape=(None,), dtype=source['data'].dtype, chunks=True, compression='gzip')
        indices = target.create_dataset('indices', (0,), maxshape=(None,), dtype=source['indices'].dtype, chunks=True, compression='gzip')
        pointers = target.create_dataset('indptr', (len(major)+1,), dtype=source['indptr'].dtype)
        pointers[0] = 0
        # Leave space for masks/remapping arrays alongside the input/output blocks.
        step = max(1, self.budget // (max(1, values.dtype.itemsize) + max(1, indices.dtype.itemsize) + 32))
        written = 0
        for out_index, original in enumerate(major):
            bounds = self.read(source['indptr'], slice(int(original), int(original)+2))
            for start in range(int(bounds[0]), int(bounds[1]), step):
                stop = min(int(bounds[1]), start + step)
                ii = self.read(source['indices'], slice(start, stop))
                vv = self.read(source['data'], slice(start, stop))
                if minor is not None:
                    # Match dtypes on the small query block so NumPy does not
                    # promote and copy the entire disk-backed index.
                    in_range = (ii >= 0) & (ii <= np.iinfo(minor.dtype).max)
                    query = np.where(in_range, ii, 0).astype(minor.dtype, copy=False)
                    positions = np.searchsorted(minor, query)
                    keep = in_range & (positions < len(minor))
                    if len(minor):
                        keep &= minor[np.minimum(positions, len(minor)-1)] == ii
                    ii, vv = positions[keep].astype(indices.dtype), vv[keep]
                end = written + len(vv)
                values.resize((end,)); indices.resize((end,))
                values[written:end] = vv; indices[written:end] = ii
                written = end
            pointers[out_index+1] = written
            if out_index % max(1, len(major)//100) == 0:
                tick((out_index+1) / max(1, len(major)))


def retained_row_index(retained):
    """Build a compact, disk-backed index in source order."""
    import numpy as np

    dtype = np.int32 if len(retained) <= np.iinfo(np.int32).max + 1 else np.int64
    rows = scratch_vector(int(np.count_nonzero(retained)), dtype, memory_limit=0)
    offset = 0
    for start in range(0, len(retained), 8192):
        block = np.flatnonzero(retained[start:start + 8192]) + start
        rows[offset:offset + len(block)] = block
        offset += len(block)
    return rows


def stream_retained_h5ad(source, output, manifest, arrays, expected, progress, block_bytes=8 * 1024 * 1024):
    import h5py
    import numpy as np
    import json
    from anndata.io import read_elem, write_elem
    rows = retained_row_index(arrays['retained'])
    copier = H5adStreamCopy(block_bytes)
    provenance = 'causalmed_qc_' + manifest['analysisId'].replace('-', '')
    columns = {key: provenance + '_' + key for key in ('source_row', 'retained', 'total_counts', 'detected_genes', 'mito_percent', 'donor_id', 'donor_missing')}
    if 'canonical_donor_id' in arrays:
        columns.update({key: provenance + '_' + key for key in ('canonical_donor_id', 'canonical_donor_reviewed')})
    with h5py.File(source, 'r') as original, h5py.File(output, 'w') as result:
        legacy = isinstance(original['obs'], h5py.Dataset)
        obs_frame = read_research_frame(original['obs'])
        if legacy:
            if not isinstance(obs_frame, LegacyFrame):
                raise ValueError('unsupported_export_encoding')
            var_original, converted_categories = configure_legacy_categories(
                obs_frame, read_research_frame(original['var']), original, exporting=True
            )
        elif not isinstance(obs_frame, H5Frame):
            raise ValueError('unsupported_export_encoding')
        shape = original['X'].shape if isinstance(original['X'], h5py.Dataset) else tuple(original['X'].attrs.get('shape', original['X'].attrs.get('h5sparse_shape')))
        if shape != (manifest['source']['cells'], manifest['source']['genes']) or not index_matches(obs_frame, arrays['cell_id']):
            raise ValueError('file_changed')
        if provenance in original.get('uns', {}) or any(key in (obs_frame.columns if legacy else original['obs']) for key in columns.values()):
            raise ValueError('metadata_collision')
        progress(10, 'writing')
        copier.attrs(original, result)
        result.attrs['encoding-type'] = 'anndata'; result.attrs['encoding-version'] = '0.1.0'
        tasks = [('X', rows, None)] if legacy else [('obs', rows, None), ('X', rows, None)]
        for mapping in ('layers', 'obsm', 'obsp'):
            if mapping in original:
                if isinstance(original[mapping], h5py.Dataset):
                    if not legacy or mapping != 'obsm': raise ValueError('unsupported_export_encoding')
                    copier.legacy_mapping(original[mapping], result, mapping, rows)
                else:
                    group = result.create_group(mapping); copier.attrs(original[mapping], group)
                    tasks.extend((f'{mapping}/{key}', rows, rows if mapping == 'obsp' else None) for key in original[mapping])
        if 'raw' in original:
            group = result.create_group('raw'); copier.attrs(original['raw'], group)
            for key in original['raw']:
                if key == 'X': tasks.append(('raw/X', rows, None))
                elif key == 'var' and isinstance(original['raw'][key], h5py.Dataset):
                    raw_var = read_research_frame(original['raw'][key])
                    copier.legacy_frame(raw_var, group, key, range(len(raw_var)))
                else: original.copy(original['raw'][key], group, name=key)
        if 'raw.X' in original:
            if 'raw' in result or 'raw.var' not in original: raise ValueError('unsupported_export_encoding')
            group = result.create_group('raw'); group.attrs['encoding-type'] = 'raw'; group.attrs['encoding-version'] = '0.1.0'
            raw_var = read_research_frame(original['raw.var'])
            if isinstance(raw_var, LegacyFrame):
                copier.legacy_frame(raw_var, group, 'var', range(len(raw_var)))
            else:
                original.copy(original['raw.var'], group, name='var')
            if 'raw.varm' in original:
                if isinstance(original['raw.varm'], h5py.Dataset): copier.legacy_mapping(original['raw.varm'], group, 'varm', np.arange(len(raw_var)))
                else: original.copy(original['raw.varm'],group,name='varm')
            tasks.append(('raw.X',rows,None))
        for key in original:
            if key in ('raw.X','raw.var','raw.varm'): continue
            if legacy and key == 'varm' and isinstance(original[key], h5py.Dataset):
                copier.legacy_mapping(original[key],result,key,np.arange(len(var_original))); continue
            if key not in ('X', 'obs', 'layers', 'obsm', 'obsp', 'raw') and not (legacy and key == 'var'):
                original.copy(original[key], result, name=key)
        last_percent = 9
        def advance(value):
            nonlocal last_percent
            if value > last_percent:
                last_percent = value
                progress(value, 'writing')
        for number, (path, rr, cc) in enumerate(tasks):
            parent, _, name = ('raw/X' if path == 'raw.X' else path).rpartition('/')
            copier.copy(original[path], result[parent] if parent else result, name, rr, cc,
                        lambda fraction, number=number: advance(10 + int(65*(number+fraction)/len(tasks))))
        if legacy:
            for key in converted_categories:
                del result['uns'][key]
            if isinstance(var_original, LegacyFrame):
                copier.legacy_frame(var_original, result, 'var', range(len(var_original)))
            elif isinstance(var_original, H5Frame):
                original.copy(original['var'], result, name='var')
            else:
                write_elem(result, 'var', var_original)
            copier.legacy_frame(obs_frame, result, 'obs', rows)
        # Original columns are already present. Append QC columns in bounded
        # selected-row blocks for both modern and legacy input layouts.
        obs_group = result['obs']
        order = list(obs_group.attrs['column-order'])
        for key, column in columns.items():
            copier.generated_column(arrays[key], obs_group, column, rows)
            order.append(column)
        obs_group.attrs['column-order'] = order
        if 'uns' not in result: write_elem(result, 'uns', {})
        write_elem(result['uns'], provenance, {'manifest_json': json.dumps(manifest, ensure_ascii=False, allow_nan=False, sort_keys=True),
                                             'obs_columns_json': json.dumps(columns, sort_keys=True), 'selection_manifest_sha256': expected['manifestSha256']})
        if 'cohort' in manifest:
            origin = manifest['cohort']
            definition_id = json.loads(origin['definition_json'])['id']
            key = 'causalmed_cohort_' + definition_id.replace('-', '')
            if key in result['uns']:
                raise ValueError('metadata_collision')
            write_elem(result['uns'], key, origin)
    progress(80, 'export_verify')
    with h5py.File(output, 'r') as check:
        shape = check['X'].shape if isinstance(check['X'], h5py.Dataset) else tuple(check['X'].attrs['shape'])
        # Identity verification needs the row index, not every annotation column.
        obs = check['obs']
        if shape != (len(rows), manifest['source']['genes']) or not index_matches(read_research_frame(obs), arrays['cell_id'], rows):
            raise ValueError('export_verification_failed')
        if read_elem(check['uns'][provenance])['manifest_json'] != json.dumps(manifest, ensure_ascii=False, allow_nan=False, sort_keys=True):
            raise ValueError('export_verification_failed')
    return provenance, {'blockBudgetBytes': copier.budget, 'maxReadBlockBytes': copier.max_read_bytes, 'readBlocks': copier.blocks, 'maxGeneratedBlockBytes': copier.max_generated_bytes, 'generatedBlocks': copier.generated_blocks}
