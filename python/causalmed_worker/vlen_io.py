"""Bounded native HDF5 variable-length reads and compatibility transfers."""
from contextvars import ContextVar
from functools import lru_cache
import sys

if 'scratch_vector' not in globals():
    from causalmed_worker.scratch import scratch_vector

# Tauri executes these sources together as one isolated script.
vlen_io = sys.modules[__name__]


VLEN_BLOCK_ROWS = 256


VLEN_BYTE_LIMIT = 8 * 1024 * 1024
ANNOTATION_SUMMARY_BYTES = 1024 * 1024
CATEGORY_LABEL_CHUNK_BYTES = 64 * 1024
_annotation_summary_read = ContextVar('annotation_summary_read', default=False)


def vlen_byte_limit():
    return VLEN_BYTE_LIMIT


def _windows_hdf5_library():
    """Use the already-loaded HDF5 DLL from the installed h5py wheel."""
    import ctypes
    from ctypes import wintypes
    from importlib.metadata import distribution

    package = distribution('h5py')
    get_module = ctypes.WinDLL('kernel32', use_last_error=True).GetModuleHandleW
    get_module.argtypes = [wintypes.LPCWSTR]
    get_module.restype = wintypes.HMODULE
    libraries = []
    for entry in package.files or ():
        name = entry.name.lower()
        if not name.endswith('.dll') or not name.startswith(('hdf5', 'libhdf5')):
            continue
        path = str(package.locate_file(entry).resolve())
        handle = get_module(path)
        if handle:
            library = ctypes.CDLL(path, handle=handle, use_errno=True)
            if hasattr(library, 'H5Pset_vlen_mem_manager') and hasattr(library, 'H5Dread'):
                libraries.append(library)
    if len(libraries) != 1:
        raise ValueError('invalid_h5ad')
    return libraries[0]


@lru_cache(maxsize=1)
def _vlen_hdf5():
    """Resolve symbols from the same HDF5 library that owns the h5py IDs."""
    import ctypes
    import h5py

    if sys.platform == 'win32':
        library = _windows_hdf5_library()
    else:
        library = ctypes.CDLL(h5py.h5d.__file__, use_errno=True)
    allocate = ctypes.CFUNCTYPE(ctypes.c_void_p, ctypes.c_size_t, ctypes.c_void_p)
    release = ctypes.CFUNCTYPE(None, ctypes.c_void_p, ctypes.c_void_p)
    hid = ctypes.c_int64
    library.H5Pset_vlen_mem_manager.argtypes = [hid, allocate, ctypes.c_void_p, release, ctypes.c_void_p]
    library.H5Pset_vlen_mem_manager.restype = ctypes.c_int
    library.H5Pset_buffer.argtypes = [hid, ctypes.c_size_t, ctypes.c_void_p, ctypes.c_void_p]
    library.H5Pset_buffer.restype = ctypes.c_int
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


def read_bounded_vlen(node, selection, *, field=None, decode=True, byte_limit=None, borrow_text=False):
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
        if byte_limit is None:
            byte_limit = VLEN_BYTE_LIMIT
            # Preserve the original limit for an individual large label.
            if element_count > 1 and info is not None and _annotation_summary_read.get():
                byte_limit = min(byte_limit, ANNOTATION_SUMMARY_BYTES)
        if element_count * ctypes.sizeof(element) > VLEN_BYTE_LIMIT:
            raise ValueError('vlen_budget_exceeded')
        output = (element * element_count)()
        if borrow_text:
            # One scalar only needs space for its on-disk and in-memory
            # descriptors. The HDF5 VLEN converter manages the payload itself.
            conversion_bytes = 4096 if element_count == 1 else CATEGORY_LABEL_CHUNK_BYTES
            if library.H5Pset_buffer(transfer.id, conversion_bytes, None, None) < 0:
                raise ValueError('invalid_h5ad')
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
            if borrow_text and address:
                # The view owns the ctypes buffer after the read returns.
                view = memoryview(owned[address]).cast('B')
                length = int(np.frombuffer(view, dtype=np.uint8).argmin())
                if view[length] != 0:
                    raise ValueError('invalid_h5ad')
                values[index] = view[:length]
            else:
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


def with_small_metadata_caches(files, transfer):
    """Temporarily reduce HDF5 metadata caching during VLEN operations."""
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


