# Chunked cohort filters and generated QC columns

2026-11-10, Apple-silicon macOS, project Python 3.12 environment.

## Behavior

Exact cohort condition/perturbation matching now processes 8,192-row blocks.
Modern HDF5 annotation readers handle numeric/string datasets, categorical codes,
nullable integers/booleans/strings and missing values without reading the complete
cell-value column. Category dictionaries decode once per column, and annotation
component lengths are checked before the first block is yielded. Legacy tables
remain eager but their string conversion/matching operates on slices.

Existing semantics remain unchanged: exact string comparison, OR within a role,
AND across roles, exclusion of missing values and intersection with parent QC.
Matching source offsets are preserved across blocks, including duplicate named
row indices. The selected-cell mask remains resident.

Generated QC/canonical-donor columns now select and write rows in blocks rather
than constructing a complete retained-column copy. Both modern and legacy input
layouts use this path. Numeric/bool/Unicode values retain the AnnData array/string
encodings, deterministic column order and provenance. Gzip compression is applied.
A single fixed-width Unicode item larger than the budget fails before its dataset
is created, using the existing translated unsupported-encoding error.

Receipts add `maxGeneratedBlockBytes` and `generatedBlocks`. The former measures
selected NumPy payload bytes; it does not include Unicode-to-object conversion,
Python string/HDF5 overhead or pre-existing source arrays. It is not a total RSS
or OS allocation cap. No cell-level payload crosses IPC.

## Evidence

Three new automated tests cover typed two-row block reconstruction (Unicode,
ordered categoricals, nullable unsigned integers, booleans and strings, numeric
NaN, duplicate named indices), malformed nullable masks rejected before yielding,
and selected-row generated writes under a 1 KiB budget with a partial final block.
Existing cohort tests forbid full modern-column reads and force two-row boundaries
through dense/CSR/CSC exports, checking unchanged retained/matched counts and donor
lineage. Existing multi-value tests retain exact OR/AND semantics.

- `npm run build` and `npm test`: passed, 21 frontend tests.
- `cargo test -p causalmed-core`: passed, 23 tests.
- `cargo check -p causalmed`: passed.
- Rust embedded-worker public-Kang integrations passed: historical QC ZIP/H5AD
  export across restart, and mixed-cohort export/adoption/restore/recheck/isolation.
- Project Python suite with public Kang enabled: 45 tests, 43 passed, two optional
  PBMC-file tests skipped because those source files are not configured.

## Exact-filter allocation probe

```sh
PYTHONPATH=python .venv/bin/python python/benchmarks/cohort_filter_memory.py
```

Separate fresh processes compare complete-column conversion with 8,192-row blocks
for a 1,048,576-row synthetic numeric condition column. A numeric synthetic index
avoids hiding filter allocations behind string-index decoding high-water peaks;
index loading occurs before measurement. Half the cells match the same exact
15-digit value in both modes. Both selected masks have SHA-256
`129034ce57658033036caaa471870918821d135ebae450ca87518b864223e143`.

| Filter | Process high-water increase |
| --- | ---: |
| Full-column conversion | 127,467,520 bytes (121.6 MiB) |
| Chunked conversion | 2,686,976 bytes (2.6 MiB) |

The probe asserts identical selection, less than 16 MiB chunked growth and more
than 32 MiB improvement. These are isolated synthetic allocation regression
thresholds, not general full-pipeline memory or biological/predictive guarantees.

The existing `annotation_export_memory.py` probe additionally checks generated
payload block bytes against its 64 KiB budget and that multiple writes occur.
The updated run exported 128 MiB of retained numeric annotations with 37,928,960
bytes (36.2 MiB) of export high-water growth. The largest generated selected
payload was 65,520 bytes over 133 generated writes; original numeric reads stayed
within 65,536 bytes. This probe excludes fixture/vector preparation from growth.

## Remaining limits

Schema/group summaries and donor QC still use complete individual columns.
Indices, category dictionaries, legacy tables, source/decision vectors and computed
canonical-donor arrays remain resident. Variable-length values can exceed a row
block's expected footprint. Strict OS containment remains unfinished.
