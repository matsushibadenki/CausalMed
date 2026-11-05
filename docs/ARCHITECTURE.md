# CausalMed 0.1 — first working increment

The implementation follows the referenced “医学AIの活用例” conversation, with the current Tauri/Rust ownership constraints taking precedence. The first increment is a research workbench and a synthetic M0 evaluation pipeline, not a completed causal discovery model.

## Ownership

```text
React WebViews (main, inspector)
  └─ typed commands / revision notifications / metadata snapshots
      └─ Rust AppState
          ├─ ResearchStore → Document + WorkspaceState → SQLite
          ├─ Renderer → native Tauri Window → wgpu surface / GPU buffers
          └─ H5AD Worker supervisor → Python subprocess → read-only backed H5AD

Python reference evaluator (CLI, not yet an app sidecar)
  └─ same scalar M0 protocol and donor-level evaluation
```

Search/filter/modal drafts/navigation are transient view state. Research records, evidence selection, locale, docking position and evaluation results belong to Rust. Events contain a revision, and views request a snapshot; older replies are discarded. Floating windows hydrate from the same store. Closing the inspector restores right docking; application restart restores a floating inspector to a docked view.

SQLite stores a versioned document in one transaction. This is intentionally bounded: 2 MiB per evidence import and 10,000 relationships per workspace. No expression matrix enters this document. A failed import leaves the previous document intact. An unsupported document schema fails explicitly rather than silently resetting data. Normalized tables and migrations are subsequent work.

## Evidence and provenance

`fixtures/evidence.synthetic.json` is the import schema example. Every edge carries its source ID, cell type, context, excerpt and explicit conflicting-record IDs. Observational edges must use `associated`. Source type is declared by the importer, not independently adjudicated by CausalMed. Intervention labels therefore must not be interpreted as a claim that the app validated an experiment.

Every import records its original bytes' SHA-256, import timestamp and source IDs. The checksum identifies the imported metadata bundle, **not** the source publication or raw experimental data. URLs are not fetched. Text is rendered as text; no LLM, tool execution or external AI transmission is connected.

Hypotheses require a title, intervention plan and falsification criterion. States are draft/planned/archived. They always remain unvalidated, with safety unknown. Creating a hypothesis never modifies the evidence type of its underlying edge.

The export is a JSON snapshot, not a restorable project package yet. Re-import accepts evidence bundles only.

## Evaluation

The bundled dataset is synthetic, scalar-valued and small. `M0-control-mean-v1` averages cells within donors, then averages donor means in the training control group. It predicts that constant for each held-out donor. Donor MAE is computed over those donor means. The displayed values are arbitrary units, not treatment effects or gene-expression validation metrics.

Training excludes the held-out dataset, the held-out perturbation and all test donor IDs. Donor IDs must be globally normalized upstream. Test data must contain exactly one cell condition, and matched training controls and held-out samples each need two independent donors. Otherwise evaluation abstains. Test controls are never used to fit the baseline. The manifest pins input bytes and train/test sample IDs.

The Python CLI is an independent, dependency-free reference for the same M0 calculations. The H5AD worker is now connected independently of the M0 CLI. Multivariate outcomes, M1, confidence intervals, calibration and independent biological validation remain future work.

## Rendering

The current native wgpu surface previews **one selected relationship**. Source is left, target right. Intervention evidence uses solid lines; observation/hypothesis use dashed lines. Activation uses an arrow; inhibition a bar; association is undirected. The window title provides molecule identity and evidence type. The main inspector remains the accessible textual equivalent.

GPU resources are stored in Rust AppState; JS sends only a selection command. Window resize regenerates Rust-side geometry. This first surface requires Tauri's `unstable` feature for native non-WebView windows. Pin `Cargo.lock`; retest the native lifecycle before upgrading. Full network layout, label rendering, pan/zoom, picking, LOD and embedding the GPU surface into a docked panel are not implemented.

## Deployment boundary

Local desktop only. No server is provisioned and no real clinical data is included. If cloud execution becomes necessary, use XServer VPS Cloud for App VPS, Managed PostgreSQL and NFS; isolated AI workers consume jobs over HTTPS or a queue. No direct worker-to-managed-database assumption.

Technical references checked during implementation: [Tauri Rust commands](https://v2.tauri.app/develop/calling-rust/), [Tauri native WindowBuilder](https://docs.rs/tauri/latest/tauri/window/struct.WindowBuilder.html), [wgpu surface API](https://docs.rs/wgpu/27.0.1/wgpu/struct.Surface.html).


## H5AD worker lifecycle

Rust owns a single active import and its last successful report. Commands reserve an import before opening the native picker, so concurrent starts are rejected. The process deadline begins at spawn, excluding time in the picker. Progress is streamed as NDJSON over stdout; a bounded reader and bounded channel enforce a 256 KiB total protocol limit. Stderr is drained without retaining tracebacks. UI receives only bounded metadata snapshots, polled at 150ms while active and 1s while idle; no WebView owns job state.

Cancellation and deadline checks run every 25ms while the supervisor receives messages. Both kill and wait for the child. App exit also requests cancellation and reaps the child. A process result is accepted only after stdout EOF and successful process exit, with no protocol failure or cancellation. The existing report is replaced via a temporary-file rename only after that boundary. The last report is persisted outside the SQLite research Document, is shared through AppState, and is not yet included in research snapshot exports. Runtime job state resets on restart.

The embedded Python script is launched using CAUSALMED_PYTHON, the development .venv, or python3, in that priority. This is a managed subprocess, not yet a packaged standalone sidecar. The source file remains local. SHA-256 pins file bytes; an unchanged size/mtime check detects ordinary concurrent modification. Provenance is unknown unless reviewed; importing a file never automatically declares real/synthetic origin, donor independence or biological validity. AnnData backed mode leaves X on disk, but obs/var and ancillary AnnData metadata can still consume Python memory. Cancellation/timeouts bound duration, not peak memory.


## Expression QC and donor mapping

The worker emits only aggregate QC output: three 32-bin histograms, per-rule counts, a union-based retention summary and up to 30 donor groups. No per-cell vector, identifier list or expression matrix is sent to the WebView. Dense/CSR inputs are scanned in row chunks; CSC inputs in column chunks to avoid repeatedly scanning the complete sparse structure. Temporary cell metric vectors stay in Python. The native GPU renderer remains responsible for large interactive rendering; these bounded aggregate charts use lightweight SVG in the UI.

QC checks finite, nonnegative, integer-like stored values before using count-based formulas. Transformed/invalid matrices do not produce retention decisions. Settings select X/raw/layer, feature-index or var-column symbols, a mitochondrial prefix, and thresholds. Missing mitochondrial features yield partial QC; zero-library cells have undefined mitochondrial percentages and are excluded from that histogram. Per-rule exclusion counts overlap, while retained/excluded totals use the union of masks. No source file is filtered.

Donor mapping is an explicit obs-column choice. IDs retain their stored labels; missing or whitespace-only labels remain missing. No biological identity or independence is inferred from the number of labels. Expression filtering does not automatically remove missing donors. This preview does not yet create a reusable cell-selection artifact or connect to M0.

For reanalysis, Rust retains a private source path and checks the requested report checksum. Python hashes the file again and rejects changes. The persisted H5AD JSON now contains an envelope with report and sourcePath; legacy report-only files remain readable and prompt reimport before reanalysis. Draft UI settings are transient; the successfully computed settings in the report are authoritative. Cancellation or failure preserves the last report and source reference.
