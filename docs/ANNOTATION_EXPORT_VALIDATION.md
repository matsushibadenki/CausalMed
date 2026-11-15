# Streamed annotation export without a full-table round trip

2026-11-10, Apple-silicon macOS, project Python 3.12 environment.

## Behavior

The modern H5AD exporter already copied obs columns in selected-row blocks, but
then decoded the complete retained obs table, added QC columns and rewrote it.
Its final identity check decoded that table again. Both full-table reads are now
removed. QC/donor lineage fields are written directly into the copied obs group,
and its `column-order` attribute is extended in the existing deterministic order.
Verification reads only the row index, matrix shape and expected provenance
manifest. The original annotation encoding and column metadata remain intact.

This applies to ordinary retained exports and cohort exports, including canonical
donor IDs/review flags. English, Japanese and Simplified Chinese annotation values
are preserved without introducing new user-facing text or errors. Source checksum,
selection identity, collision handling and Rust's staged publication are unchanged.
Rust now preserves the existing `unsupported_export_encoding` error code, so
unsupported/corrupt streamed encodings use the existing EN/JA/简体中文 explanation
instead of a generic worker-failure message. A supervisor regression verifies
that this failure removes staging, publishes nothing and retains the prior report.
Legacy structured annotation tables retain their existing compatibility path.

## Tests

The new regression test rejects any attempt to decode an HDF5 dataframe during a
modern export. It verifies named/duplicate cell IDs, Unicode labels, ordered
categories with unused levels, nullable integers/booleans, original column encoding
attributes, exact column order and every appended QC/canonical-donor value.
A second regression rejects out-of-range/negative category codes and duplicate
category labels. The copier validates category dictionaries one at a time and
checks selected codes in bounded blocks instead of expanding cell labels.
Existing tests check dense/CSR/CSC, layers, raw, embeddings, pairwise matrices,
unsupported formats and unchanged source bytes.

- `npm run build` and `npm test`: passed, 21 frontend tests.
- `cargo test -p causalmed-core`: passed, 23 tests.
- `cargo check -p causalmed`: passed.
- Rust embedded-worker public-Kang tests passed for QC ZIP/retained-H5AD export
  across restart and mixed-cohort export/adoption/restore/recheck/isolation.
- Python suite in the project environment, with the public Kang source enabled:
  42 tests, 40 passed and two optional PBMC-file tests skipped. Public Kang has
  legacy structured obs, so this also exercises the retained compatibility path.

## Memory probe

Run in a fresh process:

```sh
PYTHONPATH=python .venv/bin/python python/benchmarks/annotation_export_memory.py
```

The synthetic fixture contains 65,536 rows and 256 float64 annotation columns.
All rows are retained, representing 134,217,728 bytes (128 MiB) of numeric
annotation payload. HDF5 fill values avoid allocating that payload at setup.
The measured export high-water increase was 47,595,520 bytes (45.4 MiB).
The largest numeric read block was 65,536 bytes over 4,120 read blocks.
The probe checks output dimensions/values/column order, a 64 MiB high-water-growth
regression threshold and the configured 64 KiB numeric read-block budget.

These are fixture-specific allocation checks, not biological evidence, predictive
performance claims or a total RSS cap. Original/selected cell-ID vectors, each
new QC column's selected values, category inventories, legacy tables, HDF5 caches and variable-length
strings remain outside the numeric read-block budget. Generated QC-column writes and exact cohort filters were subsequently chunked;
see [chunked cohort validation](CHUNKED_COHORT_VALIDATION.md). Strict containment,
schema/group/donor-QC streaming and remaining resident metadata are still Next.
