# Dataset registry validation — 2026-11-06

This increment implements project-local source registration before real-data evaluation. It does not establish biological validity, licensing rights, cross-study donor identity, or predictive performance.

## Inputs and environment

Native debug Tauri app on macOS, Rust-owned SQLite research document, Node 22.17.1, Python 3.12 / anndata 0.12.19. Browser skill was unavailable; the actual desktop WebView and native file dialogs were tested through Computer Use. No browser preview or browser IPC mock was used for research writes.

Public files were downloaded to temporary storage, not bundled or redistributed:

| Input | Shape | SHA-256 |
| --- | --- | --- |
| [Kang 2018 / Pertpy](https://exampledata.scverse.org/pertpy/kang_2018.h5ad) | 24,673 × 15,706 | `e6a5adac64dcdeb36eaba27db49b63e0c64bb0ed4a64c6705971506b41c39830` |
| [PBMC3k processed / cellxgene example](https://raw.githubusercontent.com/chanzuckerberg/cellxgene/main/example-dataset/pbmc3k.h5ad) | 2,638 × 1,838 | `0db367b991dd95809732b218539ede489bea99113807f62ebd7ccc970025fe38` |

The input hashes were unchanged after verification. Processed PBMC has scaled expression values unsuitable for count QC: it registered successfully with zero saved QC histories, rather than manufacturing cell decisions.

## Verified behavior

| Check | Evidence / outcome |
| --- | --- |
| Native app identity and rendering | `tauri://localhost`, CausalMed; populated registry, no framework error overlay |
| Registration | Kang: 12/12 typed annotation columns; PBMC: 4/4; dimensions and hashes match source files |
| Provenance editor | Saved name, HTTPS source URL, notes and `replicate` donor role; empty license/version/citation kept unreviewed |
| Incomplete review | Checking user review without required provenance fields rejected; no document mutation |
| Exact-source filtering | Kang showed 5 histories; PBMC showed 0; changing the filter did not select a different QC result |
| Trash counts | Moving a new Kang history changed counts 5/0 → 4/1; restoration returned 5/0 |
| Project isolation | A held two records with edited Kang metadata; B initially held none, then independently registered the same Kang SHA with its own name, unreviewed metadata and 2 histories |
| Restart | A's two records, name, source URL, notes and `replicate` role survived native quit/relaunch |
| Source reconnection | Reconnected A's original selected historical result to downloaded Kang; full SHA match displayed |
| Native layout | 1380 × 900 and approximately 1024 × 1240; readable cards, schema table and editing controls. Fixed inherited table styling that wrapped the Japanese missing-values header vertically |
| Portable data | Real Kang integration test saved registry metadata inside a package, validated restored metadata/schema, relinked relocated input and exported historical retained H5AD |
| Large archive | 10,000 synthetic on-disk histories split across two SHA identities: 5,000 each. After a 1,000-item trash batch, each source counted 4,500 live / 500 trash; restart and restoration returned 5,000/0. Another project's counts remained isolated |
| Conflict and validation | Core tests reject stale document revisions, invalid roles, insecure/credential-bearing URLs, duplicate identities and contradictory observations; idempotent registration and unknown-schema upgrade preserve metadata |
| Legacy documents | Missing `datasets` field deserializes as an empty registry |

Native screenshots were retained in temporary storage (`causalmed-registry-desktop.jpg`, `causalmed-registry-narrow.jpg`, `causalmed-registry-editor.jpg`), outside the repository. Native console logs were not instrumented; console health is not claimed. The 720-pixel minimum width and Windows/Linux native rendering were not checked in this increment.

## Commands and results

- `npm run build`: passed; `npm test`: 17 passed.
- `cargo test -p causalmed-core --offline`: 11 passed.
- `cargo test -p causalmed --lib --offline`: 25 passed; 6 opt-in real-data tests excluded by default. The updated `real_project_restore_relink_and_historical_export` was explicitly run with `--ignored --nocapture` and passed using `CAUSALMED_TEST_DONOR_H5AD` and `CAUSALMED_PYTHON`.
- `cargo check -p causalmed --offline`: passed; debug macOS app bundle built successfully.
- `PYTHONPATH=python ... .venv/bin/python -m unittest discover -s python/tests`: 28 discovered, 27 passed, 1 skipped (raw PBMC input not supplied); processed PBMC and Kang environment variables were supplied.
- Large-index focused run: 10,000 histories, cold reconciliation 4,655 ms, warm filtered page 69 ms in this environment; these are local software measurements, not biomedical performance claims.
- `git diff --check`: passed.

An initial public-PBMC run failed because the previous temporary input had been cleaned; downloading the pinned input and rerunning passed. A load-test assertion initially counted a deliberate read from the isolation fixture; resetting its measurement counter before the warm-query phase fixed the instrumentation, and the complete app suite passed.

## Remaining work

Registration is explicit, with at most 128 records / 4 MiB per project. Schema inspection includes at most the first 256 columns and skips names beyond the metadata bound; the UI shows observed/total counts. Older reports can expose unknown types until reimported. Source metadata does not embed paths, annotation values or matrices. Registry roles do not change historical/current QC mappings. Reviewed cross-study donor identities and real-data evaluation eligibility are the next independent increment. Python distribution/resource controls remain unfinished.
