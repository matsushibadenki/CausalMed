# Oversized CSC decoding

Backed CSC QC keeps the compact decoder for ordinary blocks and the direct streaming path for oversized, strictly ordered single columns. Other oversized groups read raw entries in chunks of at most 262,144. A private temporary SQLite database orders entries by source row, column and original storage position, with a 1 MiB page cache. Entry values preserve their original binary dtype. Duplicate coordinates add sequentially in storage order using that dtype, including its existing integer overflow and floating-point behavior. This also handles a run of duplicates larger than the read chunk. Fully consolidated rows feed compact CSR tiles that flush after 8,192 rows or the entry budget at a row boundary.

This preserves the previous logical column grouping, row arithmetic, raw input validation and QC NPZ schema. Invalid stored values are counted before duplicate consolidation. The disk-backed path trades temporary disk space and sorting time for bounded decode and reduction buffers. Disk use scales with stored entries in the largest logical column block. Existing QC accumulator vectors and legacy annotation decoding still have their own memory costs.

A fresh-process synthetic probe used four million rows, two columns and 600,000 stored entries with unsorted row indices and duplicates. Additional peak RSS for decoding and reducing the block fell from 30.4 MiB to 19.9 MiB. The occupied source rows, per-row sums and positive-feature counts yielded the same SHA-256 `0c6ce19ee5a77ee20b69ba8205f3699e63531867a4504ddc1900287e19e0bffe`.

Regression cases force the new path with reversed indices, duplicates crossing chunk boundaries, multiple columns, unsigned integer overflow, floating-point addition order and invalid raw entries. They independently compare QC reports and all saved arrays against the existing in-memory SciPy path and make full backed CSC slicing fail during the test.

On public Kang 2018, a test-only entry limit of 60,000 sent 32 original CSC blocks through the disk-backed path. The QC report and all nine decoded NPZ members matched the normal threshold exactly. The forced-path QC history exported 24,562 cells with identical retained row IDs and expression values. The original source retained SHA-256 `e6a5adac64dcdeb36eaba27db49b63e0c64bb0ed4a64c6705971506b41c39830`.

The frontend build and 21 tests, Rust core's 23 tests, and app compile check passed. Python ran 66 tests with two PBMC-dependent cases skipped. The supervised Rust history test recovered a selection after worker restart and wrote QC ZIP and retained H5AD.

The portable project integration also passed with the production threshold. It saved and restored the 12,859-cell mixed cohort, reconnected the moved source by SHA-256 and exported the restored definition.
