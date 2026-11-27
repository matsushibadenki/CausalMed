"""Private bounded scratch arrays shared by H5AD readers and writers."""


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
