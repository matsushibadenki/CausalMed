# Real-H5AD scalar outcome validation — 2026-11-06

This increment connects captured source/QC/donor cohorts to a read-only scalar count-outcome inspection and a guarded donor-level M0 baseline. Public data verifies implementation behavior. It does not validate biological donor identity, cross-study assay comparability, clinical predictions, IPF generalization or model superiority.

## Public input and reference calculation

Input: [Kang 2018 distributed through Pertpy](https://exampledata.scverse.org/pertpy/kang_2018.h5ad), 24,673 cells × 15,706 genes; SHA-256 `e6a5adac64dcdeb36eaba27db49b63e0c64bb0ed4a64c6705971506b41c39830`. Test roles were `replicate` for donor, `cell_type` for condition and `label` for perturbation. QC retained 24,562 cells and excluded 111. Missing mitochondrial features leave QC partial.

For exact variable-index ID `ISG15`, the worker read the original count matrix and calculated `log1p(ISG15 count / captured library count × 10,000)` for retained cells, then averaged within each original donor. An independent full-matrix Python reference used original matrix row sums and the saved mask, agreeing within absolute tolerance `2e-6`. The larger full-matrix read occurred only in the test reference process; the application worker reads one feature in row chunks.

| Original donor | Retained cells | Observed mean log1p CP10K |
| --- | ---: | ---: |
| patient_101 | 2,024 | 2.760639 |
| patient_1015 | 5,090 | 2.666557 |
| patient_1016 | 3,358 | 2.555077 |
| patient_1039 | 992 | 3.084243 |
| patient_107 | 1,042 | 2.612206 |
| patient_1244 | 3,343 | 2.102712 |
| patient_1256 | 4,134 | 2.522846 |
| patient_1488 | 4,579 | 2.506111 |

These mixed-condition means are software-verification observations, not treatment effects. No provenance, measurement or biological identity review was invented to obtain a public-data score. M0 correctly saved abstention with null metrics for partial QC, unresolved semantics/partition, insufficient independent split donors and unreviewed measurement comparability.

## Verified behavior

| Contract | Evidence |
| --- | --- |
| Exact feature selection | Python tests compare dense/CSR/CSC donor means and feature-space digests; absent IDs produce blocked rows |
| Input integrity | Worker hashes source and immutable QC files, matches original cell IDs, retained count and donor census; changed source/selection tests return no donor values |
| Feature alignment | Rust requires equal full feature-ID-set digests/counts and matrix roles; mismatches abstain |
| Donor weighting | Synthetic core fixtures produce control mean 1.5 and MAE 2.0 with two independent train controls and two test donors. More cells do not increase donor weight; repeated common IDs and overlap block scoring |
| Fresh gates | Real integration rechecks eligibility after moving QC to trash: an existing inspection cannot bypass `qc_unavailable`, and no score is emitted |
| Review freshness | Measurement review binds to the complete context; changed feature specifications reject old inspections and require review/reinspection |
| Persistence | Frozen source metadata, QC observations, donor mappings, feature spec, result and evidence survive core restoration. Successful score fields are recomputed during validation; a forged MAE is rejected |
| Portable project | Real integration saves and validates a package containing inspection and abstention snapshots, including historical QC references; absent bindings initially block reading |
| Reconnection | Relocating and SHA-reconnecting the original after restore permits the same eight-donor inspection |
| Project separation | Project B has no plan-derived reports or cohorts from A; it cannot construct A's inspection jobs |
| Lifecycle | Outcome-specific tests exercise timeout, cancellation and malformed output; child is reaped and previous outcome/selected QC are preserved |
| Limits | Eight reports / 1 MiB, 32 cohorts / 96 KiB context, 30 donors per cohort. Limit failures are atomic and never prune reports |
| Original preservation | The original public H5AD SHA remains unchanged after inspection, package restoration and reinspection |

## Native visual QA

The tested flow was: `tauri://localhost` Benchmark → save exact `ISG15`/`_index` plan → inspect expression → select original source → display eight donor bars → run guarded M0 → see saved abstention reasons → reselect the observation snapshot.

Environment: macOS native debug Tauri/WebView, Python 3.12 / anndata 0.12.19, Node 22.17.1. Browser skill was unavailable; Computer Use exercised the actual app and Rust IPC, rather than a browser mock of research state. At 1380 × 900 and 1024 × 1245, meaningful content rendered without a framework overlay, fields and buttons remained readable, graphs aligned with donor labels/counts and history selection updated the visible result. Native console logs were not instrumented. Screenshots were saved outside the repository at `/private/tmp/causalmed-real-outcome-desktop.jpg` and `/private/tmp/causalmed-real-outcome-narrow.jpg`.

The native project also contained a previously exported stimulated H5AD whose temporary original was no longer present. Inspection saved a blocked row for that child and actual donor summaries for the connected original. It did not manufacture a child result or mark the full plan eligible. A brief project-busy banner on immediate post-launch document refresh was identified during QA; inspection now refreshes worker status immediately and waits for the committed document event before reloading research state.

After the user unlocked the Mac, the final debug app was quit and relaunched successfully. The ISG15 plan, saved observation and saved abstention were restored at revision 51. English and Simplified Chinese fields, controls and abstention reasons rendered correctly at 1380 × 900; the app was returned to Japanese. A fresh inspection at revision 55 reproduced all eight donor means and cell counts, selected the connected original automatically, and did not show the transient project-busy banner. Additional screenshots are `/private/tmp/causalmed-real-outcome-en.jpg`, `/private/tmp/causalmed-real-outcome-zh.jpg` and `/private/tmp/causalmed-real-outcome-final-ja.jpg`. The 720-pixel minimum native width and Windows/Linux rendering remain unverified. Native 1024-pixel QA improves coverage but does not clear the previous cohort-definition task's pending narrow-layout checks.

## Checks

- `npm run build` and debug macOS `.app` bundle: passed.
- `npm test`: 20 passed, including EN/JA/简体中文 evaluation label/error key coverage.
- `cargo test -p causalmed-core --offline`: 22 passed.
- `cargo test -p causalmed --offline`: 27 passed, 9 opt-in public-file tests ignored; includes the 10,000-history index/package regressions.
- Explicit `evaluation::tests::public_scalar_outcome_restore_relink_abstention_and_project_isolation -- --ignored --nocapture`: passed with public Kang input, independent expression reference, QC trash gate and restore/relink checks.
- `cargo check -p causalmed --offline`: passed. Existing dependency `block v0.1.6` has a Rust future-compatibility warning.
- `CAUSALMED_TEST_DONOR_H5AD=... PYTHONPATH=python .venv/bin/python -m unittest discover -s python/tests`: 33 discovered: 31 passed, 2 processed/raw PBMC tests skipped because their separate files were not supplied.
- `git diff --check`: passed.

## Remaining scope

The initial outcome is one exact count-expression feature with log1p CP10K normalization. Identifier aliases, cross-species conversion, partial feature intersections, multivariate models, confidence intervals and independently reviewed positive public-study pairs remain future work. Identical identifier sets are necessary for this conservative protocol but do not establish comparable assays; measurement review is a separate human attestation. The successful-score path has synthetic software fixtures only.

Original matrices, raw donor and cell vectors stay in the worker. Feature lists, annotation tables and QC selection arrays still occupy Python memory; this increment does not impose a global process memory cap or package the Python runtime for clean-machine installation.
