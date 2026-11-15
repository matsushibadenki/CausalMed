# Streamed donor QC validation — 2026-11-11

This increment implements the roadmap's donor-QC streaming step on top of the existing selective HDF5 projection. It preserves pending workspace changes and the current immutable QC artifact format. It verifies software behavior, not biological donor independence or predictive performance.

## Implementation

- Read modern donor annotations in 8,192-row blocks, using the existing typed numeric/string/categorical/nullable iterator and legacy fallback.
- Aggregate exact donor totals and retained counts in a private temporary SQLite database with a 1 MiB page cache and file-backed sorting. Return at most 30 groups, exact distinct/missing/other-cell totals, retained missing counts and the existing inventory-completeness flag.
- Only absent and whitespace-only values are missing. Nonblank raw labels such as `D1`, ` D1` and `D1 ` remain separate. No donor identity or review is inferred. Source strings are SQL parameters.
- Invalid expression still reports donor totals but returns null retention fields and never creates a cell-selection snapshot.
- For saved QC decisions, first scan label widths and then fill the final raw donor-ID and missing arrays in blocks. Missing values become empty strings; whitespace-only raw strings remain unchanged in the artifact with their missing flag set.

Equal-total donor groups use deterministic first-source-occurrence order, matching the new annotation-summary convention. This can change the order of tied display groups relative to the old unstable pandas sort, including ties at the top-30 boundary. Counts, completeness semantics and combined other-cell totals remain exact. Report fields, QC thresholds, source SHA identity and the NPZ schema are unchanged; saved results are not rewritten.

## Allocation probe

Command: `PYTHONPATH=python .venv/bin/python -B python/benchmarks/donor_qc_memory.py`.

Two fresh Python processes prepared donor summaries and raw donor/missing arrays for a synthetic 1,048,576-row integer annotation with 1,048,576 distinct IDs and alternating retained rows. Source indices and the retention mask were loaded before measuring. This isolates donor preparation, excluding expression QC, artifact compression and the final verification hash allocation. The full-column reference uses stable tie ordering to compare the same summary semantics.

| Path | Additional process high-water growth |
| --- | ---: |
| Full-column reference | 356,139,008 bytes (339.6 MiB) |
| Chunked summary and raw-array preparation | 54,312,960 bytes (51.8 MiB) |

Both summary/raw-ID/missing-array hashes were `60c7ed8bb471be3ca413ced61ed87f4aa68dce230fbfd307bd8f3844774a3773`. The benchmark checks identical payloads, less than 64 MiB chunked growth and more than 64 MiB improvement. These are synthetic allocation observations, not a whole-worker memory guarantee.

The final Unicode donor array remains O(cell count × longest label); per-cell QC arrays, indices, category dictionaries, legacy tables and large individual strings can still allocate memory. Temporary SQLite trades memory for local disk I/O. Strict OS containment and further artifact-storage work remain unfinished.

## Public H5AD verification

Input: [Kang 2018 distributed through Pertpy](https://exampledata.scverse.org/pertpy/kang_2018.h5ad), 24,673 cells × 15,706 genes. SHA-256: `e6a5adac64dcdeb36eaba27db49b63e0c64bb0ed4a64c6705971506b41c39830`.

Independent full-matrix count/gene thresholds produced the same 24,562 retained and 111 excluded cells. Every `replicate` donor total and retained count matched an independent pandas calculation; all raw donor IDs in the saved `cells.npz` matched the source in row order. Source SHA remained unchanged. The existing retained-H5AD regression independently verified original row IDs, expression values and donor labels after export. Missing mitochondrial features still produce partial QC.

The Rust supervised-worker integration passed with the embedded updated Python code: exact CD4 control/stimulated exports (5,560 / 5,678 cells), multi-value CD4/CD8 control/stimulated export (12,859 cells), frozen donor lineage, project separation, conservative eligibility, portable save/restore, SHA reconnection and historical re-export. No new native visual check is claimed for this backend-only change.

## Checks

- New tests cover multiple block sizes, ordered/unused categories, nullable numeric/boolean/string values, Unicode, long labels, non-normalized whitespace, high cardinality, all-missing and empty inputs, invalid mask dimensions and late corrupt category codes.
- Dense/CSR/CSC QC and snapshot preparation reject full modern donor-column decoding in the test while preserving raw IDs and missing flags. Invalid expression creates no snapshot.
- Python suite with the public Kang path: 55 tests, 53 passed and two unrelated PBMC-input tests skipped.
- Prepared bundled Python also ran the new helper tests; public-file checks ran through the supervised Rust worker with the development test environment.
- Frontend build and 21 tests passed; Rust core's 23 tests and `cargo check -p causalmed --offline` passed. The opt-in public mixed-cohort Rust integration passed.
