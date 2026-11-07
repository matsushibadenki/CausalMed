# CausalMed roadmap

Updated: 2026-11-07. The first research context is IPF. Public PBMC data verifies software behavior; biological and predictive validation remains separate.

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

## Priority order

1. 🟢 [Done] Stream retained H5AD matrices instead of materializing the complete subset: immediate memory benefit for the existing workflow, verified below.
2. 🟢 [Done] Project-scoped dataset registry: SHA-256 identity, editable provenance/version/license attestation, bounded annotation schemas, explicit column roles, source-specific indexed QC history counts/filtering, persistence and portable restore. Verified with Kang/PBMC and 10,000 histories.
3. 🟢 [Done] Explicit project-wide donor mappings with evidence and draft/review states; timestamped, source-verified eligibility preflight for a conservative cross-study control baseline, with saved/restored QC references and real-H5AD project isolation.
4. 🟢 [Done] Define exact condition/perturbation cohorts from mixed sources and export parent-QC-retained cells with lineage and donor review snapshots. Project-local receipt checks support explicit adoption, portable restoration and eligibility rechecking.
5. 🟢 [Done] Single-gene real-H5AD observation inspection and guarded M0: donor means, exact feature-space/matrix alignment, measurement review, fresh eligibility, explicit abstention and portable audit snapshots. Public Kang values and negative eligibility verified; successful scoring verified only with synthetic fixtures.
6. 🟢 [Done] Per-project worker budgets, live sampled RSS/CPU meters, kernel CPU fallback, numerical-library thread requests, explicit stop reasons and safe retry; real Kang import/export and native UI verified.
7. 🟢 [Done] Bundle pinned Python/dependencies for Apple-silicon macOS 14+, isolated execution without development fallback, full build inventory/audit, relocated-app real-H5AD ingestion/QC/export and resource-stop/retry verification.
8. 🟢 [Done] Multi-value cohort selection with searchable counted checkboxes, frozen OR/AND rules, compatible historical digests, portable restore and mixed-annotation eligibility abstention.
9. 🟠 [Next] Bound annotation memory, strict OS memory containment, signed/notarized clean-machine distribution and additional runtime targets.
10. 🔴 [Later] Multivariate evaluation after validated dataset/donor/perturbation mappings; encryption and full GPU exploration remain subsequent increments.

## Closest next increments

