# Annotation summary validation — 2026-11-10

This increment addresses the roadmap's remaining full-column schema/group-summary reads. It builds on the existing selective HDF5 projection and chunk iterator without replacing their pending working-tree changes. Original H5AD files and saved QC artifacts remain read-only.

## Implementation and compatibility

Modern annotation dtype/nullability scans read at most 8,192 rows per processing block. They continue through all blocks, even after finding a missing value, so malformed later categorical codes are not silently skipped. Group summaries convert only a block to strings and aggregate exact frequencies in a private temporary SQLite database with a 1 MiB page cache and file-backed sorting. Source annotation text is bound as SQL parameters. The database closes on success or exception; no named project artifact is created.

The report schema, annotation types, exact distinct/missing counts, top-20 frequencies, other totals, UTF-8 completeness bounds and QC annotation context are unchanged. Equal-frequency categories now use deterministic first-source-occurrence order. This deliberately differs from pandas' unstable default frequency sort; tied entries at the top-20 boundary may change order/membership, but their counts and combined other total remain exact. No new user-facing controls or error codes were introduced.

This is not a strict memory cap. Source indices, legacy annotation tables, category dictionaries, donor QC and variable-length values can still allocate memory. Counting very large inventories trades memory for temporary local disk/SQLite I/O. Numeric row blocks do not impose byte limits on individual strings. Cancellation/timeouts continue to be owned by the Rust process supervisor.

## Allocation probe

`PYTHONPATH=python .venv/bin/python -B python/benchmarks/annotation_summary_memory.py`

Two fresh Python subprocesses summarized a synthetic 1,048,576-row integer annotation with 1,048,576 distinct labels. The frame/index were opened before measuring process high-water growth. The full-column reference uses a stable source-order frequency sort to compare the same deterministic summary contract.

| Path | Additional process high-water growth |
| --- | ---: |
| Full-column reference | 223,854,592 bytes (213.5 MiB) |
| Chunked schema and disk-backed group summary | 7,503,872 bytes (7.2 MiB) |

Both bounded result objects had SHA-256 `46eca955b2a3a1b5e3931e77b395a4fca2255cbd8f6e48296712e67afe720fae`. These are synthetic software allocation observations, not biomedical findings or a whole-worker memory guarantee. The probe asserts exact summary equality, less than 32 MiB chunked growth and more than 32 MiB improvement.

## Public-file verification

Public [Kang 2018 H5AD distributed through Pertpy](https://exampledata.scverse.org/pertpy/kang_2018.h5ad), 24,673 cells × 15,706 genes; source SHA-256 `e6a5adac64dcdeb36eaba27db49b63e0c64bb0ed4a64c6705971506b41c39830`.

An independent full anndata read agreed with every emitted group summary and schema dtype/nullability. QC with donor column `replicate` retained 24,562 cells and excluded 111. The retained H5AD was read back independently: its original row IDs, all expression entries and donor labels matched the parent retained mask exactly. Source SHA remained unchanged. The missing mitochondrial-feature/partial-QC status was preserved, not upgraded to complete QC.

The Rust supervised-worker mixed-cohort integration also passed: single CD4 control/stimulated outputs (5,560/5,678 cells), multi-value CD4/CD8 control/stimulated output (12,859 cells), donor lineage, project-local adoption, conservative mixed-annotation abstention, portable restore, moved-source reconnection and historical re-export.

## Automated checks

- New summary tests cover multiple block sizes, nullable integer/boolean/string, float, ordered categoricals with unused labels, Unicode/overlong labels, tied frequencies, high cardinality, all-missing columns and corrupt late codes. Modern helpers reject attempted full-column reads in the test.
- Python suite with the public Kang path: 49 tests; two unrelated PBMC inputs remain opt-in.
- Frontend production build and 21 tests passed; Rust core's 23 tests and `cargo check -p causalmed --offline` passed.
- The prepared bundled Python runtime also imported its SQLite module and executed the summary helper without the development environment.
- The supervised Rust public mixed-cohort integration passed. No new native visual QA was needed for this backend-only increment; previous UI validation is not presented as a new screen check.
