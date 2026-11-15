# QC archive reads and expression aggregation

The worker validates the existing NPZ and manifest digests, inflates each required NPY member to a private temporary file in 1 MiB buffers and maps it read-only. The archive schema and receipt checksums retain their existing format. Supported NPY 1.0 and 2.0 headers cover the previous NumPy writer and the block writer. Header shape, dtype and payload lengths are checked before decompression. Row order, retained flags and retained count are checked in 8,192-row blocks. Regression coverage regenerates an archive using the previous NumPy writer, verifies all mapped fields and exports it to H5AD.

Expression QC packs six exclusion masks into one byte per cell, reuses the mitochondrial sum buffer for percentages, and accumulates histogram bins in blocks. Exact median calculation partitions a private disk-backed scratch array in place. Tests compare histogram counts, boundaries, extrema and median against the previous NumPy calculation. Existing fixtures also cover overlapping reasons, dense and sparse matrices, zero-library rows and donor alignment.

| Synthetic operation | Previous additional peak RSS | Updated additional peak RSS |
| --- | ---: | ---: |
| Decode nine NPZ arrays for 524,288 cells with 64-character IDs | 158.6 MiB | 12.4 MiB |
| Histogram and exact median of four million values | 68.1 MiB | 33.3 MiB |

Separate processes measured additional high-water RSS around the stated operation. The NPZ probe measures opening arrays before readers traverse them. Touched mapped pages can contribute to RSS. Temporary disk usage grows with the uncompressed arrays, and expression accumulators and export row vectors still scale with cell count.

Public Kang QC retained 24,562 cells and excluded 111. After worker restart, the stored QC exported to ZIP and H5AD. The portable-project integration exported the 12,859-cell mixed cohort again after save, restore and SHA-256 source reconnection. The Python public round trip checked retained row IDs, donor labels, expression values and source integrity. Frontend build and 21 tests, Rust core 23 tests, app compile check and both public Rust integrations passed. Python ran 62 tests with two PBMC-dependent cases skipped.
