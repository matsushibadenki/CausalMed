"""Read-only research projection; expression and unused payloads stay in HDF5.

Annotation columns and indices can be projected in blocks. Some explicit
compatibility reads still materialize their requested data.
The interface deliberately covers only worker research reads, not AnnData editing.
"""
from collections.abc import Mapping
from functools import cached_property, lru_cache
from types import SimpleNamespace


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


class H5Frame:
    """Minimal read-only annotation frame; never retain decoded column payloads."""
    def __init__(self, group):
        import pandas as pd
        self.group = group
        self.validated_categories = set()
        key = self.index_key = group.attrs['_index']
        self.index_node = group[key]
        import h5py
        self.length = self.index_node.shape[0] if isinstance(self.index_node, h5py.Dataset) and self.index_node.ndim == 1 else len(self.index)
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


VLEN_BLOCK_ROWS = 256


VLEN_BYTE_LIMIT = 8 * 1024 * 1024


def vlen_byte_limit():
    return VLEN_BYTE_LIMIT


@lru_cache(maxsize=1)
def _vlen_hdf5():
    """Resolve symbols from the same HDF5 library that owns the h5py IDs."""
    import ctypes
    import h5py

    library = ctypes.CDLL(h5py.h5d.__file__, use_errno=True)
    allocate = ctypes.CFUNCTYPE(ctypes.c_void_p, ctypes.c_size_t, ctypes.c_void_p)
    release = ctypes.CFUNCTYPE(None, ctypes.c_void_p, ctypes.c_void_p)
    hid = ctypes.c_int64
    library.H5Pset_vlen_mem_manager.argtypes = [hid, allocate, ctypes.c_void_p, release, ctypes.c_void_p]
    library.H5Pset_vlen_mem_manager.restype = ctypes.c_int
    for name in ('H5Dread', 'H5Dwrite'):
        function = getattr(library, name)
        function.argtypes = [hid, hid, hid, hid, hid, ctypes.c_void_p]
        function.restype = ctypes.c_int
    for name in ('H5Aread', 'H5Awrite'):
        function = getattr(library, name)
        function.argtypes = [hid, hid, ctypes.c_void_p]
        function.restype = ctypes.c_int
    library.reclaim_vlen = getattr(library, 'H5Treclaim', None) or library.H5Dvlen_reclaim
    library.reclaim_vlen.argtypes = [hid, hid, hid, ctypes.c_void_p]
    library.reclaim_vlen.restype = ctypes.c_int
    return library, allocate, release


