# CausalMed roadmap

Updated: 2026-11-05. The first research context is IPF. Public PBMC data verifies software behavior; biological and predictive validation remains separate.

Status notation:

- 🟢 [Done] Implemented in the current codebase.
- 🟠 [Next] High-priority unfinished work.
- 🔴 [Later] Planned, but not the closest next step.
- ⭕️ [Pending] Validation cannot currently be performed in the development environment.

## Research workspace foundation

- 🟢 [Done] Tauri 2 + Rust ownership of Document, WorkspaceState, persistence, Renderer, and worker state. WebViews display projections.
- 🟢 [Done] Transactional SQLite persistence, revision synchronization, and restart restoration.
- 🟢 [Done] Searchable evidence ledger, distinct observation/intervention/hypothesis records, source excerpts, context, and explicit contradictions.
- 🟢 [Done] Evidence JSON validation, duplicate rejection, SHA-256 provenance, and research snapshot export.
- 🟢 [Done] Falsification criteria and intervention plans required for hypothesis drafts; planned/archived states remain unvalidated.
- 🟢 [Done] Right/bottom docking and floating inspector, verified through placement controls on macOS.
- 🟢 [Done] Native Rust/wgpu preview of one selected relationship; no frame-buffer transfer through JavaScript.
- 🟢 [Done] English, Japanese, and Simplified Chinese UI.
- 🟢 [Done] Synthetic scalar M0 baseline with donor-weighted aggregation, dataset/perturbation/donor holdouts, and abstention on insufficient independent donors.

## H5AD ingestion and expression QC

- 🟢 [Done] Development Python 3.12 environment with pinned dependencies and automatic `.venv` detection.
- 🟢 [Done] Managed Python process, streamed stage progress, cancellation, configurable deadlines, child-process cleanup, and retry.
- 🟢 [Done] Persist the last successful report/settings/source reference. Failed or cancelled analysis leaves the previous result intact.
- 🟢 [Done] Bounded annotation composition charts, missing/other categories, duplicate-ID reporting, and public H5AD ingestion checks.
- 🟢 [Done] Chunked dense/CSR/CSC expression QC: total counts, detected genes, mitochondrial percentage, zero libraries, and invalid numeric values.
- 🟢 [Done] Explicit selection of matrix, feature-symbol column, mitochondrial prefix, and count/gene/mitochondrial thresholds. Source checksum verified before reanalysis.
- 🟢 [Done] Read-only retention preview, 32-bin histograms, saved threshold markers, and overlapping exclusion-rule counts.
- 🟢 [Done] Explicit donor-column mapping, missing donor reporting, preserved donor labels, and per-donor total/retained comparison bars.
- 🟢 [Done] Public PBMC3k raw native UI: three QC distributions, threshold changes, and 2,638 retained / 62 exclusion candidates under the tested settings.
- 🟢 [Done] Public Kang 2018 native UI: mapping `replicate` to 8 donors; 24,562 retained / 111 exclusion candidates. Missing mitochondrial features shown as partial QC.
- 🟢 [Done] English README documenting the project's purpose, scientific distinctions, operation, architecture, and current limits. Validation records live under `docs/`.

## Closest next increments

