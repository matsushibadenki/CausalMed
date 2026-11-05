# CausalMed

CausalMed is a local, evidence-first desktop workspace for researchers studying biomedical causal relationships. It brings source records, single-cell datasets, falsifiable hypotheses, and reproducible evaluation into one visual workbench.

The project begins with the research questions discussed in “医学AIの活用例” and uses idiopathic pulmonary fibrosis (IPF) as its initial organizing context. Its purpose is to help researchers move from an interesting association to a clearly stated, testable intervention hypothesis, while preserving the evidence and uncertainty behind that hypothesis.

**Status: early research software.** The app can inspect evidence, import H5AD files, save expression quality-control decisions, export retained cells as a new H5AD, map a donor annotation, and execute a synthetic baseline evaluation. It has not established a new treatment target, demonstrated causal discovery performance, or validated clinical predictions. Public PBMC datasets are used to test ingestion and QC; they do not validate the IPF research hypothesis.

## Screenshots

Native macOS app shown in Japanese; English and Simplified Chinese are also supported. These screens use public Kang data for workflow verification, not biological or clinical validation.

![QC history with three selections retained outside the current filtered page](docs/images/qc-bulk-selection.jpg)

QC history selection stays across pages, searches and filters. The toolbar counts selected items outside the current page before a bulk action.

![Project browser with three selected projects across pages](docs/images/project-bulk-selection.jpg)

Project organization supports the same page-spanning selection and reversible bulk trash/restore. The active project is protected from selection.

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
- Local H5AD metadata ingestion, expression QC, configurable thresholds, and explicit donor-column mapping.
- Immutable per-cell QC decisions, complete QC archives (ZIP), and retained-cell H5AD exports with embedded conditions and provenance.
- Managed Python worker with progress, cancellation, configurable deadlines, child-process cleanup, and preservation of the last successful result on failure.
- Synthetic donor-weighted M0 evaluation with a reproducible manifest, plus an independent Python reference CLI.
- Portable `.causalmed` project packages with the research workspace, all normal and trashed committed QC histories, conditions, donor mappings, cell decisions and selected result. SHA-256 checks protect each packaged artifact.
- Restore into a new local project generation, then reconnect relocated original H5ADs by full-file SHA-256. Matching histories share the verified source binding. JSON research snapshots remain available separately.

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
5. Select the biological donor column, adjust QC settings if necessary, and run QC again. Graphs display the last successfully saved analysis until the new run succeeds. Compatible runs also save every cell's decision.
6. Export the complete QC results as ZIP, or export retained candidates as a new H5AD. Choose a new filename; existing files are never replaced.
7. Open **Benchmark** to run the synthetic M0 workflow. It remains separate from the imported H5AD data.

`fixtures/evidence.synthetic.json` documents the evidence import format. It is seeded on first launch, so importing that exact file again is rejected as a duplicate. All bundled evidence relationships are synthetic.

## Expression QC and donor mapping

QC leaves the source H5AD unchanged. Each successful count-compatible analysis now saves a reproducible per-cell selection; retained candidates can be written to a separate H5AD.

Choose `X`, `raw`, or an available layer as the count matrix. The worker inspects the selected values for nonfinite, negative, and noninteger entries. If these occur, count-based QC is withheld and the report asks for an appropriate count matrix. A name such as `raw` is not proof that a matrix contains raw counts.

For a compatible matrix, the worker calculates total counts and detected genes per cell, and the percentage of counts assigned to mitochondrial features. Gene symbols can come from the feature index or an explicitly selected variable column. Prefix matching is case-insensitive and configurable. If no mitochondrial features match, mitochondrial percentage is unavailable and the report is marked partial. Zero-count cells are exclusion candidates, and their undefined mitochondrial percentages are omitted from that histogram.

Histograms aggregate all cells into 32 bins. Count and gene axes use `log10(1 + value)`; mitochondrial percentage uses a linear axis. Saved threshold markers are shown on the graphs. The retention summary uses the union of exclusion rules, while the individual rule counts may overlap. Initial thresholds are examples, not universal biological criteria. Filtering decisions require dataset-specific review.

Donor mapping is explicit. Choose the annotation column containing biological donor identity; do not substitute cell type, cluster, batch, or a per-cell barcode. Missing and blank IDs remain missing. Stored labels are preserved rather than silently merged or renamed. Per-donor bars compare total cells with QC-retained candidates. Mapping does not establish that donor identity is correct or that donors are independent; it also does not make the dataset eligible for the separate M0 evaluator.

The worker reads expression values in chunks and returns bounded aggregate summaries. Expression matrices, per-cell QC vectors, and frame buffers do not cross the Rust–JavaScript boundary. AnnData can still load annotations and ancillary metadata into Python memory; deadlines bound runtime, not peak memory usage.

