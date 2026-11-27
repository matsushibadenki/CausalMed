"""Read-only research projection; expression and unused payloads stay in HDF5.

Annotation columns and indices can be projected in blocks. Some explicit
compatibility reads still materialize their requested data.
The interface deliberately covers only worker research reads, not AnnData editing.
"""
from collections.abc import Mapping
from functools import cached_property
from types import SimpleNamespace

if 'vlen_io' not in globals():
    from causalmed_worker import vlen_io
    from causalmed_worker.scratch import scratch_vector
    from causalmed_worker.vlen_io import (
        CATEGORY_LABEL_CHUNK_BYTES, VLEN_BLOCK_ROWS, _annotation_summary_read,
        read_bounded_vlen, split_vlen_batch, with_small_metadata_caches,
    )


class H5Mapping(Mapping):
    def __init__(self, group, matrices=False):
        self.group = group
        self.matrices = matrices

    def __iter__(self):
        return iter(self.group) if self.group is not None else iter(())

    def __len__(self):
        return len(self.group) if self.group is not None else 0

    def __contains__(self, key):
        return self.group is not None and key in self.group

    def __getitem__(self, key):
        if self.group is None:
            raise KeyError(key)
        node = self.group[key]
        if self.matrices:
            return backed_matrix(node)
        import h5py
        from anndata.io import read_elem
        if isinstance(node, h5py.Group) and node.attrs.get('encoding-type') == 'dict':
            return H5Mapping(node)
        if isinstance(node, h5py.Dataset) and node.ndim <= 1 and is_vlen_string(node.dtype):
            return read_bounded_vlen(node, slice(None))
        return read_elem(node)


def backed_matrix(node):
    import h5py
    from anndata.io import sparse_dataset
    return sparse_dataset(node) if isinstance(node, h5py.Group) else node


_NULLABLE_INDEX_ENCODINGS = ('nullable-integer', 'nullable-boolean', 'nullable-string-array')


class H5Frame:
    """Minimal read-only annotation frame; never retain decoded column payloads."""
    def __init__(self, group):
        import pandas as pd
        self.group = group
        self.validated_categories = set()
        key = self.index_key = group.attrs['_index']
        self.index_node = group[key]
        import h5py
        index_data = self.index_node
        if isinstance(index_data, h5py.Group):
            encoding = index_data.attrs.get('encoding-type', '')
            if encoding == 'categorical':
                index_data = index_data.get('codes')
            elif encoding in _NULLABLE_INDEX_ENCODINGS:
                index_data = index_data.get('values')
        self.length = (
            index_data.shape[0]
            if isinstance(index_data, h5py.Dataset) and index_data.ndim == 1
            else len(self.index)
        )
        self.columns = pd.Index(group.attrs['column-order'])
        if not self.columns.is_unique or key in self.columns:
            raise ValueError('invalid_h5ad')

    @cached_property
    def index(self):
        import pandas as pd
        from anndata.io import read_elem
        import h5py
        node = self.index_node
        values = (read_bounded_vlen(node, slice(None))
                  if isinstance(node, h5py.Dataset) and node.ndim == 1 and is_vlen_string(node.dtype)
                  else read_elem(node))
        return pd.Index(values, name=None if self.index_key == '_index' else self.index_key)

    def __len__(self):
        return self.length

    def __iter__(self):
        return iter(self.columns)

    def __contains__(self, key):
        return key in self.columns

    def is_categorical(self, key):
        return self.group[key].attrs.get('encoding-type') == 'categorical'

    def __getitem__(self, key):
        import pandas as pd
        from anndata.io import read_elem
        if key not in self:
            raise KeyError(key)
        import h5py
        node = self.group[key]
        values = (read_bounded_vlen(node, slice(None))
                  if isinstance(node, h5py.Dataset) and node.ndim == 1 and is_vlen_string(node.dtype)
                  else read_elem(node))
        if getattr(values, 'ndim', 1) != 1 or len(values) != len(self.index):
            raise ValueError('invalid_h5ad')
        return pd.Series(values, index=self.index, name=key, copy=False)


def read_annotation_values(node, selection):
    import h5py

    info = h5py.check_string_dtype(node.dtype)
    if info is not None and info.length is None:
        return read_bounded_vlen(node, selection)
    return node.asstr()[selection] if info is not None else node[selection]


def is_vlen_string(dtype):
    import h5py

    info = h5py.check_string_dtype(dtype)
    return info is not None and info.length is None


def is_budgeted_vlen(dtype):
    import h5py
    import numpy as np

    base = h5py.check_vlen_dtype(dtype)
    return is_vlen_string(dtype) or (isinstance(base, np.dtype) and base.kind in 'biufc')