def read_bounded_vlen(node, selection, *, field=None, decode=True, byte_limit=None):
    """Read budgeted VLEN values or project a fixed-width compound leaf."""
    import ctypes
    import h5py
    import numpy as np
    from h5py._objects import phil
    from math import prod

    path = (field,) if isinstance(field, str) else field or ()
    dtype = node.dtype
    array_shape = ()
    for component in path:
        if isinstance(component, str):
            dtype = dtype.fields[component][0]
        else:
            dtype, shape = dtype.subdtype
            if shape != component:
                raise ValueError('invalid_h5ad')
            array_shape += shape
    info = h5py.check_string_dtype(dtype)
    base = h5py.check_vlen_dtype(dtype)
    numeric = isinstance(base, np.dtype) and base.kind in 'biufc'
    fixed = bool(path) and not dtype.hasobject and not numeric and (info is None or info.length is not None)
    if not fixed and not numeric and (info is None or info.length is not None):
        raise ValueError('invalid_h5ad')

    class VlenValue(ctypes.Structure):
        _fields_ = [('length', ctypes.c_size_t), ('address', ctypes.c_void_p)]

    element = (ctypes.c_char * dtype.itemsize) if fixed else VlenValue if numeric else ctypes.c_void_p
    byte_limit = VLEN_BYTE_LIMIT if byte_limit is None else byte_limit
    library, allocate_type, release_type = _vlen_hdf5()
    owned = {}
    allocated = 0
    exceeded = False
    out_of_memory = False

    @allocate_type
    def allocate(size, _):
        nonlocal allocated, exceeded, out_of_memory
        if allocated + size > byte_limit:
            exceeded = True
            return None
        try:
            buffer = ctypes.create_string_buffer(size)
            address = ctypes.addressof(buffer)
            owned[address] = buffer
            allocated += size
            return address
        except (MemoryError, OverflowError):
            out_of_memory = True
            return None

    @release_type
    def release(address, _):
        nonlocal allocated
        buffer = owned.pop(address, None)
        if buffer is not None:
            allocated -= ctypes.sizeof(buffer)

    with phil:
        file_type = node.id.get_type()
        memory_type = file_type
        for component in path:
            if isinstance(component, str):
                encoded = component.encode()
                memory_type = memory_type.get_member_type(memory_type.get_member_index(encoded))
            else:
                memory_type = memory_type.get_super()
        for component in reversed(path):
            if isinstance(component, str):
                compound = h5py.h5t.create(h5py.h5t.COMPOUND, memory_type.get_size())
                compound.insert(component.encode(), 0, memory_type)
                memory_type = compound
            else:
                memory_type = h5py.h5t.array_create(memory_type, component)
        file_space = node.id.get_space()
        output_shape = None
        if node.ndim > 1:
            from itertools import product

            if len(selection) != node.ndim:
                raise ValueError('invalid_h5ad')
            axes = [range(*key.indices(length)) if isinstance(key, slice) else key
                    for key, length in zip(selection, node.shape)]
            output_shape = tuple(map(len, axes))
            count = prod(output_shape)
            if count > VLEN_BLOCK_ROWS:
                raise ValueError('vlen_budget_exceeded')
            if count:
                file_space.select_elements(np.asarray(list(product(*axes)), dtype=np.uint64))
        elif node.ndim == 0:
            count = 1
        elif isinstance(selection, slice):
            start, stop, stride = selection.indices(len(node))
            count = len(range(start, stop, stride))
            if count:
                file_space.select_hyperslab((start,), (count,), (stride,))
        else:
            indices = np.asarray(selection, dtype=np.uint64)
            count = len(indices)
            if count:
                file_space.select_elements(indices.reshape(-1, 1))
        if not count:
            return np.empty(output_shape or (0,), dtype=object)
        memory_space = h5py.h5s.create_simple((count,))
        transfer = h5py.h5p.create(h5py.h5p.DATASET_XFER)
        result_shape = output_shape or (() if node.ndim == 0 else (count,))
        result_shape += array_shape
        if fixed:
            values = np.empty(result_shape, dtype=dtype)
            if library.H5Dread(node.id.id, memory_type.id, memory_space.id, file_space.id,
                               transfer.id, values.ctypes.data) < 0:
                raise ValueError('invalid_h5ad')
            return values
        element_count = count * prod(array_shape)
        if element_count * ctypes.sizeof(element) > VLEN_BYTE_LIMIT:
            raise ValueError('vlen_budget_exceeded')
        output = (element * element_count)()
        if library.H5Pset_vlen_mem_manager(transfer.id, allocate, None, release, None) < 0:
            raise ValueError('invalid_h5ad')
        status = library.H5Dread(node.id.id, memory_type.id, memory_space.id, file_space.id, transfer.id, output)
        if exceeded:
            raise ValueError('vlen_budget_exceeded')
        if out_of_memory:
            raise MemoryError()
        if status < 0:
            raise ValueError('invalid_h5ad')
        values = np.empty(element_count, dtype=object)
        for index, entry in enumerate(output):
            address = entry.address if numeric else entry
            if numeric:
                size = entry.length * base.itemsize
                if size and (address not in owned or size > ctypes.sizeof(owned[address])):
                    raise ValueError('invalid_h5ad')
                raw = ctypes.string_at(address, size) if size else b''
                values[index] = np.frombuffer(raw, dtype=base).astype(base.newbyteorder('='), copy=True)
                continue
            if address and address not in owned:
                raise ValueError('invalid_h5ad')
            raw = ctypes.string_at(address) if address else b''
            values[index] = raw.decode(info.encoding) if decode else raw
        return values.reshape(result_shape) if result_shape else values[0]