- 🟢 [Done] Dataset provenance/annotation registry stored in Rust-owned Document (128 sources / 4 MiB). User attestation is distinct from biological validation. Exact-SHA QC filters preserve the selected result; normal/trash counts update on organization changes. Old documents load an empty registry. See [registry validation](DATASET_REGISTRY_VALIDATION.md).
- 🟢 [Done] Explicit common donor identities, evidence, draft/review states and study/context/perturbation semantics; no automatic label merging. Conservative preflight checks complete QC, provenance, exact census, missing/independent donors, study/donor/perturbation separation, source/artifact SHA-256 and stale metadata. Portable restoration validates cohort-to-QC references. See [donor alignment validation](DONOR_ALIGNMENT_VALIDATION.md).
- 🟢 [Done] Mixed-source cohort definitions, exact condition AND perturbation AND parent-QC selection, streamed H5AD export with original/common donor IDs and review flags, SHA-bound project-local output receipts, explicit derived-source adoption and portable lineage validation. Real Kang control/stimulus cohorts, project isolation, restore/relink/re-export and eligibility rechecks verified. See [mixed-cohort validation](MIXED_COHORT_VALIDATION.md).
- 🟢 [Done] Scalar count outcome normalization, exact feature-ID alignment, donor charts and guarded real-H5AD M0 execution with saved/reselected input snapshots, stale-review rejection, restore/relink and project isolation. See [real-outcome validation](REAL_OUTCOME_VALIDATION.md).
- 🟢 [Done] Multi-value exact annotation filters: explicit selection, search/select-all/clear, original-source counts, OR within roles and AND with parent QC, immutable canonical lists, legacy digest preservation, donor lineage and conservative mixed adoption. Public Kang 12,859-cell export, restore/relink/re-export and project isolation verified. See [multi-value validation](MULTI_VALUE_COHORT_VALIDATION.md).
- 🟠 [Next] Verified dataset adapters and independently reviewed comparable study pairs; multi-gene outcomes and uncertainty estimates. Broader cohort exploration follows the multi-value workflow.
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
- 🟢 [Done] Stream dense/CSR/CSC retained H5AD matrices, layers, raw, embeddings and cell-pair matrices in bounded numeric blocks; preserve annotations/provenance, with legacy format compatibility and real/public plus memory-probe validation.
- 🟢 [Done] Supervised process RSS/CPU sampling, configurable budgets and thread requests, local persistence/CAS, isolated project settings, resource-stop cleanup and retry; outcome-launch lock ordering and pre-spawn cancellation regression fixed during native QA. See [worker resource validation](WORKER_RESOURCE_VALIDATION.md).
- 🟠 [Next] Bound annotation/metadata memory and add strict OS memory containment. Sampled RSS thresholds and numeric block budgets are not instantaneous allocation caps.
- 🔴 [Later] Connect validated dataset/donor/perturbation mappings to multivariate M0/M1 evaluation with confidence intervals, calibration, and abstention rules.
- 🟢 [Done] Reproducible bundled macOS worker: fixed Python/wheel hashes, offline verified cache, app resource selection without `.venv`/system fallback, runtime indicator, and relocated/isolated public-H5AD checks. See [bundled worker validation](BUNDLED_WORKER_VALIDATION.md).
- 🟠 [Next] Additional runtime targets; validate Linux monitoring and implement Windows process accounting.
- 🔴 [Later] Full GPU graph layout, labels, pan/zoom, picking, and docking integration. Add Rust/wgpu cell-embedding views linked to annotation/QC selection.
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
- ⭕️ [Pending] Windows/Linux native rendering, window behavior, worker lifecycle and resource monitoring. Linux resource code is unvalidated here; unsupported platforms withhold worker execution.
- 🟢 [Done] Multi-value cohort native selection/export, restart persistence and EN/JA/简体中文 display, including final 1,024-pixel compact layout.
- 🟠 [Next] Full 720-pixel native cohort layout and keyboard-only interaction regression.
- 🟢 [Done] Final native scalar-evaluation restart and EN/JA/简体中文 display checks. Saved ISG15 observations and abstention survived restart; Japanese reinspection reproduced eight donor summaries without the transient project-busy banner.
- ⭕️ [Pending] Distribution signing/notarization, fresh-machine installation, and Intel macOS/Windows/Linux runtime packages. Relocation and absent-development-runtime validation passed on the development Mac; this is not a clean-machine certification.

## Verification records

See [H5AD validation](H5AD_VALIDATION.md), [QC and donor validation](QC_VALIDATION.md), [selection/export validation](EXPORT_VALIDATION.md), [architecture](ARCHITECTURE.md), and [UI design](UI.md). See also [browse validation](BROWSE_VALIDATION.md), [search index validation](SEARCH_INDEX_VALIDATION.md), [trash validation](TRASH_VALIDATION.md), [portable trash validation](PORTABLE_TRASH_VALIDATION.md), [bulk validation](BULK_VALIDATION.md), [stream export validation](STREAM_EXPORT_VALIDATION.md), [real-outcome validation](REAL_OUTCOME_VALIDATION.md), [worker resource validation](WORKER_RESOURCE_VALIDATION.md), and [bundled worker validation](BUNDLED_WORKER_VALIDATION.md). Current automated coverage: Rust core 22 tests, Rust app 46 tests including opt-in public-file integration, frontend 21 tests, and Python 33 tests including opt-in public raw/donor datasets. Public-file tests require the documented local files and Python environment.
