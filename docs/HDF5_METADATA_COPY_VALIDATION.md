# HDF5 metadata copy and variable-length numeric sequences

Export walks ordinary HDF5 metadata groups and copies fixed-width arrays in tiles. This includes unfiltered feature annotations, raw feature metadata, category dictionaries and opaque AnnData metadata such as `uns`. Dataset shape, dtype, values and encoding attributes carry through. Within each copied group, repeated hard links share the copied object and soft links retain their target. Dataset compression and chunk layout can change.

Variable-length numeric sequences use the same 8 MiB native HDF5 payload budget as variable-length text. The exporter subdivides oversized batches, preserves empty sequences, and rejects a single sequence that exceeds the budget. It writes numeric sequences by element to preserve their shape when several sequences have equal lengths. The budget covers the allocator-managed payload. HDF5 internal buffers, Python objects and attributes still contribute to process memory. Special datasets such as compound or reference types retain the HDF5 compatibility copy.

The regression copies a wide fixed-width array and a two-dimensional variable-length integer array, checks their values and links, and forces batch subdivision and single-element rejection. The wide fixed-width read stays within the configured 1 KiB test budget.

A temporary copy of the public Kang-derived cohort included a 16 MB fixed-width metadata array and a two-dimensional dataset of variable-length floating-point sequences. QC and export retained 12,729 cells. Independent AnnData readback matched the selected cell IDs, expression values and both metadata datasets. The temporary source checksum remained stable. Normal public Kang QC also recovered its selection after worker restart and exported QC ZIP and retained H5AD.

Frontend build and 21 tests, Rust core 23 tests and app compile check passed. Python ran 71 tests with two PBMC-dependent skips. The prepared bundled Python runtime passed the metadata copy regression.

The portable project integration restored the 12,859-cell mixed cohort, reconnected the moved original by SHA-256 and re-exported the saved definition.