def copy_native_compound_tile(source, target, bounds):
    """Transfer legacy reference tokens and VLEN values using the on-disk datatype."""
    import ctypes
    from math import prod

    import h5py
    from h5py._objects import phil

    library, allocate_type, release_type = _vlen_hdf5()
    owned = {}
    allocated = 0
    exceeded = False
    allocation_failed = False

    @allocate_type
    def allocate(size, _):
        nonlocal allocated, exceeded, allocation_failed
        if allocated + size > VLEN_BYTE_LIMIT:
            exceeded = True
            return None
        try:
            buffer = ctypes.create_string_buffer(size)
            address = ctypes.addressof(buffer)
            owned[address] = buffer
        except (MemoryError, OverflowError):
            allocation_failed = True
            return None
        allocated += size
        return address

    @release_type
    def release(address, _):
        nonlocal allocated
        buffer = owned.pop(address, None)
        if buffer is not None:
            allocated -= ctypes.sizeof(buffer)

    shape = tuple(stop - start for start, stop in bounds)
    with phil:
        datatype = source.id.get_type()
        input_space = source.id.get_space()
        output_space = target.id.get_space()
        if shape:
            starts = tuple(start for start, _ in bounds)
            input_space.select_hyperslab(starts, shape)
            output_space.select_hyperslab(starts, shape)
        memory_space = h5py.h5s.create_simple((prod(shape),))
        transfer = h5py.h5p.create(h5py.h5p.DATASET_XFER)
        if library.H5Pset_vlen_mem_manager(transfer.id, allocate, None, release, None) < 0:
            raise ValueError('invalid_h5ad')
        buffer = ctypes.create_string_buffer(prod(shape) * datatype.get_size())
        ctypes.set_errno(0)
        status = library.H5Dread(source.id.id, datatype.id, memory_space.id, input_space.id,
                                 transfer.id, buffer)
        if allocation_failed:
            raise MemoryError()
        if exceeded:
            raise ValueError('vlen_budget_exceeded')
        check_native_hdf5(status, 'invalid_h5ad')
        # The allocation registry owns the VLEN pointers until the synchronous write returns.
        ctypes.set_errno(0)
        check_native_hdf5(
            library.H5Dwrite(target.id.id, datatype.id, memory_space.id, output_space.id, 0, buffer),
            'invalid_export',
        )


def with_small_attribute_caches(files, transfer):
    """Limit HDF5 heap caching while transferring a whole VLEN attribute."""
    from h5py._objects import phil

    originals = []
    with phil:
        try:
            for file in files:
                if any(file.id == previous.id for previous, _ in originals):
                    continue
                original = file.id.get_mdc_config()
                if not original.evictions_enabled or original.max_size <= 1024 * 1024:
                    continue
                small = file.id.get_mdc_config()
                small.min_size = small.max_size = small.initial_size = 1024 * 1024
                small.set_initial_size = True
                file.id.set_mdc_config(small)
                originals.append((file, original))
            transfer()
        finally:
            for file, original in reversed(originals):
                file.id.set_mdc_config(original)


def check_native_hdf5(status, code):
    import ctypes
    import errno

    if status < 0:
        if ctypes.get_errno() == errno.ENOMEM:
            raise MemoryError(code)
        raise ValueError(code)


def copy_native_vlen_attribute(source, target, name):
    """Transfer a VLEN attribute without decoding it to Python objects."""
    import ctypes
    from contextlib import closing
    from math import prod

    import h5py
    from h5py._objects import phil

    library, _, _ = _vlen_hdf5()
    with phil:
        datatype = source.get_type()
        space = source.get_space()
        count = prod(source.shape)
        memory_type = datatype
        if datatype.get_class() == h5py.h5t.COMPOUND:
            packed_type = datatype.copy()
            packed_type.pack()
            if packed_type.get_size() < datatype.get_size():
                memory_type = packed_type
        buffer = scratch_vector(count * memory_type.get_size(), 'uint8', memory_limit=1024 * 1024)
        pointer = buffer.ctypes.data
        ctypes.set_errno(0)
        check_native_hdf5(library.H5Aread(source.id, memory_type.id, pointer), 'invalid_h5ad')
        try:
            with closing(h5py.h5a.create(target.id, name.encode('utf-8'), datatype, space)) as output:
                ctypes.set_errno(0)
                check_native_hdf5(library.H5Awrite(output.id, memory_type.id, pointer), 'invalid_export')
        finally:
            if library.reclaim_vlen(memory_type.id, space.id, 0, pointer) < 0:
                raise ValueError('invalid_export')


