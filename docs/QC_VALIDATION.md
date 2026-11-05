# Expression QC and donor mapping validation

Date: 2026-11-05. Environment: macOS Tauri development app, 1380 × 900; Python 3.12.15, anndata 0.12.19. No source H5AD was modified.

## Inputs and provenance

| Input | Public source | SHA-256 | Observed shape |
|---|---|---|---|
| PBMC3k raw | [Scanpy's referenced H5AD](https://falexwolf.de/data/pbmc3k_raw.h5ad), documented in the [Scanpy loader](https://github.com/scverse/scanpy/blob/1.11.5/src/scanpy/datasets/_datasets.py) | `89a96f1beaa2dd83a687666d3f19a4513ac27a2a2d12581fcd77afed7ea653a1` | 2,700 cells × 32,738 genes |
| Kang 2018 | [Pertpy's referenced H5AD](https://exampledata.scverse.org/pertpy/kang_2018.h5ad), documented in the [Pertpy loader](https://pertpy.readthedocs.io/en/stable/_modules/pertpy/data/_datasets.html) | `e6a5adac64dcdeb36eaba27db49b63e0c64bb0ed4a64c6705971506b41c39830` | 24,673 cells × 15,706 genes |

Files are local test inputs in `/private/tmp/causalmed-pbmc3k-raw.h5ad` and `/private/tmp/causalmed-kang2018.h5ad`. They are not bundled with the application. Upstream URLs are mutable; the checksums identify the bytes actually tested. These are ingestion/QC examples, not IPF validation data or evidence of predictive superiority.

Metric definitions follow the usual [Scanpy QC measures](https://scanpy.readthedocs.io/en/1.10.x/generated/scanpy.pp.calculate_qc_metrics.html): per-cell total counts, number of features with positive counts, and percentage of counts in selected mitochondrial features. Computation is implemented directly with NumPy/SciPy and checked against an independent full-matrix calculation in tests.

## PBMC3k raw results

- Matrix `X`, feature index as gene symbols, case-insensitive `MT-` prefix: 13 mitochondrial features.
- Median total counts: 2,197. Median detected genes: 817. Median mitochondrial percentage: approximately 2.02964%.
- Initial example settings: min genes 200, max genes 6,000, min counts 0, no upper count limit, mitochondrial maximum 20%. Result: 2,699 retained candidates and 1 exclusion candidate.
- Native UI settings changed to max genes 2,500 and mitochondrial maximum 5%. Result: 2,638 retained candidates and 62 exclusion candidates. Histogram threshold markers updated with the saved settings.
- Three histograms and a retention summary were visually checked in the native app. These thresholds are test settings, not a universal filtering recommendation.

## Kang 2018 results

- Matrix `X` contains compatible nonnegative integer counts in the tested input. The matrix is stored in CSC form and is processed by column chunks.
- Initial example settings yield 24,562 retained candidates and 111 exclusion candidates.
- No feature matched `MT-`. Mitochondrial percentage is unavailable; the UI shows partial QC rather than reporting a false 0% value. Retention here reflects only the available count/gene rules.
- Native UI mapping: explicitly select `replicate`, then execute QC. The result has 8 donor labels and no missing donor IDs.
- The per-donor chart displays total and retained candidates. Example: `patient_1039` has 1,102 total cells and 992 retained candidates. Totals over donor groups agree with the whole-dataset result.
- After quitting and restarting the native app, the selected `replicate` mapping, thresholds, aggregate results, and reanalysis availability were restored.
- The labels are treated as supplied annotations. Mapping does not independently establish biological identity or replicate independence.

## Automated checks

Python tests cover dense/CSR/CSC H5ADs, duplicate sparse entries, overlapping exclusion masks, threshold equality, zero-count cells, nonfinite/negative/fractional values, missing mitochondrial features, explicit layer and gene-symbol selection, invalid settings, missing columns, changed source checksums, blank donor IDs, and preserving distinct stored donor labels. Public-file tests compare raw PBMC metrics with an independent full-matrix calculation and verify the Kang donor aggregates.

Rust tests supervise the actual embedded Python script and check metadata/QC persistence, donor-mapping reanalysis, source-reference restoration, stale checksum rejection, and existing cancellation/timeout/output-boundary behavior. Frontend tests cover language-key completeness for QC and worker errors.

```sh
PYTHONPATH=python \
CAUSALMED_TEST_H5AD=/private/tmp/causalmed-pbmc3k.h5ad \
CAUSALMED_TEST_RAW_H5AD=/private/tmp/causalmed-pbmc3k-raw.h5ad \
CAUSALMED_TEST_DONOR_H5AD=/private/tmp/causalmed-kang2018.h5ad \
.venv/bin/python -m unittest discover -s python/tests

CAUSALMED_TEST_H5AD=/private/tmp/causalmed-pbmc3k.h5ad \
CAUSALMED_TEST_DONOR_H5AD=/private/tmp/causalmed-kang2018.h5ad \
cargo test -p causalmed --lib -- --include-ignored
```

## Boundaries

This is a read-only QC preview, not a filtered-dataset export. Per-cell QC vectors and the expression matrix remain in Python; WebViews receive 32-bin histograms, aggregate rule counts, and at most 30 donor rows. Missing donors are counted but are not automatically excluded by expression QC. Gene symbols, mitochondrial prefix and matrix provenance need dataset-specific review. Integer-like values are required within an absolute tolerance of `1e-6`; this check cannot prove that a count matrix is unnormalized. An initial import requires a readable `X`; subsequent QC can select available raw/layer matrices. There is no doublet or ambient-RNA detection.

Only the latest report/settings/private source reference are retained. QC results are not yet connected to the M0 evaluator or included in research JSON exports. Native UI validation in this run covers macOS desktop; native narrow-window QA, Windows/Linux behavior and packaged Python distribution remain separate work.
