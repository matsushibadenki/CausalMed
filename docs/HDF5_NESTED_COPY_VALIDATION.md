# Nested compound metadata and large HDF5 attributes

H5AD export projects the leaves of nested compound records and writes bounded tiles with the original dtype, field ordering and shape. Supported leaves include fixed-width fields, variable-length text and variable-length numeric sequences. The 8 MiB native payload budget is shared across all variable-length leaves in a tile. Tiles split on overflow, and an oversized single record produces the existing budget error. See the subsequent [array member validation](HDF5_VLEN_ATTRIBUTE_ARRAY_VALIDATION.md) for variable-length leaves inside arrays.

Fixed-width attributes of at least 1 MiB use a private file-backed transfer buffer and retain their original HDF5 datatype and dataspace. HDF5 attribute reads and writes operate on the whole value. The mapping removes the full Python heap allocation for this transfer. Mapped pages and HDF5 internal buffers still contribute to RSS. The subsequent [attribute validation](HDF5_VLEN_ATTRIBUTE_ARRAY_VALIDATION.md) covers native transfer for variable-length attributes.

Regression coverage compares nested UTF-8, ragged integers and a big-endian fixed-width array field against a native HDF5 copy. A test-only byte budget forces subdivision and single-record rejection. The large-attribute check forbids the normal Python attribute read during export and verifies the resulting dtype and values. The prepared bundled Python runtime also passed this test.

A temporary copy of the public Kang-derived H5AD contained 700 nested compound records and a 1.2 MB fixed-width attribute. QC retained 12,729 cells. Independent HDF5 readback matched retained row IDs, expression values, the compound fields, and the attribute's values and datatype. The input checksum was unchanged. The supervised Rust history test recovered the public Kang selection after worker restart and wrote QC ZIP and retained H5AD.

Frontend build and 21 tests, Rust core's 23 tests and app compile check passed. Python ran 73 tests with two PBMC-dependent cases skipped.

The portable project integration restored the 12,859-cell public Kang definition, reconnected the original by SHA-256 and re-exported the saved cohort.
