# Cell index and QC artifact streaming

Modern H5AD cell indices are opened lazily. Duplicate detection uses the existing disk-backed exact string set. QC outcome inspection and H5AD export compare original row identities in blocks, including selected-row readback. Duplicate cell names remain legal and retain their source positions.

QC saves the existing nine one-dimensional NPY members inside the compressed NPZ. Cell and donor IDs use a width scan and block conversion. Source row numbers, reason bits, mitochondrial percentages and donor missing flags are generated as their members are written. The writer keeps the original dtypes and values, including NaN for undefined mitochondrial percentages. Unicode write buffers are split at approximately 1 MiB, subject to a single value's size. A regression compares every decoded member against independently formed arrays across the 8,192-row boundary, including Unicode, repeated cell IDs and missing donor labels. The original source is checked again after deferred reads finish.

A synthetic probe wrote 524,288 fixed-width IDs of 64 characters in separate fresh processes. Additional peak RSS during preparation and write dropped from 190.5 MiB to 6.2 MiB. Both readbacks yielded SHA-256 `ce483c37a906f91b2718171bac6d21de7f7828c04357780c9fed476fe0e4ebc4` over the decoded ID array. This measures the ID write path. Expression aggregation, retained row indices and NPZ loading still use per-cell arrays. Legacy HDF5 annotation tables still use their compatibility decoder.

Public Kang QC retained 24,562 cells. The Python round trip independently checked retained rows, donor labels, expression values and the original SHA-256. Rust worker restart recovered the saved selection and exported both QC ZIP and retained H5AD. The separate portable-project integration saved and restored the 12,859-cell mixed cohort, reconnected the original by SHA-256 and exported again.

Frontend build and 21 tests, Rust core's 23 tests, the app compile check and both public Rust integrations passed. The Python suite ran 60 tests with two PBMC-dependent cases skipped.
