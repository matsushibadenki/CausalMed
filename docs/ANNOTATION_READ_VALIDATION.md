# Annotation reading without retaining complete tables

2026-11-10, Apple-silicon macOS, project Python 3.12 environment.

## Behavior

Modern H5AD obs/var/raw-var dataframes now load only their indices and column
names at opening. Individual columns decode when requested and are not cached by
the read-only projection. QC schema inspection holds one decoded column at a time
and releases its last column before expression analysis. Category discovery reads
HDF5 encoding attributes instead of loading every column a second time. Group
summary temporaries are also released before expression QC.

QC, cohort filtering and scalar outcomes share this projection, including Rust's
concatenated embedded worker. Nullable numeric values, categorical codes and
missing categories, named and duplicate row indices, and explicit column order
are preserved. Accessing a malformed column with a mismatched row count fails.
Legacy structured tables retain the existing eager compatibility path, including
categorical repair in mixed legacy/modern files.

Unstructured dictionary nodes also remain lazy. Inspecting cohort origins reads
only definition JSON, its digest and retained count; unrelated payloads attached
to the same origin are not decoded. Existing checksum validation and bounded
origin reports remain unchanged.

## Limits and next work

This is not a hard allocation cap. Each requested column, category inventory,
index and scalar metadata value can still be large. The legacy path still materializes complete annotation tables. The subsequent
[annotation export increment](ANNOTATION_EXPORT_VALIDATION.md) removes full-table
reconstruction for modern exports. Chunked
column processing, bounded variable-length values and indices, and strict OS
containment remain unfinished. Sampled worker RSS supervision remains in force.
No new prediction or biological validation claim follows from these changes.

## Allocation probe

Run:

```sh
PYTHONPATH=python .venv/bin/python python/benchmarks/annotation_read_memory.py
```

The synthetic fixture has 262,144 rows and 64 float64 annotation columns (128 MiB
of column payload). Fill-value datasets avoid materializing payload at setup.
Independent fresh processes open it and verify one selected column's sum.

| Reader | Process high-water increase |
| --- | ---: |
| AnnData backed | 315,670,528 bytes (301.0 MiB) |
| Column-on-demand projection | 46,907,392 bytes (44.7 MiB) |

The probe requires projection growth below 64 MiB and improvement above 64 MiB.
These fixture thresholds are not universal guarantees. The remaining growth
includes the full string row index and library/HDF5 overhead.

## Regression coverage

New tests verify that unused annotation payloads and unrelated nested provenance
arrays are never read, schema-only category discovery does not decode columns,
nullable values/categories/duplicate named indices round-trip unchanged, mutation
of a returned column cannot alter later reads, and malformed lengths are rejected.
Existing tests exercise dense/CSR/CSC layers, raw, old structured tables and
isolated embedded execution.

Validation commands completed:

- `npm run build` and `npm test`: passed, 21 frontend tests.
- `cargo test -p causalmed-core`: passed, 23 tests.
- `cargo check -p causalmed`: passed.
- Project Python with `PYTHONPATH=python` and `CAUSALMED_TEST_DONOR_H5AD` set
  to the retained Kang fixture: 40 tests, two optional PBMC-file tests skipped.
- Rust embedded-worker public-Kang checks: donor QC/restart and scalar
  observation restore/relink/abstention/project isolation, and mixed-cohort
  export/adoption/restore/recheck/isolation passed.
