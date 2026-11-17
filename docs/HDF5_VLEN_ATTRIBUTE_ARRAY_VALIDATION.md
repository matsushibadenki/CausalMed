# Variable-length attributes and compound array members

The metadata copier projects string and numeric variable-length leaves through nested HDF5 array and compound types. Shape, field order and fixed-width values are preserved. All variable-length leaves in a tile share the existing 8 MiB native payload budget. An oversized tile splits by dataset records. A single record with an oversized array fails with the existing budget error. Native descriptors also have a byte limit.

Reference-free variable-length metadata attributes transfer between HDF5 objects through a native buffer and keep their datatype and dataspace. HDF5 reclaims native payload allocations after the write. Attributes with large descriptors use a temporary file-backed buffer. HDF5 still reads the complete attribute payload in a single call. The native attribute API uses its own allocations and does not expose the dataset payload allocator. Reference-bearing attributes retain their existing paths.

A fresh-process synthetic probe copied 64 variable-length Unicode strings totaling about 32 MiB in one attribute. Additional peak RSS dropped from 88.5 MiB to 47.0 MiB, with matching SHA-256 `6a17af797b7a6f553dd30887222db81c27461323cdf0badc4a0929f64978e6a6` over the decoded values. HDF5 internal memory and the entire native attribute payload still affect RSS.

The regression compares nested arrays of compound values, ragged numeric sequences, wide-character strings and fixed-width leaves with native HDF5 copy. It forces tile subdivision and single-record rejection, and checks that attribute export avoids Python attribute decoding. The prepared bundled Python runtime passed the regression.

A temporary Kang-derived H5AD included 700 compound records with array members and a long UTF-8 attribute. After QC, the exported 12,729 rows matched the selected IDs and expression values. Independent HDF5 readback matched every compound array field, its dtype and the attribute. The temporary input checksum was unchanged. The supervised Rust history test recovered the public Kang selection after restart and exported QC ZIP and retained H5AD.

Frontend build and 21 tests, Rust core 23 tests, and app compile check passed. Python ran 74 tests with two PBMC-dependent cases skipped.

The portable project integration restored the 12,859-cell public Kang cohort, reconnected the original by SHA-256 and re-exported the saved definition.