- 🟠 [Next] Multi-dataset registry with dataset provenance, version/license review, annotation schemas, and explicit cross-study donor normalization.
- 🟢 [Done] Save per-cell retained/excluded decisions, overlapping rule flags, metrics, donor IDs and source-row identities with checksummed QC manifests.
- 🟢 [Done] Export complete QC results as ZIP and retained rows as a new H5AD, including conditions and donor mapping. Preserve original data, reject existing destinations, and publish only after successful worker verification.
- 🟢 [Done] QC history list, condition/result comparison, exact cell-decision transitions, persistent reselection and export from past runs, verified with public Kang H5AD.
- 🟢 [Done] Portable project save/restore with research document, all live QC histories, conditions, donor mappings, cell decisions and selected result; checksummed artifacts and staged project activation.
- 🟢 [Done] Relocate original H5AD by full SHA-256 verification, persist local source bindings and export restored past results.
- 🟢 [Done] Local project list, empty-project creation, naming and switching; isolate research records, QC histories/options/donor mappings and source bindings, with persisted active project and cleared temporary UI state.
- 🟢 [Done] Search, combined filters, stable sorting and bounded pagination for project/QC lists; comparison choices persist across pages, with project-scoped queries and real-H5AD isolation checks.
- 🟢 [Done] Project-local persistent SQLite/FTS5 search indexes, incremental new-history reconciliation, indexed catalog queries, scope isolation and rebuild after portable restore; validated with public H5AD and 10,000 on-disk histories.
- 🟢 [Done] Reversible local project/QC trash and restore, selected-result cleanup, scoped live/trash indexes, restart persistence and restored-result H5AD export; validated with public data and 10,000 histories.
- 🟠 [Next] Encrypted packages and future package schema migrations.
- 🟢 [Done] Schema-2 project packages include normal/trash QC histories, classification timestamps, conditions and donor mappings; new-owner tombstones, selected-state validation, schema-1 reading and two-generation 10,000-history verification.
- 🟢 [Done] Page-spanning project/QC multiple selection, bulk trash/restore with complete preflight, explicit per-item outcomes, scoped selection reset and interruption-safe index invalidation; validated with real H5AD and 1,000-item batches over 10,000 histories.
- 🟠 [Next] Memory-bounded/chunked H5AD export for larger datasets.
- 🟠 [Next] Connect validated dataset/donor/perturbation mappings to multivariate M0/M1 evaluation with confidence intervals, calibration, and abstention rules.
- 🟠 [Next] Package the Python worker for distribution; add memory/resource controls beyond runtime deadlines.
- 🟠 [Next] Full GPU graph layout, labels, pan/zoom, picking, and docking integration. Add Rust/wgpu cell-embedding views linked to annotation/QC selection.
- 🔴 [Later] Future package schema migrations and optional inclusion of original source datasets.
- 🟠 [Next] Native narrow-window and keyboard QA, native drag-docking regression coverage, and additional panels.

## Subsequent research and expansion

- 🔴 [Later] Literature extraction with human review of sources and claims.
- 🔴 [Later] Independent perturbation evidence integration, experimental-context comparison, and scPerturb adapters.
- 🔴 [Later] Doublet/ambient-RNA workflows and advanced quality models with separately validated assumptions.
- 🔴 [Later] Advanced causal models only after credible M0/M1 evidence, OOD evaluation, and ablations.
- 🔴 [Later] Cloud execution if needed: XServer VPS Cloud core with independently deployed workers communicating over HTTPS/job queues.

## Validation awaiting another environment

- ⭕️ [Pending] Independent biological and prospective laboratory validation.
- ⭕️ [Pending] Windows/Linux native rendering, window behavior, and worker lifecycle.
- ⭕️ [Pending] Distribution signing/notarization and clean-machine installation with packaged Python.

## Verification records

See [H5AD validation](H5AD_VALIDATION.md), [QC and donor validation](QC_VALIDATION.md), [selection/export validation](EXPORT_VALIDATION.md), [architecture](ARCHITECTURE.md), and [UI design](UI.md). See also [browse validation](BROWSE_VALIDATION.md), [search index validation](SEARCH_INDEX_VALIDATION.md), [trash validation](TRASH_VALIDATION.md), [portable trash validation](PORTABLE_TRASH_VALIDATION.md), and [bulk validation](BULK_VALIDATION.md). Current automated coverage: Rust core 8 tests, Rust app 31 tests including opt-in public-file integration, frontend 16 tests, and Python 24 tests including opt-in public raw/donor datasets. Public-file tests require the documented local files and Python environment.