def string_block_rows(dtype, rows):
    if is_budgeted_vlen(dtype):
        return min(rows, VLEN_BLOCK_ROWS)
    if _annotation_summary_read.get() and dtype.kind == 'S':
        return min(rows, max(1, vlen_io.ANNOTATION_SUMMARY_BYTES // dtype.itemsize))
    return rows


class CategoryDictionary:
    """Projected plain or nullable scalar categorical labels."""

    def __init__(self, node):
        import h5py

        self.node = node
        self.encoding = node.attrs.get('encoding-type', '')
        if isinstance(node, h5py.Dataset):
            self.values, self.mask = node, None
        elif isinstance(node, h5py.Group) and self.encoding in (
            'nullable-integer', 'nullable-boolean', 'nullable-string-array'
        ):
            self.values, self.mask = node['values'], node['mask']
        else:
            raise TypeError('unsupported category encoding')
        if self.values.ndim != 1 or (self.mask is not None and self.mask.shape != self.values.shape):
            raise ValueError('invalid_h5ad')
        self.dtype = self.values.dtype
        self.text = h5py.check_string_dtype(self.dtype) is not None
        if not self.text and self.dtype.kind not in 'biuf':
            raise TypeError('unsupported category dtype')

    def __len__(self):
        return len(self.values)

    def __getitem__(self, selection):
        import numpy as np
        import pandas as pd

        values = read_annotation_values(self.values, selection)
        if self.mask is None:
            return values
        mask = self.mask[selection]
        if np.any(mask):
            raise ValueError('invalid_h5ad')
        if self.encoding == 'nullable-string-array':
            return pd.array(values, dtype=pd.StringDtype())
        constructor = pd.arrays.IntegerArray if self.encoding == 'nullable-integer' else pd.arrays.BooleanArray
        return constructor(values, mask)

    def _validation_values(self, selection):
        from codecs import decode, getincrementaldecoder

        import h5py
        import numpy as np

        if self.mask is not None and np.any(self.mask[selection]):
            raise ValueError('invalid_h5ad')
        encoding = h5py.check_string_dtype(self.dtype).encoding
        labels = read_bounded_vlen(self.values, selection, decode=False, borrow_text=True)
        # Validate the original text encoding without keeping a decoded label
        # alongside the bytes passed to SQLite.
        decoder_type = getincrementaldecoder(encoding)
        chunk_bytes = CATEGORY_LABEL_CHUNK_BYTES
        for label in labels:
            if len(label) <= chunk_bytes:
                decode(label, encoding)
                continue
            decoder = decoder_type()
            view = memoryview(label)
            for start in range(0, len(view), chunk_bytes):
                decoder.decode(view[start:start + chunk_bytes])
            decoder.decode(b'', final=True)
        return labels

    def blocks(self, *, raw_text=False):
        step = string_block_rows(self.dtype, 8192)
        if not raw_text:
            for start in range(0, len(self), step):
                yield from split_vlen_batch(
                    start, min(start + step, len(self)), lambda a, b: self[slice(a, b)]
                )
            return

        # Probe the first label before grouping reads. Once a batch fails,
        # shrink subsequent reads instead of retrying each sibling range.
        start, batch, largest = 0, 1, 1
        budget = min(vlen_io.ANNOTATION_SUMMARY_BYTES, vlen_io.VLEN_BYTE_LIMIT)
        while start < len(self):
            stop = min(start + batch, len(self))
            try:
                values = self._validation_values(slice(start, stop))
            except ValueError as error:
                if str(error) != 'vlen_budget_exceeded' or stop - start == 1:
                    raise
                step = batch = max(1, (stop - start) // 2)
                continue
            largest = max(largest, max(len(label) + 1 for label in values))
            yield start, values
            del values
            start = stop
            batch = min(step, max(1, budget // largest))

    def _validate_vlen_labels(self):
        from contextlib import closing
        from hashlib import sha256
        import sqlite3
        from tempfile import TemporaryFile

        duplicate = False
        with closing(sqlite3.connect('')) as database, TemporaryFile() as payloads:
            database.execute('PRAGMA cache_size=-1024')
            database.execute(
                'CREATE TABLE labels (digest BLOB, length INTEGER, offset INTEGER, '
                'PRIMARY KEY (digest, length, offset)) WITHOUT ROWID'
            )
            database.execute('CREATE TABLE short_labels (label BLOB PRIMARY KEY) WITHOUT ROWID')
            buffer = memoryview(bytearray(CATEGORY_LABEL_CHUNK_BYTES))
            for _, block in _bounded_annotation_blocks(self.blocks(raw_text=True)):
                for label in block:
                    if len(label) <= CATEGORY_LABEL_CHUNK_BYTES:
                        inserted = database.execute(
                            'INSERT OR IGNORE INTO short_labels VALUES (?)', (label,)
                        )
                        duplicate |= inserted.rowcount == 0
                        continue
                    digest = sha256(label).digest()
                    view = memoryview(label)
                    matches = database.execute(
                        'SELECT offset FROM labels WHERE digest=? AND length=?', (digest, len(view))
                    )
                    # Spool distinct long labels once. Compare hash candidates
                    # in bounded chunks without reloading a second HDF5 payload.
                    for (offset,) in matches:
                        payloads.seek(offset)
                        for start in range(0, len(view), len(buffer)):
                            expected = view[start:start + len(buffer)]
                            actual = buffer[:len(expected)]
                            if payloads.readinto(actual) != len(actual):
                                raise OSError('short category validation read')
                            same = actual == expected
                            del expected
                            if not same:
                                break
                        else:
                            duplicate = True
                            break
                    else:
                        offset = payloads.seek(0, 2)
                        payloads.write(view)
                        database.execute('INSERT INTO labels VALUES (?,?,?)', (digest, len(view), offset))
                    del view
                    matches.close()
                # The loop variable otherwise pins the final native buffer.
                if len(block):
                    del label
                del block
        if duplicate:
            raise ValueError('invalid_h5ad')

    def validate(self):
        import numpy as np

        if is_vlen_string(self.dtype):
            with_small_metadata_caches((self.values.file,), self._validate_vlen_labels)
            return
        blocks = (values for _, values in _bounded_annotation_blocks(self.blocks()))
        keys = blocks if self.text else (_numeric_category_keys(np.asarray(values, dtype=self.dtype)) for values in blocks)
        if _distinct_labels(keys) != len(self):
            raise ValueError('invalid_h5ad')


def projected_category_dictionary(node):
    try:
        return CategoryDictionary(node)
    except TypeError:
        return None


class LegacyFrame:
    """Field projection for a pre-0.7 compound annotation dataset."""

    def __init__(self, dataset):
        import pandas as pd

        self.dataset = dataset
        self.index_key, *columns = dataset.dtype.names
        self.columns = pd.Index(columns)
        self.categories = {}
        self.category_nullable = {}

    def __len__(self):
        return len(self.dataset)

    def __iter__(self):
        return iter(self.columns)

    def __contains__(self, key):
        return key in self.columns

    def is_categorical(self, key):
        return key in self.categories

    def dtype(self, key):
        return self.dataset.dtype.fields[key][0]

    def read(self, key, selection):
        from anndata._io.h5ad import _decode_structured_array, _from_fixed_length_strings

        import h5py
        info = h5py.check_string_dtype(self.dtype(key))
        if info is not None and info.length is None:
            return read_bounded_vlen(self.dataset, selection, field=key, decode=info.encoding == 'utf-8')
        field = self.dataset.fields([key])[selection]
        return _decode_structured_array(_from_fixed_length_strings(field), dtype=field.dtype)[key]

    def column(self, key, selection, include_index=True, *, project_categories=False):
        import pandas as pd

        values = self.read(key, selection)
        if key in self.categories:
            dictionary = self.categories[key]
            if isinstance(dictionary, CategoryDictionary):
                if project_categories:
                    values = _categorical_block(values, dictionary, len(dictionary), False)
                else:
                    values = pd.Categorical.from_codes(values, categories=dictionary[:])
            else:
                values = pd.Categorical.from_codes(values, dtype=dictionary)
        index = self.read_index(selection) if include_index else None
        return pd.Series(values, index=index, name=key, copy=False)

    def read_index(self, selection):
        import pandas as pd

        return pd.Index(self.read(self.index_key, selection), name=self.index_key)

    @cached_property
    def index(self):
        return self.read_index(slice(None))

    def __getitem__(self, key):
        if key not in self:
            raise KeyError(key)
        return self.column(key, slice(None))


def iter_annotation_column(frame, key, block_rows=8192, *, include_index=True):
    """Yield (source offset, Series) without expanding a whole modern column.

    Projected dictionaries load only referenced labels per block. The native
    string payload allocation has a separate byte budget.
    """
    if block_rows < 1:
        raise ValueError('invalid_h5ad')
    if isinstance(frame, LegacyFrame):
        step = string_block_rows(frame.dtype(key), block_rows)
        if include_index:
            step = string_block_rows(frame.dtype(frame.index_key), step)
        dictionary = frame.categories.get(key)
        if isinstance(dictionary, CategoryDictionary):
            step = string_block_rows(dictionary.dtype, step)
        for start in range(0, len(frame), step):
            yield from split_vlen_batch(
                start, min(start + step, len(frame)),
                lambda a, b: frame.column(key, slice(a, b), include_index, project_categories=True),
            )
        return
    if not isinstance(frame, H5Frame):
        for start in range(0, len(frame), block_rows):
            yield start, frame[key].iloc[start:start + block_rows]
        return
    if key not in frame:
        raise KeyError(key)
    yield from _iter_h5_column(frame, frame.group[key], key, block_rows, include_index)


def _iter_h5_column(frame, node, key, block_rows, include_index):
    import h5py
    import pandas as pd
    from anndata.io import read_elem

    encoding = node.attrs.get('encoding-type', '')
    dtype = None
    categories = None
    if isinstance(node, h5py.Dataset):
        children = [node]
    elif encoding == 'categorical':
        dictionary = node['categories']
        projected = projected_category_dictionary(dictionary)
        needs_projection = projected is not None and (
            projected.mask is not None or len(projected) > string_block_rows(projected.dtype, 8192)
        )
        if not needs_projection:
            try:
                labels = projected[:] if projected is not None else read_elem(dictionary)
                dtype = pd.CategoricalDtype(labels, ordered=bool(node.attrs.get('ordered', False)))
            except ValueError as error:
                if projected is None or str(error) != 'vlen_budget_exceeded':
                    raise
                needs_projection = True
        if needs_projection:
            categories = projected
            if key not in frame.validated_categories:
                categories.validate()
                frame.validated_categories.add(key)
        children = [node['codes']]
    elif encoding in ('nullable-integer', 'nullable-boolean', 'nullable-string-array'):
        children = [node['values'], node['mask']]
    else:
        raise ValueError('invalid_h5ad')
    if any(not isinstance(child, h5py.Dataset) or child.shape != (len(frame),) for child in children):
        raise ValueError('invalid_h5ad')
    block_rows = string_block_rows(children[0].dtype, block_rows)
    if include_index and isinstance(frame.index_node, h5py.Dataset):
        block_rows = string_block_rows(frame.index_node.dtype, block_rows)
    if categories is not None:
        block_rows = string_block_rows(categories.dtype, block_rows)
    def read_block(start, stop):
        values = read_annotation_values(children[0], slice(start, stop))
        if categories is not None:
            values = _categorical_block(values, categories, len(categories), bool(node.attrs.get('ordered', False)))
        elif encoding == 'categorical':
            values = pd.Categorical.from_codes(values, dtype=dtype)
        elif encoding in ('nullable-integer', 'nullable-boolean'):
            constructor = pd.arrays.IntegerArray if encoding == 'nullable-integer' else pd.arrays.BooleanArray
            values = constructor(values, children[1][start:stop])
        elif encoding == 'nullable-string-array':
            values = pd.array(values, dtype=pd.StringDtype())
            values[children[1][start:stop]] = pd.NA
        if not include_index:
            index = None
        elif ('index' not in vars(frame) and isinstance(frame.index_node, h5py.Dataset)
              and h5py.check_string_dtype(frame.index_node.dtype)):
            index = pd.Index(read_annotation_values(frame.index_node, slice(start, stop)),
                             name=None if frame.index_key == '_index' else frame.index_key)
        else:
            index = frame.index[start:stop]
        return pd.Series(values, index=index, name=key, copy=False)

    for start in range(0, len(frame), block_rows):
        yield from split_vlen_batch(start, min(start + block_rows, len(frame)), read_block)



def _categorical_block(codes, categories, cardinality, ordered):
    import numpy as np
    import pandas as pd

    if not np.issubdtype(codes.dtype, np.integer) or np.any(codes < -1) or np.any(codes >= cardinality):
        raise ValueError('invalid_h5ad')
    present = codes >= 0
    referenced, inverse = np.unique(codes[present], return_inverse=True)
    local_codes = np.full(len(codes), -1, dtype=np.intp)
    local_codes[present] = inverse
    labels = categories[referenced] if referenced.size else []
    return pd.Categorical.from_codes(local_codes, categories=labels, ordered=ordered)


def read_research_frame(node):
    import h5py
    from anndata._io.h5ad import read_dataframe
    if isinstance(node, h5py.Group) and node.attrs.get('encoding-type') == 'dataframe':
        return H5Frame(node)
    if isinstance(node, h5py.Dataset) and node.ndim == 1 and node.dtype.names:
        return LegacyFrame(node)
    return read_dataframe(node)


def categorical_columns(frame):
    import pandas as pd
    for key in frame:
        if (frame.is_categorical(key) if isinstance(frame, (H5Frame, LegacyFrame))
                else isinstance(frame[key].dtype, pd.CategoricalDtype)):
            yield key


def configure_legacy_categories(obs, var, handle, *, exporting=False):
    """Associate pre-0.7 code fields with their matching uns dictionary."""
    import numpy as np
    import pandas as pd
    from anndata.io import read_elem
    from anndata._io.h5ad import read_dataframe

    converted = set()
    for key in handle.get('uns', {}):
        if not key.endswith('_categories'):
            continue
        label = key[:-11]
        if isinstance(var, H5Frame) and label in var:
            var = read_dataframe(handle['var'])
        for frame in (obs, var):
            if label not in frame:
                continue
            dtype = frame.dtype(label) if isinstance(frame, LegacyFrame) else frame[label].dtype
            if not pd.api.types.is_integer_dtype(dtype):
                continue
            dictionary = handle['uns'][key]
            categories = projected_category_dictionary(dictionary) if isinstance(frame, LegacyFrame) else None
            if categories is None or (not is_vlen_string(categories.dtype) and categories.mask is None
                                      and len(categories) <= 8192):
                if categories is not None:
                    categories = categories[:]
                else:
                    categories = read_elem(dictionary)
                    categories = [categories] if np.isscalar(categories) else categories
            if isinstance(frame, LegacyFrame):
                blocks = (frame.read(label, slice(start, start + 8192))
                          for start in range(0, len(frame), 8192))
            else:
                blocks = (block.to_numpy() for _, block in iter_annotation_column(frame, label))
            below_range = above_range = nullable = False
            for codes in blocks:
                below_range |= bool(np.any(codes < -1))
                above_range |= bool(np.any(codes >= len(categories)))
                if isinstance(frame, LegacyFrame) and not nullable:
                    nullable = bool(np.any(codes == -1))
            if above_range or (below_range and not exporting):
                continue
            if below_range:
                raise ValueError('invalid_h5ad')
            if isinstance(frame, LegacyFrame):
                if isinstance(categories, CategoryDictionary):
                    categories.validate()
                    frame.categories[label] = categories
                else:
                    frame.categories[label] = pd.CategoricalDtype(categories)
                frame.category_nullable[label] = nullable
            else:
                frame[label] = pd.Categorical.from_codes(frame[label].to_numpy(), categories)
            converted.add(key)
    return var, converted


def open_research_h5ad(path):
    import h5py
    handle = h5py.File(path, 'r')
    try:
        obs, var = read_research_frame(handle['obs']), read_research_frame(handle['var'])
        if isinstance(obs, LegacyFrame):
            var, _ = configure_legacy_categories(obs, var, handle)
        raw = None
        if 'raw/var' in handle or 'raw.var' in handle:
            prefix = 'raw/' if 'raw/var' in handle else 'raw.'
            raw = SimpleNamespace(var=read_research_frame(handle[prefix + 'var']),
                                  X=backed_matrix(handle[prefix + 'X']) if prefix + 'X' in handle else None)
        return SimpleNamespace(obs=obs, var=var,
                               n_obs=len(obs), n_vars=len(var), raw=raw,
                               X=backed_matrix(handle['X']) if 'X' in handle else None,
                               layers=H5Mapping(handle.get('layers'), matrices=True),
                               uns=H5Mapping(handle.get('uns')),
                               file=SimpleNamespace(close=handle.close, _file=handle))
    except BaseException:
        handle.close()
        raise


def annotation_schema(frame, key, block_rows=8192):
    """Inspect dtype and nullability without expanding categorical labels."""
    import pandas as pd

    if isinstance(frame, LegacyFrame) and key in frame.category_nullable:
        if block_rows < 1:
            raise ValueError('invalid_h5ad')
        return {
            'name': key, 'dtype': 'category', 'kind': 'categorical',
            'nullable': frame.category_nullable[key],
        }

    if isinstance(frame, H5Frame) and key in frame and frame.is_categorical(key):
        node = frame.group[key]
        dictionary = projected_category_dictionary(node['categories'])
        if dictionary is not None:
            nullable = False
            for codes in _validated_category_codes(frame, key, node, dictionary, block_rows):
                nullable |= bool((codes == -1).any())
            return {'name': key, 'dtype': 'category', 'kind': 'categorical', 'nullable': nullable}

    dtype = None
    nullable = False
    for _, block in iter_annotation_column(frame, key, block_rows, include_index=False):
        dtype = block.dtype
        nullable |= bool(block.isna().any())
    if dtype is None:
        dtype = frame[key].dtype  # Empty frames only; no row allocation.
    kind = ('categorical' if isinstance(dtype, pd.CategoricalDtype) else
            'boolean' if pd.api.types.is_bool_dtype(dtype) else
            'numeric' if pd.api.types.is_numeric_dtype(dtype) else
            'string' if pd.api.types.is_string_dtype(dtype) else 'unknown')
    return {'name': key, 'dtype': str(dtype)[:100], 'kind': kind, 'nullable': nullable}


def _bounded_annotation_blocks(blocks):
    """Apply the smaller annotation budget only while advancing a read."""
    while True:
        token = _annotation_summary_read.set(True)
        try:
            block = next(blocks, None)
        finally:
            _annotation_summary_read.reset(token)
        if block is None:
            return
        yield block
        del block


def _categorical_code_blocks(frame, key, block_rows):
    if block_rows < 1:
        raise ValueError('invalid_h5ad')
    if isinstance(frame, LegacyFrame) and frame.is_categorical(key):
        category = frame.categories[key]
        dictionary = category if isinstance(category, CategoryDictionary) else category.categories
        # configure_legacy_categories already checked the full code range.
        return dictionary, (
            frame.read(key, slice(start, start + block_rows))
            for start in range(0, len(frame), block_rows)
        )
    if isinstance(frame, H5Frame) and key in frame and frame.is_categorical(key):
        node = frame.group[key]
        dictionary = projected_category_dictionary(node['categories'])
        if dictionary is not None:
            return dictionary, _validated_category_codes(frame, key, node, dictionary, block_rows)
    return None


def iter_annotation_matches(frame, key, values, block_rows=8192):
    """Yield exact string-match masks, excluding missing values."""
    yield from iter_annotation_predicate(frame, key, lambda labels: labels.isin(values), block_rows)


def iter_annotation_predicate(frame, key, predicate, block_rows=8192, *, missing=False):
    """Apply a string predicate to dictionary entries or bounded row blocks."""
    from itertools import chain
    import numpy as np
    import pandas as pd

    categorical = _categorical_code_blocks(frame, key, block_rows)
    if categorical is None:
        for start, block in iter_annotation_column(frame, key, block_rows, include_index=False):
            yield start, predicate(block.astype('string')).to_numpy(dtype=bool, na_value=missing)
        return

    dictionary, blocks = categorical
    first = next(blocks, None)
    if first is None:
        return
    matches = scratch_vector(len(dictionary) + 1, np.bool_)
    matches[0] = missing

    def label_batches():
        step = string_block_rows(dictionary.dtype, block_rows)
        for start in range(0, len(dictionary), step):
            yield from split_vlen_batch(start, min(start + step, len(dictionary)),
                                        lambda a, b: dictionary[a:b])

    for offset, labels in _bounded_annotation_blocks(label_batches()):
        strings = pd.Series(pd.Categorical(labels), copy=False).astype('string')
        matches[offset + 1:offset + 1 + len(labels)] = predicate(strings).to_numpy(
            dtype=bool, na_value=missing
        )
        del labels, strings

    start = 0
    for codes in chain((first,), blocks):
        yield start, matches[codes.astype(np.intp, copy=False) + 1]
        start += len(codes)


def categorical_group_rows(frame, key, block_rows, retained_mask=None):
    """Return bounded categorical counts, or None for the compatibility path."""
    categorical = _categorical_code_blocks(frame, key, block_rows)
    if categorical is None:
        return None
    dictionary, blocks = categorical
    return _bounded_annotation_blocks(
        _categorical_group_rows(blocks, dictionary, block_rows, retained_mask)
    )


def _categorical_group_rows(blocks, dictionary, block_rows, retained_mask):
    """Aggregate codes before decoding each referenced label once."""
    import numpy as np
    import pandas as pd

    counts = scratch_vector(len(dictionary) + 1, np.int64)
    first = scratch_vector(len(dictionary) + 1, np.int64)
    retained = scratch_vector(len(dictionary) + 1, np.int64) if retained_mask is not None else None
    offset = 0
    for block in blocks:
        codes, indices, frequencies = np.unique(block, return_index=True, return_counts=True)
        slots = codes.astype(np.intp, copy=False) + 1
        unseen = counts[slots] == 0
        first[slots[unseen]] = offset + indices[unseen]
        counts[slots] += frequencies
        if retained is not None:
            codes, frequencies = np.unique(
                block[retained_mask[offset:offset + len(block)]], return_counts=True
            )
            retained[codes.astype(np.intp, copy=False) + 1] += frequencies
        offset += len(block)
    yield None, int(counts[0]), 0, int(retained[0]) if retained is not None else 0

    step = string_block_rows(dictionary.dtype, block_rows)
    for start in range(0, len(dictionary), step):
        slots = np.flatnonzero(counts[start + 1:start + step + 1]) + start + 1
        if not slots.size:
            continue
        for offset, labels in split_vlen_batch(
            0, len(slots), lambda a, b: dictionary[slots[a:b] - 1]
        ):
            strings = pd.Series(pd.Categorical(labels), copy=False).astype('string')
            for slot, label in zip(slots[offset:offset + len(labels)], strings):
                yield str(label), int(counts[slot]), int(first[slot]), int(retained[slot]) if retained is not None else 0
            del labels, strings, label


def annotation_group(frame, key, block_rows=8192):
    """Exact disk-backed counts with a cardinality-independent frequency cache.

    SQLite has a 1 MiB page cache. Projected string reads use a smaller
    multi-value payload budget. Individual labels retain the normal VLEN limit.
    Equal-frequency labels use deterministic first-source-occurrence order;
    pandas' default unstable frequency sort is deliberately not reproduced. No SQL text is constructed from source annotation values.
    """
    import sqlite3
    from contextlib import closing
    missing = 0
    labels_fit = True
    # An empty filename asks SQLite for its private temporary on-disk database.
    # No named application artifact is left behind after the worker exits.
    with closing(sqlite3.connect('')) as database:
        database.execute('PRAGMA cache_size=-1024')
        database.execute('PRAGMA temp_store=FILE')
        database.execute('CREATE TABLE counts (label TEXT PRIMARY KEY, count INTEGER NOT NULL, first_row INTEGER NOT NULL) WITHOUT ROWID')
        def count_rows():
            nonlocal missing, labels_fit
            rows = categorical_group_rows(frame, key, block_rows)
            if rows is not None:
                for label, count, first_row, _ in rows:
                    if label is None:
                        missing = count
                    else:
                        if labels_fit:
                            labels_fit = len(label) <= 200 and len(label.encode('utf-8')) <= 200
                        yield label, count, first_row
                return
            for start, block in _bounded_annotation_blocks(
                iter_annotation_column(frame, key, block_rows, include_index=False)
            ):
                strings = block.astype('string')
                missing += int(strings.isna().sum())
                counts = strings.dropna().value_counts(sort=False)
                for ordinal, (label, count) in enumerate(counts.items()):
                    label = str(label)
                    if labels_fit:
                        labels_fit = len(label) <= 200 and len(label.encode('utf-8')) <= 200
                    yield label, int(count), start + ordinal
                    del label
                del block, strings, counts

        database.executemany(
            'INSERT INTO counts VALUES (?,?,?) ON CONFLICT(label) DO UPDATE SET count=count+excluded.count, first_row=min(first_row,excluded.first_row)',
            count_rows(),
        )
        distinct, total = database.execute('SELECT count(*), coalesce(sum(count),0) FROM counts').fetchone()
        # UTF-8 needs at most four bytes per character. Slice the BLOB to
        # preserve embedded NULs and drop any partial character at the end.
        top = database.execute(
            'SELECT substr(CAST(label AS BLOB),1,800),count '
            'FROM counts ORDER BY count DESC,first_row ASC LIMIT 20'
        ).fetchall()
    return {'distinct': distinct, 'missing': missing,
            'complete': distinct <= 20 and labels_fit,
            'other': total - sum(count for _, count in top),
            'counts': [{'label': label.decode('utf-8', errors='ignore')[:200], 'count': count}
                       for label, count in top]}


def _supports_grouped_index(node):
    encoding = node.attrs.get('encoding-type', '')
    return encoding in _NULLABLE_INDEX_ENCODINGS or (
        encoding == 'categorical' and projected_category_dictionary(node['categories']) is not None
    )


def iter_frame_index(frame, block_rows=8192):
    import h5py
    import pandas as pd

    if isinstance(frame, LegacyFrame):
        step = string_block_rows(frame.dtype(frame.index_key), block_rows)
        for start in range(0, len(frame), step):
            for offset, values in split_vlen_batch(
                start, min(start + step, len(frame)), lambda a, b: frame.read_index(slice(a, b))
            ):
                yield offset, pd.Series(values, copy=False)
        return

    node = frame.index_node if isinstance(frame, H5Frame) else None
    if isinstance(node, h5py.Dataset) and node.ndim == 1:
        block_rows = string_block_rows(node.dtype, block_rows)
        for start in range(0, len(frame), block_rows):
            for offset, values in split_vlen_batch(
                start, min(start + block_rows, len(frame)), lambda a, b: read_annotation_values(node, slice(a, b))
            ):
                yield offset, pd.Series(values, copy=False)
    elif isinstance(node, h5py.Group) and _supports_grouped_index(node):
        yield from _iter_h5_column(frame, node, frame.index_key, block_rows, False)
    else:
        for start in range(0, len(frame), block_rows):
            yield start, pd.Series(frame.index[start:start + block_rows], copy=False)


def _numeric_category_keys(values, *, allow_nan=False):
    """Encode exact numeric keys, normalizing zero and NaN components."""
    import numpy as np

    if values.dtype.kind in 'fc':
        values = values.copy()
        components = (values.real, values.imag) if values.dtype.kind == 'c' else (values,)
        for component in components:
            missing = np.isnan(component)
            if missing.any() and not allow_nan:
                raise ValueError('invalid_h5ad')
            component[component == 0] = 0
            component[missing] = np.nan
    return (value.tobytes() for value in values)


def _distinct_labels(blocks):
    """Count exact distinct labels with a local disk-backed set."""
    import sqlite3
    from contextlib import closing

    with closing(sqlite3.connect('')) as database:
        database.execute('PRAGMA cache_size=-1024')
        database.execute('CREATE TABLE labels (label TEXT PRIMARY KEY) WITHOUT ROWID')
        for block in blocks:
            database.executemany('INSERT OR IGNORE INTO labels VALUES (?)', ((label,) for label in block))
        return database.execute('SELECT count(*) FROM labels').fetchone()[0]


def _encoded_index_keys(block):
    """Normalize the scalar values of a grouped index for exact SQLite keys."""
    import numpy as np
    import pandas as pd

    for value in block:
        if pd.isna(value):
            yield b''
        elif isinstance(value, (float, np.floating)):
            yield np.format_float_scientific(value if value != 0 else 0.0, unique=True)
        else:
            yield str(value)


def _validated_category_codes(frame, key, node, dictionary, block_rows):
    import h5py
    import numpy as np

    if block_rows < 1:
        raise ValueError('invalid_h5ad')
    codes = node['codes']
    if (not isinstance(codes, h5py.Dataset) or codes.shape != (len(frame),)
            or not np.issubdtype(codes.dtype, np.integer)):
        raise ValueError('invalid_h5ad')
    if key not in frame.validated_categories:
        dictionary.validate()
        frame.validated_categories.add(key)
    for start in range(0, len(frame), block_rows):
        block = codes[start:start + block_rows]
        if np.any(block < -1) or np.any(block >= len(dictionary)):
            raise ValueError('invalid_h5ad')
        yield block


def _categorical_index_stats(frame, dictionary, measure_width):
    """Count seen category codes directly, reserving slot zero for missing IDs."""
    import numpy as np
    import pandas as pd

    seen = None
    distinct = 0
    mixed_missing = False
    projected_width = measure_width and dictionary.mask is not None
    block_rows = string_block_rows(dictionary.dtype, 8192) if projected_width else 8192
    for block in _validated_category_codes(frame, frame.index_key, frame.index_node, dictionary, block_rows):
        if seen is None:
            seen = scratch_vector(len(dictionary) + 1, np.bool_)
        slots = np.unique(block).astype(np.intp, copy=False) + 1
        distinct += int(np.count_nonzero(~seen[slots]))
        seen[slots] = True
        if projected_width and len(slots) > 1 and slots[0] == 0:
            mixed_missing = True
    width = None
    if measure_width:
        # Missing-only projected blocks have an empty category index and yield NaN.
        width = 3 if seen is not None and seen[0] else 1
        if seen is not None:
            for _, labels in _bounded_annotation_blocks(_seen_index_labels(dictionary, seen)):
                categories = pd.Index(labels)
                width = max(width, max(len(str(label)) for label in categories))
                if mixed_missing:
                    # Short strings cannot trigger the byte-budget split that
                    # could otherwise separate missing codes from present labels.
                    missing = pd.Categorical.from_codes([-1], categories=categories)
                    width = max(width, len(str(next(iter(missing)))))
                    mixed_missing = False
                del labels
                del categories
    return len(frame) - distinct, width


def _seen_index_labels(dictionary, seen):
    import numpy as np

    step = string_block_rows(dictionary.dtype, 8192)
    for start in range(0, len(dictionary), step):
        codes = np.flatnonzero(seen[start + 1:start + step + 1]) + start
        if codes.size:
            yield from split_vlen_batch(0, len(codes), lambda a, b: dictionary[codes[a:b]])


def text_block_width(block):
    import numpy as np

    if block.dtype.kind in ('O', 'U', 'T'):
        return max((len(str(value)) for value in block), default=0)
    return np.asarray(block, dtype=str).dtype.itemsize // 4


def duplicate_frame_ids(frame):
    return frame_id_stats(frame)[0]


def frame_id_stats(frame, *, measure_width=False):
    """Count duplicates and optionally measure text width during the same read."""
    import h5py
    import numpy as np

    def streamed_stats(encode):
        width = 1
        def keys():
            nonlocal width
            for _, block in iter_frame_index(frame):
                if measure_width:
                    width = max(width, text_block_width(block))
                yield encode(block)

        distinct = _distinct_labels(keys())
        return len(frame) - distinct, width if measure_width else None

    if isinstance(frame, LegacyFrame):
        dtype = frame.dtype(frame.index_key)
    elif isinstance(frame, H5Frame) and isinstance(frame.index_node, h5py.Dataset) and frame.index_node.ndim == 1:
        dtype = frame.index_node.dtype
    elif isinstance(frame, H5Frame) and isinstance(frame.index_node, h5py.Group):
        node = frame.index_node
        if node.attrs.get('encoding-type') == 'categorical':
            dictionary = projected_category_dictionary(node['categories'])
            if dictionary is not None:
                return _categorical_index_stats(frame, dictionary, measure_width)
        if node.attrs.get('encoding-type') not in _NULLABLE_INDEX_ENCODINGS:
            return int(frame.index.duplicated().sum()), None
        return streamed_stats(_encoded_index_keys)
    else:
        return int(frame.index.duplicated().sum()), None

    if h5py.check_string_dtype(dtype):
        return streamed_stats(lambda block: block)
    if dtype.kind in 'biufc':
        return streamed_stats(
            lambda block: _numeric_category_keys(np.asarray(block, dtype=dtype), allow_nan=True)
        )
    return int(frame.index.duplicated().sum()), None


def index_matches(frame, expected, rows=None, block_rows=8192):
    import numpy as np
    import pandas as pd

    if len(frame) != (len(expected) if rows is None else len(rows)):
        return False
    for start, block in iter_frame_index(frame, block_rows):
        selection = slice(start, start + len(block))
        if rows is not None:
            selection = rows[selection]
        dtype = str
        if isinstance(block.dtype, pd.CategoricalDtype):
            # NumPy may cast integer categories with missing codes through
            # floats when inferring width. Use the saved NPZ conversion.
            dtype = expected.dtype
            if text_block_width(block) > dtype.itemsize // 4:
                return False
        if not np.array_equal(np.asarray(block, dtype=dtype), expected[selection]):
            return False
    return True
