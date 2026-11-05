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

The legacy JSON export is a research snapshot; evidence import still accepts evidence bundles. Separate `.causalmed` project save/restore includes research state and QC artifacts.

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

Donor mapping is an explicit obs-column choice. IDs retain their stored labels; missing or whitespace-only labels remain missing. No biological identity or independence is inferred from the number of labels. Expression filtering does not automatically remove missing donors. Compatible analyses now create reusable cell-selection artifacts; these remain separate from M0 evaluation.

For reanalysis, Rust retains a private source path and checks the requested report checksum. Python hashes the file again and rejects changes. The persisted H5AD JSON now contains an envelope with report and sourcePath; legacy report-only files remain readable and prompt reimport before reanalysis. Draft UI settings are transient; the successfully computed settings in the report are authoritative. Cancellation or failure preserves the last report and source reference.


## Selection archives and export publication

Every compatible QC run writes `cells.npz` and `manifest.json` inside a Rust-owned pending directory. NPZ arrays contain the full source row positions/IDs, retention mask, reason bits, metrics and donor labels; no pickle payloads are used. The manifest records the source checksum, algorithm/options, donor mapping, summary, timestamp and NPZ checksum. Only bounded selection identity/digests/counts enter the WebView report.

After clean worker exit, Rust verifies the manifest binding and promotes the run directory, then saves the report pointer. Earlier committed run directories are retained; failed/cancelled runs do not replace the previous report. Legacy report-only JSON remains readable but has no exportable selection until QC is rerun. History browsing and archival cleanup are future work.

A native save dialog chooses a new destination. Export commands identify the exact saved selection by analysis ID, reject stale IDs, and share the single-flight worker supervisor. Python verifies artifact hashes. H5AD export additionally checks the original checksum, source row identities and shape, slices retained rows, preserves AnnData components, and embeds a namespaced manifest and per-cell metrics. It verifies the staged output and rehashes the original before success. ZIP export writes all cell decisions and the manifest, without requiring the original H5AD to remain available.

The worker can only write a Rust-owned staging file. Rust checks completion, receipt shape and file length, then uses `NamedTempFile::persist_noclobber` to publish without overwriting a destination. Cancellation, timeouts, invalid worker responses and nonzero exit drop the staged file. Source H5ADs are opened read-only, and existing targets are rejected. The publication point is the completion boundary; a cancellation arriving after publication is a no-op. Runtime export receipts do not replace research/QC state.

Selected expression arrays are materialized in Python memory for H5AD serialization. Memory limits and chunked export are not implemented. Crash-orphaned hidden staging/pending directories are never treated as committed runs or outputs; automatic orphan cleanup is not implemented.

### QC history

Each committed run stores `report.json` (the full report and private source path) beside its checksummed manifest and compressed cell decisions. `h5ad-metadata.json` is the atomically replaced selected-run pointer. Rust enumerates run metadata, restores selection and handles exports; comparison executes in the existing supervised Python worker. Only bounded aggregate transition counts reach the WebView. Different source checksums permit condition/summary comparison only. Legacy archives recover their original conditions from the manifest; only archives matching the selected source can recover its private path and annotation metadata. Other legacy runs remain viewable/comparable and support ZIP export, but require the original source binding before H5AD export. The source-relink control verifies the full source SHA-256 and stores a local checksum-to-path binding. Corrupt archives are excluded from enumeration; no archive is deleted.

### Portable projects and source relocation

`project.rs` writes a versioned streaming archive: magic, bounded JSON entry index, then indexed payload bytes. Every entry has a SHA-256 and length. Only document/selected metadata, schema-2 `history-state.json` classification metadata, and `qc-runs/<analysis-id>/{report.json,manifest.json,cells.npz}` are allowed. No original H5AD or local source path is included. Restoration stages all content in a fresh `project-*` directory, verifies hashes and cross-references, prepares SQLite and the selected report, then atomically replaces `active-project.json` before swapping the live Rust store and worker. Research revisions increase across switches, and a project-key change refreshes history projections even when the selected analysis ID is unchanged. Previous directories remain intact. A project read/write gate prevents research mutations and worker starts during a switch; restoration refuses an active worker.

