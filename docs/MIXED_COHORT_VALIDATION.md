# Mixed-source cohort validation — 2026-11-06

This increment defines exact condition/perturbation subsets of a mixed H5AD and exports their parent-QC-retained cells with lineage and donor review snapshots. Extraction is not evidence that studies or donors are independent, and does not run a predictive model.

## Public input and exact outputs

Input: [Kang 2018 distributed through Pertpy](https://exampledata.scverse.org/pertpy/kang_2018.h5ad), 24,673 cells × 15,706 genes, SHA-256 `e6a5adac64dcdeb36eaba27db49b63e0c64bb0ed4a64c6705971506b41c39830`. Software checks used `replicate` for donors, `cell_type` for condition and `label` for perturbation. These roles are explicit test configuration, not an automatic assertion of biological comparability.

| Definition | Rule combined with parent QC retained | Export shape |
| --- | --- | --- |
| CD4 control | `cell_type == "CD4 T cells"` AND `label == "ctrl"` | 5,560 × 15,706 |
| CD4 stimulated | `cell_type == "CD4 T cells"` AND `label == "stim"` | 5,678 × 15,706 |

The parent QC retained 24,562 cells and excluded 111 under the tested default thresholds. Both CD4 subsets happened to contain no additional parent-QC exclusions. Separate synthetic HDF5 tests verified that a matching zero-library row is excluded by the QC intersection and that missing donors are retained as unconfirmed, not assigned invented IDs.

The original input SHA remained unchanged. The native stimulated output `/private/tmp/causalmed-cd4-stim.h5ad` had SHA `503d79520a017e08728660455000275fb2dfb8df9f0a8f40db1a07d12ba51cc4`; its embedded definition, condition, perturbation, source SHA and shape were independently read back through anndata.

## Verified contracts

| Check | Result |
| --- | --- |
| Exact selection | Independent Python calculation intersected condition, perturbation and the immutable parent `cells.npz` mask; exported source rows and original observation IDs matched exactly |
| Matrix preservation | Public outputs matched the original CSC expression rows exactly. Dense, CSR and CSC synthetic HDF5 fixtures also passed; original donor labels, layers and original row indices were preserved |
| Donor review snapshot | Namespaced common donor IDs and reviewed/draft flags matched the frozen alignment for every exported cell. Missing IDs stayed empty/unreviewed |
| Embedded lineage | `uns` carried exact definition JSON/SHA, parent QC reference/manifest, rule, selection counts and source-row digest; parent QC remained distinguishable from cohort selection |
| Definition integrity | Rust-generated IDs, CAS and snapshot digests rejected stale/forged definitions. Draft names could be corrected before export; exported names were locked |
| Project-local adoption | A matching recorded output SHA and embedded origin plus a fresh committed donor QC were required. Adoption atomically added a derived source and cohort, preserved donor links and reset new-source/semantic review and partition |
| Cross-project separation | The same child file imported into project B could not adopt project A's definition or reviews. Native B displayed 0 definitions and 0 donor cohorts after switching from A |
| Portable restore | Definition filters/digests, output receipts, original/derived QC references, donor mappings and confirmation states survived project save/unpack/validation. Parent observations were checked against packaged immutable artifacts |
| Native round trip | Defined, corrected an unexported caption, exported stimulated cells, imported/re-QC'd with `replicate`, adopted, saved through the native dialog and restored into a new project from B |
| Reconnection | Both original mixed H5AD and derived H5AD were reconnected by full SHA after native restore; the selected derived result retained 5,678 cells and its donor census |
| Re-export after restore | Automated test relocated parent/children, restarted workers, reconnected each SHA and exported the saved definition again |
| Eligibility recheck | Derived observations resolved to single `CD4 T cells` and `ctrl`/`stim` values. Review, partial-QC and independence issues still blocked readiness. The automated train/test setup detected shared study and shared reviewed donor identities |
| Invalid/empty selection | Invalid definition digest and empty intersection were rejected. Original files were unchanged |
| Worker lifecycle | Cohort exports reused staged non-overwrite publication, supervised progress, cancellation and deadlines. Existing lifecycle and failure-cleanup regressions passed |

## Native visual QA

Environment: macOS native debug Tauri/WebView app, Node 22.17.1, Rust, Python 3.12 / anndata 0.12.19. Browser skill unavailable; Computer Use exercised the actual desktop app and native file dialogs. No browser IPC mock wrote research state.

The meaningful `tauri://localhost` screen rendered without a framework overlay. The flow showed exact filter badges, frozen donor-review counts, output counts/SHA, derived-source dimensions, single-valued cohort context, draft donor links and remaining eligibility reasons. Native console logs were not instrumented. A 1380 × 900 desktop screenshot was saved outside the repository at `/private/tmp/causalmed-mixed-cohort-eligibility.jpg`. Additional narrow-window capture was blocked when Computer Use lost window access and then repeatedly timed out; this did not affect the completed native export/adopt/restore/relink/recheck flow. The 720/1024-pixel native layouts and Windows/Linux rendering were not checked in this increment.

## Commands and test results

- `npm run build`, `npm test`: passed; 19 frontend tests.
- `cargo test -p causalmed-core --offline`: passed; 18 core tests.
- `cargo test -p causalmed --lib --offline`: passed; 26 default tests, 8 opt-in public-file tests excluded by default.
- Explicit `public_mixed_cohorts_export_adopt_restore_and_recheck_isolated -- --ignored --nocapture`: passed with public Kang input; independently verified both exact matrix subsets, portable restore/relink/re-export, project isolation and eligibility abstention.
- `cargo check -p causalmed --offline` and debug macOS app bundle: passed.
- `PYTHONPATH=python ... .venv/bin/python -m unittest discover -s python/tests`: 30 discovered, 29 passed, 1 skipped (raw PBMC not supplied). Public Kang and processed PBMC variables were supplied.
- `git diff --check` and original-source SHA checks: passed.

The public integration test uses `CAUSALMED_TEST_DONOR_H5AD` and `CAUSALMED_PYTHON`. It creates temporary project stores and disposable source copies, not mutations of the user's research projects.

## Limits and follow-up

Definitions use two distinct annotation roles and one exact value per role; complete displayed categories are required (at most 20, without truncated labels). The parent must have a captured exact donor census, currently bounded to 30 donors. Missing annotation values do not match. Multi-value filters and a richer cohort explorer remain future work.

There are at most 64 definitions / 2 MiB per project and 64 KiB per frozen snapshot. Each definition retains the latest eight output receipts. Adoption does not overwrite an already registered source. Project packages contain metadata and QC artifacts, not external H5AD matrices or local source bindings. Reconnect files and recheck readiness after restore; prior readiness is never reused.

Public Kang lacks mitochondrial features under this configuration and remains partial QC. Its control/stimulated subsets share a study and biological donors. Splitting one original into different SHA files must not create independent train/test evidence. Donor/provenance/semantic review, comparable outcomes/features and real-data model execution remain separate work. No causal, predictive or clinical performance is claimed.
