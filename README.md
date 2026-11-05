# CausalMed

CausalMed is a local, evidence-first desktop workspace for researchers studying biomedical causal relationships. It brings source records, single-cell datasets, falsifiable hypotheses, and reproducible evaluation into one visual workbench.

The project begins with the research questions discussed in “医学AIの活用例” and uses idiopathic pulmonary fibrosis (IPF) as its initial organizing context. Its purpose is to help researchers move from an interesting association to a clearly stated, testable intervention hypothesis, while preserving the evidence and uncertainty behind that hypothesis.

**Status: early research software.** The app can inspect evidence, import H5AD files, preview expression quality control, map a donor annotation, and execute a synthetic baseline evaluation. It has not established a new treatment target, demonstrated causal discovery performance, or validated clinical predictions. Public PBMC datasets are used to test ingestion and QC; they do not validate the IPF research hypothesis.

## Why this project exists

A molecular association, an intervention experiment, and a proposed mechanism answer different questions. A useful research tool should keep those distinctions visible throughout the workflow. It should also make it possible to trace a claim back to its source, compare contradictory records, identify differences in experimental conditions, and state what future result would disprove a proposed explanation.

CausalMed is designed around that workflow:

1. **Inspect the evidence.** Review a relationship together with its source, excerpt, cell model, experimental context, and explicitly linked contradictory records.
2. **Understand the data.** Inspect an H5AD dataset's dimensions, annotations, count distributions, missing values, and donor structure before attempting evaluation.
3. **Form a falsifiable hypothesis.** Describe the intervention, its context and controls, and a concrete falsification criterion. A saved hypothesis remains unvalidated.
4. **Evaluate without confusing cells with independent replicates.** The current synthetic M0 protocol separates datasets, perturbations, and donors, and aggregates measurements at donor level. Actual H5AD outcomes are not connected to this evaluator yet.
5. **Escalate model complexity only with evidence.** M1 and more advanced models belong after reliable baselines, independent evaluation, calibration, and explicit abstention rules.

The intended output is an inspectable research argument and a reproducible experiment plan. Source provenance and uncertainty are part of the result, rather than information to hide behind a single score.

## Visual interaction

The interface prioritizes compact panels, direct manipulation, and visual summaries. Evidence is shown in a searchable relationship ledger with a detachable inspector. Dataset annotations use composition bars and labeled proportions. Expression QC uses histograms, threshold markers, a retention summary, and per-donor comparison bars. Explanations are available through information icons.

The interface supports English, Japanese, and Simplified Chinese. Scientific annotation values are preserved as supplied by the dataset. The renderer currently previews one relationship in a native GPU window; full-network exploration and cell embeddings are planned.

## Current capabilities

- Distinct observation, intervention, and hypothesis records, with provenance and contradictory-record links.
- Validated evidence JSON imports with SHA-256 provenance, duplicate rejection, and transactional SQLite persistence.
- Hypothesis drafts with required intervention plans and falsification criteria; draft, planned, and archived states.
- Right/bottom docking and a floating inspector synchronized through Rust-owned state.
- Local H5AD metadata ingestion, expression QC previews, configurable thresholds, and explicit donor-column mapping.
- Managed Python worker with progress, cancellation, configurable deadlines, child-process cleanup, and preservation of the last successful result on failure.
- Synthetic donor-weighted M0 evaluation with a reproducible manifest, plus an independent Python reference CLI.
- JSON research snapshots. These are not yet restorable project packages and do not include the separately stored H5AD report.

Detailed implementation status and next steps are maintained in **[docs/ROADMAP.md](docs/ROADMAP.md)**. Architectural boundaries are documented in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Development setup

Requirements: Node.js 22.12 or newer, Rust 1.88 or newer, and the native prerequisites for Tauri 2 on your operating system. The H5AD worker has been tested with Python 3.12.15 and anndata 0.12.19 on macOS.

```sh
npm ci
python3.12 -m venv .venv
.venv/bin/python -m pip install -r python/requirements.lock.txt
npm run tauri dev
```

The development app automatically detects `.venv/bin/python` in this checkout. To use another environment, set `CAUSALMED_PYTHON` to its Python executable's absolute path before launching the app. Without either environment, it falls back to `python3`. Python is not yet bundled for distribution. The dependency lock records the tested environment; `python/requirements.txt` specifies the supported anndata range.

For a read-only browser preview:

```sh
npm run dev
```

Open `http://127.0.0.1:1420`. Importing, saving, worker execution, floating panels, and native GPU rendering require the Tauri app.

To build a development macOS app:

```sh
npm run tauri build -- --debug --bundles app
```

Output: `target/debug/bundle/macos/CausalMed.app`. Distribution signing and notarization are not configured.

## First workflow

