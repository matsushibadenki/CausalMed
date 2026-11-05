# CausalMed 0.1 — first working increment

The implementation follows the referenced “医学AIの活用例” conversation, with the current Tauri/Rust ownership constraints taking precedence. The first increment is a research workbench and a synthetic M0 evaluation pipeline, not a completed causal discovery model.

## Ownership

```text
React WebViews (main, inspector)
  └─ typed commands / revision notifications / metadata snapshots
      └─ Rust AppState
          ├─ ResearchStore → Document + WorkspaceState → SQLite
          └─ Renderer → native Tauri Window → wgpu surface / GPU buffers

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

The Python CLI is an independent, dependency-free reference for the same M0 calculations. Connecting a managed Python sidecar, cancellation/progress, H5AD input, multivariate outcomes, M1, confidence intervals, calibration and independent biological validation remains future work.

## Rendering

The current native wgpu surface previews **one selected relationship**. Source is left, target right. Intervention evidence uses solid lines; observation/hypothesis use dashed lines. Activation uses an arrow; inhibition a bar; association is undirected. The window title provides molecule identity and evidence type. The main inspector remains the accessible textual equivalent.

GPU resources are stored in Rust AppState; JS sends only a selection command. Window resize regenerates Rust-side geometry. This first surface requires Tauri's `unstable` feature for native non-WebView windows. Pin `Cargo.lock`; retest the native lifecycle before upgrading. Full network layout, label rendering, pan/zoom, picking, LOD and embedding the GPU surface into a docked panel are not implemented.

## Deployment boundary

Local desktop only. No server is provisioned and no real clinical data is included. If cloud execution becomes necessary, use XServer VPS Cloud for App VPS, Managed PostgreSQL and NFS; isolated AI workers consume jobs over HTTPS or a queue. No direct worker-to-managed-database assumption.

Technical references checked during implementation: [Tauri Rust commands](https://v2.tauri.app/develop/calling-rust/), [Tauri native WindowBuilder](https://docs.rs/tauri/latest/tauri/window/struct.WindowBuilder.html), [wgpu surface API](https://docs.rs/wgpu/27.0.1/wgpu/struct.Surface.html).