## Saving QC decisions and exporting cells

After a compatible QC run, the app automatically saves a selection archive. It contains every original row index and cell ID, the retained/excluded decision, all exclusion-rule flags, per-cell metrics, donor IDs and missing-donor flags. The accompanying manifest records the input SHA-256, exact QC options, donor-column mapping, partial-QC status, algorithm version, timestamp, and selection hashes. Duplicate cell IDs remain distinguishable through their source row indices. Invalid count matrices produce no exportable selection.

- **Export QC results (ZIP)** saves `manifest.json` and `cells.tsv` for all cells, including excluded cells and overlapping exclusion reasons. This archive is self-contained and can be exported even if the original file has subsequently moved. Numeric mitochondrial percentages are blank where undefined.
- **Export retained H5AD** selects only the saved retained rows, in their original order. Original `X`, variable annotations, layers, `raw`, embeddings, pairwise matrices, and unstructured metadata are preserved with the corresponding row subset. Choosing a different QC input matrix does not replace the original `X` in the export.

The H5AD adds namespaced per-cell QC columns and a namespaced `uns` entry containing the complete manifest as JSON. The namespace includes the analysis ID to preserve existing fields. Mitochondrial values remain undefined when the analysis had no matching features. A partial-QC selection can be exported, and its partial status is recorded. An empty retained selection can be saved in the QC ZIP but cannot be exported as a retained-cell H5AD.

Exports use saved settings, not unsaved form edits. Re-run QC after editing conditions. Retained-cell export verifies the original checksum and row identities before writing, then reopens the staged output to verify its shape, cell order and embedded provenance. Source checksum is checked again before completion. Selection hashes are verified for both formats.

Rust supervises the worker and publishes the destination only after successful completion. Staging files are in the output directory and are removed on ordinary failure, cancellation, or timeout. Publication never replaces an existing destination, including the original H5AD. Application/process crashes can leave hidden staging files; these are not completed exports. Python currently materializes the retained subset in memory for writing, so large exports need sufficient memory. Progress and cancellation remain available; no expression matrix or per-cell decision vector is transferred to JavaScript.

Existing pre-selection QC reports remain readable. Run QC once to create an exportable selection. Export receipts in the UI describe the current session; provenance in the output files and saved selection archives survives restarts. A research snapshot JSON still excludes these separately stored artifacts.

## Worker lifecycle and storage

The default deadline is 120 seconds and excludes time spent in the file picker. Progress shows runtime initialization, hashing, H5AD opening, annotation processing, expression QC, and validation. Progress percentages represent processing stages, not a prediction of remaining time.

Cancellation, timeout, and application exit terminate and reap the Python child. A report is accepted only after a valid result, stdout completion, and a successful process exit. Failures preserve the previous report. Reanalysis verifies the original file's checksum; a changed source must be imported again.

Rust owns research state, persistence, the renderer, and worker state. WebViews render projections of that state. Research records are stored in `research.sqlite3` in the active project directory under the application's `org.causalmed.research` data directory. On macOS this is normally:

```text
~/Library/Application Support/org.causalmed.research/
```

The latest H5AD report, its analysis settings, and a private source-file reference are stored separately in `h5ad-metadata.json`. The screen displays the selected saved result. Immutable decision archives for successful compatible QC runs are retained under `qc-runs/<analysis-id>/`, including each run’s report and private source-file binding. The QC history lists saved runs, compares conditions and retention totals, and restores past results for export. For matching source SHA-256 values, the managed Python worker also compares exact cell decisions (retained in both, excluded in both, added and removed); matrices and cell masks stay outside the WebView. Keep the original H5AD available for reanalysis and retained-cell export; use the source-relink control if it moves. No external AI service or server is connected.

## Saving and restoring a project

Use the save icon in the top bar to create a new `.causalmed` file. It contains the research document, every normal and trashed committed QC history, per-cell decisions, exact QC settings and donor-column mapping, and the selected historical result. It does not embed the original H5AD matrices. Local absolute source paths are removed from the portable package; source filenames, dimensions and SHA-256 identities remain in the reports and manifests.

Use the folder icon to restore a package. Rust verifies its version, whitelisted entry names, lengths, checksums, research-document references and QC artifact bindings before switching to a fresh local project directory. Existing local data remain in their prior directory. The selected-result pointer and research revision are restored consistently; detached inspection/rendering windows close so they cannot show the previous project. On restart, `active-project.json` selects the restored generation.