1. Open **Evidence**, select a relationship, and inspect its source, conditions, and contradictions.
2. Use **Create hypothesis** to save a question, intervention plan, and falsification criterion.
3. Use the inspector's placement controls to dock it on the right or bottom, or detach it into another window.
4. Open **Datasets** and import a local H5AD file. Review the dimensions and annotation distribution, then inspect the expression QC result.
5. Select the biological donor column, adjust QC settings if necessary, and run QC again. Graphs display the last successfully saved analysis until the new run succeeds.
6. Open **Benchmark** to run the synthetic M0 workflow. It remains separate from the imported H5AD data.

`fixtures/evidence.synthetic.json` documents the evidence import format. It is seeded on first launch, so importing that exact file again is rejected as a duplicate. All bundled evidence relationships are synthetic.

## Expression QC and donor mapping

QC is a read-only preview. It does not remove cells, rewrite expression values, or modify the source H5AD.

Choose `X`, `raw`, or an available layer as the count matrix. The worker inspects the selected values for nonfinite, negative, and noninteger entries. If these occur, count-based QC is withheld and the report asks for an appropriate count matrix. A name such as `raw` is not proof that a matrix contains raw counts.

For a compatible matrix, the worker calculates total counts and detected genes per cell, and the percentage of counts assigned to mitochondrial features. Gene symbols can come from the feature index or an explicitly selected variable column. Prefix matching is case-insensitive and configurable. If no mitochondrial features match, mitochondrial percentage is unavailable and the report is marked partial. Zero-count cells are exclusion candidates, and their undefined mitochondrial percentages are omitted from that histogram.

Histograms aggregate all cells into 32 bins. Count and gene axes use `log10(1 + value)`; mitochondrial percentage uses a linear axis. Saved threshold markers are shown on the graphs. The retention summary uses the union of exclusion rules, while the individual rule counts may overlap. Initial thresholds are examples, not universal biological criteria. Filtering decisions require dataset-specific review.

Donor mapping is explicit. Choose the annotation column containing biological donor identity; do not substitute cell type, cluster, batch, or a per-cell barcode. Missing and blank IDs remain missing. Stored labels are preserved rather than silently merged or renamed. Per-donor bars compare total cells with QC-retained candidates. Mapping does not establish that donor identity is correct or that donors are independent; it also does not make the dataset eligible for the separate M0 evaluator.

The worker reads expression values in chunks and returns bounded aggregate summaries. Expression matrices, per-cell QC vectors, and frame buffers do not cross the Rust–JavaScript boundary. AnnData can still load annotations and ancillary metadata into Python memory; deadlines bound runtime, not peak memory usage.

## Worker lifecycle and storage

The default deadline is 120 seconds and excludes time spent in the file picker. Progress shows runtime initialization, hashing, H5AD opening, annotation processing, expression QC, and validation. Progress percentages represent processing stages, not a prediction of remaining time.

Cancellation, timeout, and application exit terminate and reap the Python child. A report is accepted only after a valid result, stdout completion, and a successful process exit. Failures preserve the previous report. Reanalysis verifies the original file's checksum; a changed source must be imported again.

Rust owns research state, persistence, the renderer, and worker state. WebViews render projections of that state. Research records are stored in `research.sqlite3` under the application's `org.causalmed.research` data directory. On macOS this is normally:

```text
~/Library/Application Support/org.causalmed.research/
```

The latest H5AD report, its analysis settings, and a private source-file reference are stored separately in `h5ad-metadata.json`. Only the latest dataset is retained. Keep the original H5AD available for reanalysis. No external AI service or server is connected.

## Verification

```sh
npm run build
npm test
cargo test -p causalmed-core
cargo test -p causalmed --lib
cargo check -p causalmed
cargo clippy --workspace --all-targets -- -D warnings
PYTHONPATH=python .venv/bin/python -m unittest discover -s python/tests
```

Public-file integration tests are opt-in; see [H5AD validation](docs/H5AD_VALIDATION.md) and [QC validation](docs/QC_VALIDATION.md) for the tested inputs, checksums, commands, observed results, and limits.

The dependency-free synthetic reference evaluator can also run independently:

```sh
PYTHONPATH=python python3 -m causalmed_worker \
  --samples fixtures/benchmark.synthetic.json \
  --held-out-dataset held-out \
  --held-out-perturbation GENE-E --synthetic
```

Its fixture yields a control mean of `1.2` and donor MAE of `0.6` in arbitrary units. These values demonstrate pipeline behavior, not biomedical performance.

## Remaining boundaries

Expression QC is not doublet detection, ambient-RNA correction, biological validation, or evidence of causality. Full dataset management, normalized donor identifiers across studies, project-package restoration, multivariate evaluation, M1, confidence intervals, calibration, and independent validation remain staged work in the roadmap.

Current native verification is on macOS. Windows/Linux behavior and a distributable Python runtime require separate validation. If cloud execution later becomes necessary, the planned boundary is an XServer VPS Cloud application/database core with independent workers communicating over HTTPS or a job queue.