`source-bindings.json` is local to each project generation. A supervised hash-only worker operation verifies a candidate H5AD with progress, cancellation and deadline handling. Only a clean successful result updates the checksum-to-path binding atomically. History selection and source exports resolve that binding, so all matching histories reconnect without altering immutable QC manifests or decisions. Exports still verify source bytes independently. Package checksums detect changes and truncation; they are not signatures or proof of biological validity.

### Local project catalog and switching

The catalog discovers the legacy root database (`local`) and direct `project-*` directories containing readable research databases. Summary reads use read-only SQLite access, scoped report metadata and per-project source bindings. No catalog row owns research state. Names are persisted in the authoritative document; stable directory IDs resolve the selection. Invalid, missing and symlink directories are excluded or rejected. A root selection writes `folder: "."` to the atomic active pointer.

`AppState::switch_to` takes the project write gate, rejects an active worker, validates/reopens the target research store, advances its revision, creates a fresh worker scoped to that directory, saves the pointer and publishes the new store/worker pair. Source maps are never global, even for identical SHA-256 values. New projects start with an empty document and no QC history or source binding. The frontend refreshes current snapshots after switching and remounts the dataset projection by project key, clearing local forms, comparison choices and transient errors. Graph/inspector windows close rather than retaining an obsolete projection.

### Bounded catalog queries

`browse.rs` validates query bounds and performs metadata search, filtering, deterministic sorting and pagination under the project read gate. Responses include scope, filtered/unfiltered totals and at most 50 summary rows. Initial index construction visits QC report metadata one report at a time; no matrix or cell-decision payload crosses IPC. History queries and selection/comparison commands require the current project key. Frontend query generations discard obsolete responses and the project-keyed subtree clears pinned comparison choices after switching. `search_index.rs` owns disposable project-local SQLite/FTS5 trigram projections, B-tree sort/filter indexes and transactionally synchronized catalog summaries. Warm history queries read no archive reports; commit markers and directory timestamps trigger incremental ID reconciliation. Portable packages exclude caches, and restored projects rebuild under their new scope. Short-token substring checks and catalog discovery retain bounded-metadata scan costs. See [index design and validation](SEARCH_INDEX_VALIDATION.md).

### Reversible organization

`trash.rs` writes versioned, owner-scoped tombstones at `.project-trash.json` or `.trash/qc/<id>.json`, retaining source files and immutable artifacts in place. Organization commands hold the project write gate and reject stale scopes, busy workers and active-project moves. A selected QC report is atomically cleared before its tombstone is published; an interrupted operation can leave an unselected live archive, never a hidden selected result. Restore validates identity and cell-decision hashes, and does not auto-select. Search schema 2 adds an indexed trash flag; incremental reconciliation updates existing row flags without report rereads. Schema-2 portable save includes all run artifacts and an owner-free classification manifest. Restore validates it, materializes new-owner tombstones and removes the portable manifest before activation, preserving one local classification authority. Schema 1 remains readable as normal-only histories. See [trash validation](TRASH_VALIDATION.md) and [portable trash validation](PORTABLE_TRASH_VALIDATION.md) for failure boundaries and tests.

### Bulk organization

`set_trash_batch` holds the project write gate and preflights a bounded, distinct ID selection before mutation. Apply reports explicit per-item success/failure and preserves existing selected-result clearing rules. History invalidation is persisted before the first mutation so interrupted batches rebuild from committed tombstones. Frontend selections contain transient IDs only, scoped to project/kind/bucket; page/query changes preserve them, scope changes reset them, and late receipts cannot clear a new scope. See [bulk validation](BULK_VALIDATION.md).
