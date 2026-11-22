# CausalMed roadmap

Updated: 2026-11-17. The first research context is IPF. Public PBMC data verifies software behavior; biological and predictive validation remains separate.

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
9. 🟠 [Next] Reduce remaining annotation memory and verify signed/notarized clean-machine distribution and additional runtime targets.
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
- 🟢 [Done] Selective read-only H5AD research projection for QC, cohort filtering and scalar outcomes: unused layers, embeddings, pairwise matrices and unrelated `uns` payloads stay on disk; selected dense/CSR/CSC layers remain backed. Legacy categorical tables and dot-layout raw data are retained. Synthetic allocation probe reduced opening high-water growth from 129.3 MiB to 1.0 MiB; this is software allocation evidence, not biological validation. See [selective reading validation](SELECTIVE_READ_VALIDATION.md).
- 🟢 [Done] Modern annotation frames read columns on demand without retaining decoded columns; categorical discovery uses HDF5 encoding metadata. Nested provenance dictionaries load only accessed fields. Nullable values, categories, duplicate row IDs and legacy compatibility are preserved. Synthetic 128 MiB wide-annotation probe reduced opening/one-column high-water growth from 301.0 MiB to 44.7 MiB. See [annotation reading validation](ANNOTATION_READ_VALIDATION.md).
- 🟢 [Done] Modern retained-H5AD export appends QC/donor lineage columns directly to the streamed obs group; verification reads only cell IDs. No full obs-table reconstruction or verification decode. Original categories, nullable values, semantic column order, named/duplicate IDs and matrix/raw payloads are preserved. Synthetic 128 MiB retained-annotation probe used 45.4 MiB export high-water growth with 64 KiB maximum numeric read blocks. See [annotation export validation](ANNOTATION_EXPORT_VALIDATION.md).
- 🟢 [Done] Chunk exact cohort annotation matching into 8,192-row blocks (modern numeric/string/categorical/nullable columns; legacy transformation fallback), and write generated QC/canonical-donor columns in byte-budgeted selected-row blocks for modern and legacy exports. Synthetic 1,048,576-row exact filtering reduced allocation growth from 121.6 MiB to 2.6 MiB with identical selected-mask SHA-256. Generated-block telemetry and typed/boundary/corrupt-input regressions added. See [chunked cohort validation](CHUNKED_COHORT_VALIDATION.md).
- 🟢 [Done] Stream modern annotation schema/nullability and exact category summaries in 8,192-row blocks; spill distinct-label counts to private temporary SQLite with a 1 MiB page cache instead of keeping a full-column frequency table. Preserve exact missing/distinct/top-20/other/complete semantics; equal-frequency labels use deterministic source-first order. A synthetic 1,048,576-distinct-label probe reduced allocation growth from 213.5 MiB to 7.2 MiB with identical summary SHA. Public Kang schema/group counts, retained export and Rust cohort restore/re-export verified. See [annotation summary validation](ANNOTATION_SUMMARY_VALIDATION.md).
- 🟢 [Done] Stream donor QC totals/retained counts and missing-donor accounting in 8,192-row blocks with private temporary SQLite; fill immutable raw donor arrays in blocks after a width scan, preserving nonblank whitespace and reviewed identity semantics. Synthetic 1,048,576-distinct-ID donor preparation reduced allocation growth from 339.6 MiB to 51.8 MiB with identical summary/raw-ID/missing-array hashes. Real Kang donor census, source integrity, H5AD export and restored cohort re-export verified. See [streamed donor validation](STREAMED_DONOR_VALIDATION.md).
- 🟢 [Done] Build the mitochondrial feature mask in 8,192-row blocks from the selected matrix feature index or annotation column. Preserve exact case-insensitive prefix matching and missing-value handling across X, raw and layers. A 1,048,576-feature synthetic probe returned identical masks and reduced additional peak RSS from 67.3 MiB to 46.3 MiB. The feature index and categorical dictionaries still load in memory.
- 🟢 [Done] Defer modern feature indices and stream string IDs for mitochondrial QC and exact duplicate counts. Validate large string category dictionaries in blocks and decode only the referenced labels within each row block. Synthetic probes reduced additional peak RSS from 144.1 to 37.2 MiB for feature IDs and from 111.8 to 39.1 MiB for category dictionaries, with identical masks. [Validation](FEATURE_INDEX_VALIDATION.md).
- 🟢 [Done] Stream large numeric category dictionaries and scalar outcome feature-space matching. Preserve exact numeric labels and the historical sorted JSON SHA-256 using temporary SQLite. Public Kang ISG15 inspection, donor observations and portable restoration verified. [Validation](FEATURE_SPACE_VALIDATION.md).
- 🟢 [Done] Read modern cell indices in blocks for duplicate detection and source/export identity checks. Stream cell IDs, donor IDs and generated QC fields into the existing NPZ schema. A 524,288-ID writer probe reduced additional peak RSS from 190.5 to 6.2 MiB with identical decoded IDs. [Validation](CELL_QC_STREAM_VALIDATION.md).
- 🟢 [Done] Map decoded QC NPZ members from temporary disk storage, validate row identity and retention in blocks, and aggregate expression QC with packed reasons, block histograms and an exact disk-backed median. Read and export previous NumPy archives. [Validation](QC_READ_MEMORY_VALIDATION.md).
- 🟢 [Done] Spill large per-cell expression QC vectors to temporary mappings, reduce CSC totals in row blocks, and build compact disk-backed H5AD export row indices. Preserve historical QC and cohort provenance. [Validation](QC_VECTOR_EXPORT_VALIDATION.md).
- 🟢 [Done] Decode backed CSC blocks against occupied rows, stream oversized sorted single columns, and generate cohort donor metadata only for selected output blocks. [Validation](CSC_COHORT_MEMORY_VALIDATION.md).
- 🟢 [Done] Decode oversized backed CSC blocks with multiple columns, unsorted indices or duplicates through disk-backed ordering and bounded CSR tiles. Preserve duplicate arithmetic and the QC artifact format. [Validation](OVERSIZED_CSC_VALIDATION.md).
- 🟢 [Done] Project legacy compound annotation fields and indices in blocks for QC and H5AD export. Bound variable-length text reads by element count in annotation paths. [Validation](LEGACY_VLEN_VALIDATION.md).
- 🟢 [Done] Stream duplicate checks for complex-valued HDF5 indices, including legacy compound fields. Normalize signed zero and NaN separately for real and imaginary components to match pandas. A 1,048,576-row complex128 index returned the same 524,288 duplicates, with additional peak RSS reduced from 119.78 to 1.83 MiB. A temporary public Kang 2018 copy with complex gene IDs matched pandas duplicate counts and preserved all 24,562 retained cell IDs and expression values in the exported H5AD.
- 🟠 [Next] Investigate an upstream-compatible way to avoid HDF5’s full-label conversion allocation during a single VLEN read. The HDF5 2.0.0 converter sizes a separate internal buffer by the label length. The local single-label comparison found no repeatable gain from reducing the metadata cache from 1 MiB to 256 KiB (three-run medians 114.81 and 114.52 MiB total peak RSS). Reproduce with `PYTHONPATH=python python python/benchmarks/category_single_label_memory.py`. [HDF5 2.0.0 converter](https://raw.githubusercontent.com/HDFGroup/hdf5/2.0.0/src/H5Tconv_vlen.c).
- 🟢 [Done] Request a 4 KiB HDF5 transfer conversion buffer for a single borrowed text value. In three fresh processes reading one 6 MiB label without retries, median total peak RSS was 110.41 MiB before and 109.88 MiB after. The HDF5 VLEN converter still allocates a label-sized internal buffer. Smaller metadata-cache targets and free-list collection showed no reliable benefit in the local probe. [HDF5 converter](https://raw.githubusercontent.com/HDFGroup/hdf5/develop/src/H5Tconv_vlen.c).
- 🟢 [Done] Probe VLEN dictionary labels before batching, reduce subsequent batch sizes after a budget failure, and configure a 64 KiB HDF5 conversion buffer target for borrowed text reads. Three fresh-process runs with two 6 MiB labels and a forced digest match gave median total peak RSS of 135.81 MiB before and 135.36 MiB after. The first single-label native payload remains subject to the worker memory limits.
- 🟢 [Done] Temporarily cap the HDF5 metadata cache configuration at 1 MiB during VLEN category dictionary validation and restore the original settings on success or failure. In three fresh-process runs with two 6 MiB labels and a forced digest match, median total peak RSS fell from 141.62 to 135.86 MiB. HDF5 internal buffers remain subject to worker resource limits.
- 🟢 [Done] Spool distinct long category labels to temporary storage and verify equal-length SHA-256 matches in 64 KiB chunks. Forced collisions distinguish different labels from duplicates without rereading candidates from HDF5. A two-label 6 MiB-per-label probe showed no reduction in end-to-end peak RSS despite removing the second comparison payload. The public Kang 2018 copy with long categories retained 24,562 cells with exact cell-ID and expression parity after export.
- 🟢 [Done] Validate individual long VLEN category labels directly from the HDF5 buffer. Store only SHA-256 digests and source positions for labels larger than 64 KiB, and compare exact bytes on a digest match. A single 6 MiB label reduced additional peak RSS from 42.11 to 34.84 MiB.
- 🟢 [Done] Reuse the 1 MiB multi-label read budget when validating category dictionaries. Validate variable-length text in chunks and bind the original bytes as exact SQLite keys. A 64-label HDF5 probe reduced additional peak RSS from 28.20 to 16.62 MiB.
- 🟢 [Done] Count categorical cell and gene ID duplicates directly from validated HDF5 codes, with bounded block reads and a disk-backed seen vector for large dictionaries. A 1,048,576-row probe with 255 long labels returned the same 1,048,320 duplicates in 0.006 s versus 1.077 s for label expansion and SQLite counting.
- 🟢 [Done] Stream duplicate checks for categorical and nullable HDF5 indices stored as groups. Derive axis lengths from codes or values and preserve missing and empty-label equality. A 1,048,576-row nullable UInt64 index produced the same 524,287 duplicates and reduced additional peak RSS from 56.09 to 5.16 MiB. On a temporary copy of public Kang 2018 with grouped cell and gene indices, duplicate counts matched pandas and QC export preserved all 24,562 retained cell IDs and expression values.
- 🟢 [Done] Check duplicate IDs in numeric HDF5 indices in row blocks with a disk-backed set, including legacy compound indices. Preserve pandas equality for signed zero and NaN. A 1,048,576-row unsigned index produced the same 524,288 duplicates and reduced additional peak RSS from 80.8 to 2.5 MiB.
- 🟢 [Done] Limit multi-label HDF5 reads during annotation summaries to 1 MiB of native string payload per read, while retaining the 8 MiB single-label limit. Stream counted rows to SQLite without a separate parameter list. A long-label H5AD probe reduced additional peak RSS from 23.89 to 15.58 MiB versus the same implementation with the prior read limit, with identical summary hashes.
- 🟢 [Done] Bound the annotation summary label payload returned by SQLite to 800 UTF-8 bytes per displayed category. Preserve exact counts, tie order, and the existing 200-character display prefix.
- 🟢 [Done] Limit HDF5 variable-length string payload allocations to 8 MiB per protected read, split oversized batches, and project nullable and legacy category dictionaries. [Validation](VLEN_BYTE_BUDGET_VALIDATION.md).
- 🟢 [Done] Tile multidimensional variable-length text during H5AD export. Apply the 8 MiB native text allocation budget and split oversized tiles while preserving row and column order.
- 🟢 [Done] Apply the native byte budget to variable-length numeric sequences and copy ordinary HDF5 metadata datasets in bounded tiles. [Validation](HDF5_METADATA_COPY_VALIDATION.md).
- 🟢 [Done] Tile flat compound metadata with a shared variable-length payload budget and avoid reading plain cross-file reference datasets to produce their existing null-reference representation. [Validation](HDF5_SPECIAL_COPY_VALIDATION.md).
- 🟢 [Done] Project nested compound fields under the shared variable-length budget and transfer large fixed-width HDF5 attributes through temporary mappings. [Validation](HDF5_NESTED_COPY_VALIDATION.md).
- 🟢 [Done] Transfer variable-length metadata attributes without Python decoding and project variable-length leaves inside HDF5 array members under the shared tile budget. [Validation](HDF5_VLEN_ATTRIBUTE_ARRAY_VALIDATION.md).
- 🟢 [Done] Reduce HDF5 metadata cache usage during native VLEN attribute transfer and tile compound datasets with isolated reference fields. [Validation](HDF5_REFERENCE_CACHE_VALIDATION.md).
- 🟢 [Done] Pack padded compound attribute staging records and tile datasets whose nested fields mix reference tokens and variable-length payloads. Preserve attribute datatypes and existing HDF5 reference tokens. [Validation](HDF5_MIXED_NATIVE_COPY_VALIDATION.md).
- 🟢 [Done] Apply per-worker kernel address-space limits to whole-value VLEN attributes and compatibility copies. Check the OS-reported peak RSS before publishing and retain committed work on failure. [Validation](WORKER_MEMORY_LIMIT_VALIDATION.md).
- 🟢 [Done] Verify Linux ARM64 worker address-space limits with public H5AD data, including native and compatibility allocation failures, preservation of committed artifacts, and exact QC and export parity after portable restore. Both 10,000-history regressions completed on macOS. [Validation](WORKER_MEMORY_LIMIT_VALIDATION.md).
- ⭕️ [Pending] Validate Windows process accounting and portable worker distribution on additional targets.
- 🔴 [Later] Connect validated dataset/donor/perturbation mappings to multivariate M0/M1 evaluation with confidence intervals, calibration, and abstention rules.
- 🟢 [Done] Reproducible bundled macOS worker: fixed Python/wheel hashes, offline verified cache, app resource selection without `.venv`/system fallback, runtime indicator, and relocated/isolated public-H5AD checks. See [bundled worker validation](BUNDLED_WORKER_VALIDATION.md).
- 🟠 [Next] Additional runtime targets; validate Linux monitoring and implement Windows process accounting.
- 🟢 [Done] Use independent Tauri WebviewWindows for the main workspace, floating inspector and native wgpu preview. The Rust workspace remains the shared source of state.
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
- 🟢 [Done] Keep cohort value search separate from form submission, expose selection counts and empty search results, and wrap cohort actions in narrow layouts.
- 🟢 [Done] Verify the Japanese cohort form in a 720-pixel macOS window. With public Kang data, keyboard search showed the empty state, Enter in the search field kept the draft, and Tab and Space selected CD8 T cells and stim and saved the definition. The saved definition was present after restart.
- 🟢 [Done] Final native scalar-evaluation restart and EN/JA/简体中文 display checks. Saved ISG15 observations and abstention survived restart; Japanese reinspection reproduced eight donor summaries without the transient project-busy banner.
- ⭕️ [Pending] Distribution signing/notarization, fresh-machine installation, and Intel macOS/Windows/Linux runtime packages. Relocation and absent-development-runtime validation passed on the development Mac; this is not a clean-machine certification.

## Verification records

See [H5AD validation](H5AD_VALIDATION.md), [QC and donor validation](QC_VALIDATION.md), [selection/export validation](EXPORT_VALIDATION.md), [architecture](ARCHITECTURE.md), and [UI design](UI.md). See also [browse validation](BROWSE_VALIDATION.md), [search index validation](SEARCH_INDEX_VALIDATION.md), [trash validation](TRASH_VALIDATION.md), [portable trash validation](PORTABLE_TRASH_VALIDATION.md), [bulk validation](BULK_VALIDATION.md), [stream export validation](STREAM_EXPORT_VALIDATION.md), [real-outcome validation](REAL_OUTCOME_VALIDATION.md), [worker resource validation](WORKER_RESOURCE_VALIDATION.md), and [bundled worker validation](BUNDLED_WORKER_VALIDATION.md). Current automated coverage: Rust core 23 tests, Rust app 46 tests including opt-in public-file integration, frontend 21 tests, and Python 75 tests including opt-in public raw/donor datasets. Public-file tests require the documented local files and Python environment.

## Development housekeeping

- 🟢 [Done] 2026-11-09: Removed accumulated Rust build outputs/debug app, generated frontend output, staged bundled Python runtime and local build/bytecode caches. Retained development dependencies, H5AD validation data and pinned runtime download/wheel caches for offline reconstruction. The next build regenerates outputs; a full Rust rebuild is required.

- 🟢 [Done] 2026-11-21: Removed regenerated Rust build outputs, frontend output, staged bundled Python runtime and local build/bytecode caches. Preserved source files, development environments, H5AD validation data and verified offline runtime caches. Outputs require regeneration before the next app build.
