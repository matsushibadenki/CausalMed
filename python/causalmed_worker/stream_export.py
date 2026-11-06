"""Selected-row H5AD copy without materializing retained expression matrices.

The budget bounds explicit numeric read blocks, not total process RSS: cell/QC
vectors, annotations, HDF5 compression caches and library imports remain separate.
"""
import math


class H5adStreamCopy:
    def __init__(self, block_bytes=8 * 1024 * 1024):
        self.budget = max(1024, int(block_bytes))
        self.max_read_bytes = 0
        self.blocks = 0

    def read(self, node, key):
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
                step = max(1, self.budget // max(16, source.dtype.itemsize))
                for start in range(0, len(rows), step):
                    target[start:start + step] = self.read(source, rows[start:start + step])
                    tick((start + min(step, len(rows)-start)) / max(1, len(rows)))
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
        target = parent.create_group(name)
        self.attrs(source, target)
        for key in source:
            if encoding == 'categorical' and key == 'categories':
                source.file.copy(source[key], target, name=key)
            else:
                self.copy(source[key], target, key, rows, tick=tick)

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
        major = rows if csr else (np.arange(source_shape[1]) if columns is None else columns)
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
                    positions = np.searchsorted(minor, ii)
                    keep = positions < len(minor)
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


def stream_retained_h5ad(source, output, manifest, arrays, expected, progress, block_bytes=8 * 1024 * 1024):
    import h5py
    import numpy as np
    import json
    from anndata.io import read_elem, write_elem
    from anndata._io.h5ad import read_dataframe
    rows = np.flatnonzero(arrays['retained'])
    copier = H5adStreamCopy(block_bytes)
    provenance = 'causalmed_qc_' + manifest['analysisId'].replace('-', '')
    columns = {key: provenance + '_' + key for key in ('source_row', 'retained', 'total_counts', 'detected_genes', 'mito_percent', 'donor_id', 'donor_missing')}
    if 'canonical_donor_id' in arrays:
        columns.update({key: provenance + '_' + key for key in ('canonical_donor_id', 'canonical_donor_reviewed')})
    with h5py.File(source, 'r') as original, h5py.File(output, 'w') as result:
        legacy = isinstance(original['obs'], h5py.Dataset)
        if legacy:
            # These are annotation tables, not expression/embedding matrices.
            from anndata._io.h5ad import read_dataframe
            obs_original, var_original = read_dataframe(original['obs']), read_dataframe(original['var'])
            names = np.asarray(obs_original.index, dtype=str)
        else:
            if original['obs'].attrs.get('encoding-type') != 'dataframe':
                raise ValueError('unsupported_export_encoding')
            names = np.asarray(read_elem(original['obs'][original['obs'].attrs['_index']]), dtype=str)
        shape = original['X'].shape if isinstance(original['X'], h5py.Dataset) else tuple(original['X'].attrs.get('shape', original['X'].attrs.get('h5sparse_shape')))
        if shape != (manifest['source']['cells'], manifest['source']['genes']) or not np.array_equal(names, arrays['cell_id']):
            raise ValueError('file_changed')
        if provenance in original.get('uns', {}) or any(key in (obs_original.columns if legacy else original['obs']) for key in columns.values()):
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
                elif key == 'var' and isinstance(original['raw'][key], h5py.Dataset): write_elem(group, key, read_dataframe(original['raw'][key]))
                else: original.copy(original['raw'][key], group, name=key)
        if 'raw.X' in original:
            if 'raw' in result or 'raw.var' not in original: raise ValueError('unsupported_export_encoding')
            group = result.create_group('raw'); group.attrs['encoding-type'] = 'raw'; group.attrs['encoding-version'] = '0.1.0'
            from anndata._io.h5ad import read_dataframe
            raw_var = read_dataframe(original['raw.var']); write_elem(group, 'var', raw_var)
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
            import pandas as pd
            # Match AnnData's pre-0.7 categorical metadata interpretation.
            for key in list(result.get('uns', {})):
                if not key.endswith('_categories'): continue
                label, categories = key[:-11], read_elem(result['uns'][key])
                categories = [categories] if np.isscalar(categories) else categories
                converted = False
                for frame in (obs_original, var_original):
                    if label in frame and np.issubdtype(frame[label].dtype, np.integer) and np.all(frame[label].values < len(categories)):
                        frame[label] = pd.Categorical.from_codes(frame[label].values, categories); converted = True
                if converted: del result['uns'][key]
            obs = obs_original.iloc[rows].copy()
            write_elem(result, 'var', var_original)
        else:
            obs = read_elem(result['obs'])
        for key, column in columns.items(): obs[column] = arrays[key][rows]
        if 'obs' in result: del result['obs']
        write_elem(result, 'obs', obs)
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
        obs = read_elem(check['obs'])
        if shape != (len(rows), manifest['source']['genes']) or not np.array_equal(np.asarray(obs.index, dtype=str), arrays['cell_id'][rows]):
            raise ValueError('export_verification_failed')
        if read_elem(check['uns'][provenance])['manifest_json'] != json.dumps(manifest, ensure_ascii=False, allow_nan=False, sort_keys=True):
            raise ValueError('export_verification_failed')
    return provenance, {'blockBudgetBytes': copier.budget, 'maxReadBlockBytes': copier.max_read_bytes, 'readBlocks': copier.blocks}