def split_vlen_batch(start, stop, reader):
    """Retry large batches in halves while preserving source order."""
    try:
        values = reader(start, stop)
    except ValueError as error:
        if str(error) != 'vlen_budget_exceeded' or stop - start <= 1:
            raise
        middle = (start + stop) // 2
    else:
        yield start, values
        return
    yield from split_vlen_batch(start, middle, reader)
    yield from split_vlen_batch(middle, stop, reader)


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
    return min(rows, VLEN_BLOCK_ROWS) if is_budgeted_vlen(dtype) else rows


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

    def blocks(self):
        step = string_block_rows(self.dtype, 8192)
        for start in range(0, len(self), step):
            yield from split_vlen_batch(start, min(start + step, len(self)),
                                         lambda a, b: self[slice(a, b)])

    def validate(self):
        import numpy as np

        blocks = (values for _, values in self.blocks())
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
    import h5py
    import pandas as pd
    from anndata.io import read_elem
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
    node = frame.group[key]
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
        elif 'index' not in vars(frame) and h5py.check_string_dtype(frame.index_node.dtype):
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
            below_range = above_range = False
            for codes in blocks:
                below_range |= bool(np.any(codes < -1))
                above_range |= bool(np.any(codes >= len(categories)))
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
    """Inspect dtype/nullability without expanding an entire modern column."""
    import pandas as pd
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


def annotation_group(frame, key, block_rows=8192):
    """Exact disk-backed counts with a cardinality-independent frequency cache.

    SQLite has a 1 MiB page cache. Row strings/category dictionaries and indices
    still follow iter_annotation_column's documented bounds, not a byte cap.
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
        for start, block in iter_annotation_column(frame, key, block_rows, include_index=False):
            strings = block.astype('string')
            missing += int(strings.isna().sum())
            # sort=False preserves first occurrence within the processing block.
            counts = strings.dropna().value_counts(sort=False)
            rows = []
            for ordinal, (label, count) in enumerate(counts.items()):
                label = str(label)
                labels_fit &= len(label.encode('utf-8')) <= 200
                rows.append((label, int(count), start + ordinal))
            database.executemany('INSERT INTO counts VALUES (?,?,?) ON CONFLICT(label) DO UPDATE SET count=count+excluded.count', rows)
        distinct, total = database.execute('SELECT count(*), coalesce(sum(count),0) FROM counts').fetchone()
        top = database.execute('SELECT label,count FROM counts ORDER BY count DESC,first_row ASC LIMIT 20').fetchall()
    return {'distinct': distinct, 'missing': missing,
            'complete': distinct <= 20 and labels_fit,
            'other': total - sum(count for _, count in top),
            'counts': [{'label': label[:200], 'count': count} for label, count in top]}


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
    else:
        for start in range(0, len(frame), block_rows):
            yield start, pd.Series(frame.index[start:start + block_rows], copy=False)


def _numeric_category_keys(values):
    """Encode numeric keys losslessly, with pandas-equivalent signed zero."""
    import numpy as np

    if values.dtype.kind == 'f':
        if np.isnan(values).any():
            raise ValueError('invalid_h5ad')
        values[values == 0] = 0
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


def duplicate_frame_ids(frame):
    import h5py

    if isinstance(frame, LegacyFrame):
        if h5py.check_string_dtype(frame.dtype(frame.index_key)):
            return len(frame) - _distinct_labels(block for _, block in iter_frame_index(frame))
        return int(frame.index.duplicated().sum())

    node = frame.index_node if isinstance(frame, H5Frame) else None
    if isinstance(node, h5py.Dataset) and node.ndim == 1 and h5py.check_string_dtype(node.dtype):
        return len(frame) - _distinct_labels(block for _, block in iter_frame_index(frame))
    return int(frame.index.duplicated().sum())


def index_matches(frame, expected, rows=None, block_rows=8192):
    import numpy as np

    if len(frame) != (len(expected) if rows is None else len(rows)):
        return False
    for start, block in iter_frame_index(frame, block_rows):
        selection = slice(start, start + len(block))
        if rows is not None:
            selection = rows[selection]
        if not np.array_equal(np.asarray(block, dtype=str), expected[selection]):
            return False
    return True


def scratch_vector(length, dtype, memory_limit=8 * 1024 * 1024):
    """Return zero-filled scratch storage with a private file for large vectors."""
    import tempfile
    import numpy as np

    dtype = np.dtype(dtype)
    size = length * dtype.itemsize
    if size <= memory_limit:
        return np.zeros(length, dtype=dtype)
    with tempfile.TemporaryFile() as backing:
        backing.truncate(size)
        return np.memmap(backing, dtype=dtype, mode='r+', shape=(length,))
