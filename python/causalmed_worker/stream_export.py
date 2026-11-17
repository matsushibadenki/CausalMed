"""Selected-row H5AD copy without materializing retained expression matrices.
The budget bounds explicit numeric read blocks, not total process RSS: cell/QC
vectors, annotations, HDF5 compression caches and library imports remain separate.
"""

if 'index_matches' not in globals():
    from causalmed_worker.backed import (index_matches, read_research_frame, scratch_vector, LegacyFrame, H5Frame,
                                         configure_legacy_categories, string_block_rows, is_budgeted_vlen,
                                         read_bounded_vlen, projected_category_dictionary, CategoryDictionary, split_vlen_batch, VLEN_BLOCK_ROWS, vlen_byte_limit, copy_native_vlen_attribute,
                                         copy_native_compound_tile, with_small_attribute_caches)


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
        if getattr(node, 'ndim', 0) > 0 and is_budgeted_vlen(node.dtype):
            value = read_bounded_vlen(node, key, decode=False)
        else:
            value = node[key]
        self.max_read_bytes = max(self.max_read_bytes, value.nbytes)
        self.blocks += 1
        return value

    @staticmethod
    def attrs(source, target):
        for key, value in source.attrs.items():
            target.attrs[key] = value

    @staticmethod
    def write_block(target, axes, values):
        import h5py
        import numpy as np

        if is_budgeted_vlen(target.dtype) and h5py.check_string_dtype(target.dtype) is None:
            # h5py can interpret equal-length sequence elements as an extra axis.
            for index in np.ndindex(values.shape):
                destination = tuple(axis.start + offset for axis, offset in zip(axes, index))
                target[destination] = values[index]
        else:
            target[tuple(axes)] = values

    @staticmethod
    def fixed_reference_subtree(dtype):
        """Identify fields whose object values are exclusively HDF5 references."""
        import h5py

        if not dtype.hasobject or h5py.check_ref_dtype(dtype) is not None:
            return True
        if dtype.subdtype:
            return H5adStreamCopy.fixed_reference_subtree(dtype.subdtype[0])
        return bool(dtype.fields) and all(
            H5adStreamCopy.fixed_reference_subtree(dtype.fields[name][0])
            for name in dtype.names
        )

    @staticmethod
    def compound_fields(dtype, prefix=()):
        """Project leaves of nested compounds and arrays containing VLEN data."""
        if len(prefix) == 1 and dtype.hasobject and H5adStreamCopy.fixed_reference_subtree(dtype):
            yield prefix, dtype
        elif dtype.fields and (dtype.hasobject or not prefix):
            for name in dtype.names:
                yield from H5adStreamCopy.compound_fields(dtype.fields[name][0], (*prefix, name))
        elif dtype.subdtype and dtype.hasobject:
            element, shape = dtype.subdtype
            yield from H5adStreamCopy.compound_fields(element, (*prefix, shape))
        else:
            yield prefix, dtype

    @staticmethod
    def supports_compound(dtype):
        return bool(dtype.fields) and all(
            not field_dtype.hasobject or is_budgeted_vlen(field_dtype)
            or (len(path) == 1 and H5adStreamCopy.fixed_reference_subtree(field_dtype))
            for path, field_dtype in H5adStreamCopy.compound_fields(dtype)
        )

    def copy_compound(self, source, target, *, native=False):
        """Copy projected fields with one shared payload budget per tile."""
        from itertools import product
        import h5py
        import numpy as np

        fields = list(self.compound_fields(source.dtype))
        variable = {path for path, dtype in fields if is_budgeted_vlen(dtype)}

        def write_native_tile(bounds):
            try:
                copy_native_compound_tile(source, target, bounds)
            except ValueError as error:
                if str(error) != 'vlen_budget_exceeded' or all(stop - start == 1 for start, stop in bounds):
                    raise
            else:
                return
            for half in split_tile(bounds):
                write_native_tile(half)

        def split_tile(bounds):
            axis = max(range(len(bounds)), key=lambda index: bounds[index][1] - bounds[index][0])
            start, stop = bounds[axis]
            middle = (start + stop) // 2
            for half in ((start, middle), (middle, stop)):
                yield bounds[:axis] + [half] + bounds[axis + 1:]

        def write_tile(bounds):
            selection = tuple(slice(start, stop) for start, stop in bounds)
            key = selection[0] if source.ndim == 1 else selection
            records = np.empty(tuple(stop - start for start, stop in bounds), dtype=source.dtype)
            remaining = vlen_byte_limit()
            try:
                for path, dtype in fields:
                    destination = records
                    for name in path:
                        if isinstance(name, str):
                            destination = destination[name]
                    if path not in variable:
                        destination[...] = (self.read(source.fields(path[0]), key) if len(path) == 1
                                            else read_bounded_vlen(source, key, field=path))
                        continue
                    values = read_bounded_vlen(source, key, field=path, decode=False, byte_limit=remaining)
                    if destination.ndim == 0:
                        destination[()] = values
                    else:
                        destination[...] = values
                    text = h5py.check_string_dtype(dtype) is not None
                    if destination.ndim == 0:
                        remaining -= len(values) + 1 if text else values.nbytes
                    else:
                        remaining -= sum(len(value) + 1 if text else value.nbytes for value in values.flat)
                    values = None
            except ValueError as error:
                if str(error) != 'vlen_budget_exceeded' or records.size <= 1:
                    raise
            else:
                target[selection] = records
                return
            del destination, records
            for half in split_tile(bounds):
                write_tile(half)

        write = write_native_tile if native else write_tile
        if not source.ndim:
            write([])
        elif all(source.shape):
            remaining = max(1, self.budget // source.dtype.itemsize)
            if variable or native:
                remaining = min(remaining, VLEN_BLOCK_ROWS)
            steps = []
            for length in reversed(source.shape):
                step = min(length, remaining)
                steps.append(step)
                remaining //= step
            steps.reverse()
            for starts in product(*(range(0, length, step) for length, step in zip(source.shape, steps))):
                write([(start, min(start + step, length))
                            for start, step, length in zip(starts, steps, source.shape)])

    @staticmethod
    def copy_object_attrs(source, target):
        import h5py
        import numpy as np

        from contextlib import closing

        cross_file = source.file.id != target.file.id
        for name in source.attrs:
            attribute = source.attrs.get_id(name)
            ref_type = h5py.check_ref_dtype(attribute.dtype)
            if cross_file and ref_type is not None and attribute.shape is not None:
                value = np.empty(attribute.shape, dtype=attribute.dtype)
                value.fill(ref_type())
            elif (attribute.shape is not None and attribute.dtype.hasobject
                  and not attribute.get_type().detect_class(h5py.h5t.REFERENCE)):
                with_small_attribute_caches(
                    (source.file, target.file),
                    lambda: copy_native_vlen_attribute(attribute, target, name),
                )
                continue
            elif (attribute.shape and not attribute.dtype.hasobject
                  and math.prod(attribute.shape) * attribute.dtype.itemsize >= 1024 * 1024):
                # H5Aread is whole-value. Keep the transfer buffer file-backed.
                with closing(h5py.h5a.create(
                    target.id, name.encode('utf-8'), attribute.get_type(), attribute.get_space()
                )) as output:
                    buffer = scratch_vector(math.prod(attribute.shape), attribute.dtype, memory_limit=0)
                    buffer = buffer.reshape(attribute.shape)
                    attribute.read(buffer)
                    output.write(buffer)
                    del buffer
                continue
            else:
                value = source.attrs[name]
            target.attrs.create(name, value, dtype=attribute.dtype)

    def copy_object(self, source, parent, name, seen=None):
        """Copy ordinary metadata groups and datasets without a whole-array read."""
        from itertools import product
        import h5py

        if seen is None:
            seen = {}
        address = h5py.h5o.get_info(source.id).addr
        if address in seen:
            parent[name] = seen[address]
            return
        track_order = False
        if isinstance(source, (h5py.Group, h5py.Dataset)):
            properties = source.id.get_create_plist()
            track_order = bool(properties.get_attr_creation_order())
            if not track_order:
                for key in source.attrs:
                    attribute = source.attrs.get_id(key)
                    if attribute.shape and math.prod(attribute.shape) * attribute.dtype.itemsize >= 64 * 1024:
                        track_order = True
                        break
        if isinstance(source, h5py.Group):
            target = parent.create_group(name, track_order=track_order)
            seen[address] = target
            self.copy_object_attrs(source, target)
            for key in source:
                link = source.get(key, getlink=True)
                if isinstance(link, h5py.HardLink):
                    self.copy_object(source[key], target, key, seen)
                else:
                    target[key] = link
            return
        if (isinstance(source, h5py.Dataset) and source.shape is not None
                and h5py.check_ref_dtype(source.dtype) is not None and source.file.id != parent.file.id):
            # Cross-file H5Ocopy without reference expansion writes null refs.
            target = parent.create_dataset(name, shape=source.shape, dtype=source.dtype, track_order=track_order)
            seen[address] = target
            self.copy_object_attrs(source, target)
            return
        if isinstance(source, h5py.Dataset) and source.shape is not None and source.dtype.fields:
            datatype = source.id.get_type()
            if (not self.supports_compound(source.dtype)
                    and datatype.detect_class(h5py.h5t.REFERENCE)
                    and (datatype.detect_class(h5py.h5t.VLEN) or any(
                        h5py.check_string_dtype(dtype) is not None and dtype.hasobject
                        for _, dtype in self.compound_fields(source.dtype)
                    ))):
                properties = h5py.h5p.create(h5py.h5p.DATASET_CREATE)
                if track_order:
                    properties.set_attr_creation_order(h5py.h5p.CRT_ORDER_TRACKED | h5py.h5p.CRT_ORDER_INDEXED)
                target = h5py.Dataset(h5py.h5d.create(
                    parent.id, name.encode(), datatype, source.id.get_space(), dcpl=properties,
                ))
                seen[address] = target
                self.copy_compound(source, target, native=True)
                self.copy_object_attrs(source, target)
                return
        if (not isinstance(source, h5py.Dataset) or source.shape is None
                or (source.dtype.fields and not self.supports_compound(source.dtype))
                or (source.dtype.hasobject and not (is_budgeted_vlen(source.dtype)
                                                    or self.supports_compound(source.dtype)))):
            source.file.copy(source, parent, name=name)
            seen[address] = parent[name]
            return

        if source.dtype.fields:
            target = parent.create_dataset(name, shape=source.shape, dtype=source.dtype,
                                           compression='gzip' if source.ndim and all(source.shape) else None, track_order=track_order)
            self.copy_compound(source, target)
        elif source.ndim == 0:
            value = (read_bounded_vlen(source, (), decode=False) if is_budgeted_vlen(source.dtype)
                     else source[()])
            target = parent.create_dataset(name, shape=(), dtype=source.dtype, track_order=track_order)
            target[()] = value
        else:
            target = parent.create_dataset(name, shape=source.shape, dtype=source.dtype,
                                           compression='gzip' if all(source.shape) else None, track_order=track_order)
            if all(source.shape):
                if is_budgeted_vlen(source.dtype):
                    if source.ndim == 1:
                        step = min(VLEN_BLOCK_ROWS, max(1, self.budget // source.dtype.itemsize))
                        for start in range(0, len(source), step):
                            stop = min(start + step, len(source))
                            for offset, values in split_vlen_batch(
                                start, stop, lambda a, b: self.read(source, slice(a, b))
                            ):
                                self.write_block(target, (slice(offset, offset + len(values)),), values)
                    else:
                        self.copy_vlen_tiles(source, target, range(len(source)), None, lambda _: None)
                else:
                    remaining = max(1, self.budget // max(1, source.dtype.itemsize))
                    steps = []
                    for length in reversed(source.shape):
                        step = min(length, remaining)
                        steps.append(step)
                        remaining //= step
                    steps.reverse()
                    for starts in product(*(range(0, length, step) for length, step in zip(source.shape, steps))):
                        tile = tuple(slice(start, start + step) for start, step in zip(starts, steps))
                        target[tile] = self.read(source, tile)
        seen[address] = target
        self.copy_object_attrs(source, target)

    def copy_vlen_tiles(self, source, target, rows, columns, tick):
        """Tile all axes so even one wide row can fit the string read budget."""
        from itertools import product

        remaining = min(VLEN_BLOCK_ROWS, max(1, self.budget // source.dtype.itemsize))
        steps = []
        for length in reversed(target.shape):
            step = max(1, min(length, remaining))
            steps.append(step)
            remaining //= step
        steps.reverse()

        def write_tile(bounds):
            axes = [slice(start, stop) for start, stop in bounds]
            keys = [axes[0] if isinstance(rows, range) else rows[axes[0]], *axes[1:]]
            if columns is not None:
                keys[1] = columns[axes[1]]
            try:
                block = self.read(source, tuple(keys))
            except ValueError as error:
                if str(error) != 'vlen_budget_exceeded':
                    raise
                axis = max(range(len(bounds)), key=lambda i: bounds[i][1] - bounds[i][0])
                start, stop = bounds[axis]
                if stop - start <= 1:
                    raise
                middle = (start + stop) // 2
                for limit in ((start, middle), (middle, stop)):
                    write_tile(bounds[:axis] + [limit] + bounds[axis + 1:])
            else:
                self.write_block(target, axes, block)

        if not all(target.shape):
            return
        trailing = [range(0, length, step) for length, step in zip(target.shape[1:], steps[1:])]
        for row in range(0, len(rows), steps[0]):
            for offsets in product(*trailing):
                bounds = [(start, min(start + step, length))
                          for start, step, length in zip((row, *offsets), steps, target.shape)]
                write_tile(bounds)
            tick(min(row + steps[0], len(rows)) / len(rows))

    def copy(self, source, parent, name, rows, columns=None, tick=lambda value: None):
        import h5py
        import numpy as np
        if isinstance(source, h5py.Dataset):
            if not source.shape or source.shape[0] == 0:
                self.copy_object(source, parent, name)
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
                        self.write_block(target, (slice(offset, offset + len(block)),), block)
                    tick(stop / max(1, len(rows)))
            elif is_budgeted_vlen(source.dtype):
                self.copy_vlen_tiles(source, target, rows, columns, tick)
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
                self.copy_object(source[key], target, key)
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
                        self.copy_object(category.node, output, 'categories')
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
                else: copier.copy_object(original['raw'][key], group, key)
        if 'raw.X' in original:
            if 'raw' in result or 'raw.var' not in original: raise ValueError('unsupported_export_encoding')
            group = result.create_group('raw'); group.attrs['encoding-type'] = 'raw'; group.attrs['encoding-version'] = '0.1.0'
            raw_var = read_research_frame(original['raw.var'])
            if isinstance(raw_var, LegacyFrame):
                copier.legacy_frame(raw_var, group, 'var', range(len(raw_var)))
            else:
                copier.copy_object(original['raw.var'], group, 'var')
            if 'raw.varm' in original:
                if isinstance(original['raw.varm'], h5py.Dataset): copier.legacy_mapping(original['raw.varm'], group, 'varm', np.arange(len(raw_var)))
                else: copier.copy_object(original['raw.varm'], group, 'varm')
            tasks.append(('raw.X',rows,None))
        for key in original:
            if key in ('raw.X','raw.var','raw.varm'): continue
            if legacy and key == 'varm' and isinstance(original[key], h5py.Dataset):
                copier.legacy_mapping(original[key],result,key,np.arange(len(var_original))); continue
            if key not in ('X', 'obs', 'layers', 'obsm', 'obsp', 'raw') and not (legacy and key == 'var'):
                copier.copy_object(original[key], result, key)
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
                copier.copy_object(original['var'], result, 'var')
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