After restoration, select a saved QC result and choose **Locate original H5AD**. The managed worker hashes the entire file. Only an identical SHA-256 is accepted; a mismatch, failure, cancellation or timeout retains the previous binding. One successful verification reconnects all histories for that source checksum. Bindings persist locally in `source-bindings.json`, while every H5AD export checks the source hash again. You can then export retained cells from any restored historical result without rerunning QC.

New schema-2 packages are CausalMed streaming archives, not ZIP; schema-1 packages remain readable. Schema 2 allows up to 10,000 QC runs across normal and trash, 8 MiB per JSON entry, 4 GiB per artifact and 16 GiB total payload. Saving/restoring needs temporary disk space and currently shows a busy indicator rather than per-byte progress. Local reversible trash is available for project/history organization. Encrypted packages and future schema migrations remain staged work. See [project validation](docs/PROJECT_VALIDATION.md) for the real-data round trip.

## Local projects

Open **Projects** in the top bar to list the original local workspace and restored/created projects. Each row shows the project identity, QC-history count, selected result, retention totals, donor column and source connection. Create an empty project or rename the active one in this panel. Names are display labels; separate local directory IDs identify the projects.

Switching reopens only that project's SQLite document, QC archives, selected report and `source-bindings.json`. An identical H5AD checksum in two projects does not share donor mappings, QC settings, saved decisions or original-file paths. The active project pointer is atomically saved, so restart returns to the last selected project. Research revisions increase across switches to keep window projections synchronized. Detached panels and GPU previews close on switching; temporary QC forms and comparison choices reset to the saved result. Finish or cancel the worker before switching.

The catalog discovers valid databases directly below the app data directory, including the original local database. Invalid project folders are omitted, and direct path traversal/symlink targets are rejected. No project is deleted during switching. Search, pagination, archive cleanup and a delete workflow are future work. See [multi-project validation](docs/MULTI_PROJECT_VALIDATION.md) for same-SHA real-data isolation and restart checks.

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

Public-file integration tests are opt-in; see [H5AD validation](docs/H5AD_VALIDATION.md) and [QC validation](docs/QC_VALIDATION.md) and [selection/export validation](docs/EXPORT_VALIDATION.md) and [QC history validation](docs/HISTORY_VALIDATION.md) for the tested inputs, checksums, commands, observed results, and limits.

The dependency-free synthetic reference evaluator can also run independently:

```sh
PYTHONPATH=python python3 -m causalmed_worker \
  --samples fixtures/benchmark.synthetic.json \
  --held-out-dataset held-out \
  --held-out-perturbation GENE-E --synthetic
```

Its fixture yields a control mean of `1.2` and donor MAE of `0.6` in arbitrary units. These values demonstrate pipeline behavior, not biomedical performance.

## Remaining boundaries

Expression QC is not doublet detection, ambient-RNA correction, biological validation, or evidence of causality. Full dataset management, normalized donor identifiers across studies, future package schema migrations, multivariate evaluation, M1, confidence intervals, calibration, and independent validation remain staged work in the roadmap.

Current native verification is on macOS. Windows/Linux behavior and a distributable Python runtime require separate validation. If cloud execution later becomes necessary, the planned boundary is an XServer VPS Cloud application/database core with independent workers communicating over HTTPS or a job queue.

Project and QC lists now provide metadata search, combined filters, stable sorting and bounded pages. Comparison selections stay pinned across pages; switching projects clears transient queries and comparison choices. Search now uses project-local persistent SQLite/FTS5 indexes with incremental updates and rebuild after restore. See [browse validation](docs/BROWSE_VALIDATION.md) and [search index validation](docs/SEARCH_INDEX_VALIDATION.md) for public-H5AD checks, 10,000-history tests and scaling limits.

Projects and QC histories can be moved to local Trash and restored with their original IDs and source connections. Moving a selected QC history clears its selection and comparisons; restoring leaves it unselected until chosen explicitly. Active projects must be switched away from first. Schema-2 packages include both normal and trash histories with their original classification and timestamps; restored tombstones are owned by the new project. Original H5ADs and immutable QC artifacts remain unchanged. See [trash validation](docs/TRASH_VALIDATION.md) and [portable trash validation](docs/PORTABLE_TRASH_VALIDATION.md) for public-data exports, ownership isolation and two-generation 10,000-history package tests.

Project and QC lists support multiple selection across pages, searches and filters, with explicit counts outside the current page. Bulk trash/restore checks every target in Rust before changes, protects the active project, and reports per-item filesystem failures while retaining failed selections for retry. Project/trash-view changes clear temporary selection. See [bulk validation](docs/BULK_VALIDATION.md) for native public-data checks, interruption recovery and 1,000-item batches over 10,000 histories.
