# Reversible organization and trash validation

Validated on macOS on 2026-11-05 using the native Tauri debug app, public H5AD files and 10,000 on-disk synthetic load histories. This extends [search index validation](SEARCH_INDEX_VALIDATION.md).

The initial live-only package boundary below is superseded by [portable trash validation](PORTABLE_TRASH_VALIDATION.md). Current schema-2 packages include normal and trashed QC histories.

## Behavior and storage

Project and QC lists now have a local Trash toggle with the existing search, filters, ordering and bounded pagination. Normal lists exclude trashed items; trash lists contain only those items and offer Restore. English, Japanese and Simplified Chinese cover actions, empty states, help and errors. Explanations stay behind information controls.

Trash is **logical and reversible**, not the operating system Trash: immutable archives and source files stay in their original directories. A project receives `.project-trash.json`; a QC history receives `.trash/qc/<analysis-id>.json` inside its owning project. Each small versioned tombstone records ID, owner scope and timestamp. This also supports the legacy root project without moving the application directory or its child projects. Restoration checks the tombstone's identity and scope. Project restoration checks its research database; history restoration checks report/manifest consistency and the SHA-256 of `cells.npz` before removing the marker. Failed integrity checks leave the item in trash.

An active project must be switched away from before moving it to trash. The UI disables that action; Rust independently rejects it. Trashed projects cannot be opened. Worker activity and stale project scopes reject organization commands. The command holds the project write gate, excluding selection, queries, package operations, switches and new worker starts during mutation.

Moving the selected QC history first atomically persists a report without its QC/selection fields, preserving dataset annotations and source connection. It clears export receipts and comparisons. Only then is the tombstone atomically published. A crash or marker-write failure between these boundaries can leave a live, unselected history, never a selected pointer to a hidden archive. A fault-injection test covers marker-write failure and subsequent restart/reselection. Restore does not automatically select a result; explicit selection restores its saved conditions, donor mapping, charts and export actions. Pinned comparison choices clear when trash operations or view changes occur.

Source H5ADs, immutable reports/manifests/cell decisions, research documents and source bindings are never deleted, renamed or rewritten by trash operations. Only selected-result metadata, tombstones and disposable indexes change. There is no permanent-delete/empty-trash command.

## Index and package consistency

Search schema 2 adds an indexed `trashed` state. Old caches rebuild from artifacts and tombstones. History changes invalidate the commit signature; reconciliation updates existing rows' trash flags without reopening their reports. Both result counts and returned pages are restricted to the requested bucket. Project catalog summaries include tombstones and live-history counts, and SQL separates live/trash rows. Restore keeps original IDs and dates, so normal ordering remains stable.

Current schema-2 `.causalmed` packages include normal and trash histories and their original timestamps. Local owner keys are not copied; restoration materializes tombstones scoped to the new project. Search caches are rebuilt. See [portable trash validation](PORTABLE_TRASH_VALIDATION.md) for the current package contract. The automated/native records below describe the initial trash implementation.

## Automated validation

- **10,000 disk histories:** moved 100 histories into trash; live count became 9,900 and trash count 100, with two 50-row trash pages. Reconciliation read zero existing report files. A restarted worker retained all 100 trash entries. A second project had zero trash entries. Restoring all 100 returned counts to 10,000/0. Fixture names explicitly identify synthetic load data; their payloads are not biomedical cell decisions.
- **Selected history:** moving the selected run clears persisted selection/comparison; hidden histories reject selection and export. Original report bytes remain identical. Restart preserves trash and cleared selection; restore is unselected until explicit reselection. Duplicate, traversal, stale-scope and busy-worker requests fail safely. Altered cell-decision bytes prevent restoration.
- **Projects:** inactive project and legacy root project round trips preserve the active pointer, names and child-project state. Normal/trash catalog queries return separate counts; trashed projects reject switching.
- **Public H5AD:** independent A/B projects analyze the same Kang checksum with mapped/minimum-200 versus unmapped/minimum-300 conditions. A's selected history is trashed without affecting B. A live-only package restores with no hidden history or stale selection. Project B's trash/restore retains its saved report. A's history restoration, explicit reselection and H5AD export preserve the exact retained matrix, cell IDs and donor values. Original SHA-256 remains unchanged.

All 25 Rust app tests passed, including six opt-in public-file tests. Rust core: 8; frontend: 11; Python: 24 with all configured public inputs. Total: **68 tests**. Frontend/native builds, cargo check and workspace Clippy with warnings denied pass. The existing upstream `block` future-compatibility notice remains.

## Native real-data round trip

The source is the public Kang H5AD: 24,673 cells × 15,706 features, SHA-256 `e6a5adac64dcdeb36eaba27db49b63e0c64bb0ed4a64c6705971506b41c39830`.

1. In `project-Hoeqja` (Kang A), pinned comparison choices and moved selected history `3f04f29b-3d05-4c55-b62f-f64071836c5b` to trash. Normal count changed 4→3; the selection, comparison targets and saved-result export actions cleared. Trash contained exactly that one history with its original donor/count summary.
2. Moved inactive `project-sTLPOd` (Kang B) to project trash. Normal project count changed 4→3 and the trash search returned B alone. Active A's trash action was disabled.
3. Quit and reopened the app. Active A remained active; project B was still in trash. Restored B, switched to it, and verified its original two histories, selected `bb8af9f9`, unmapped donor setting and relocated source connection. B's QC trash was empty; A's trashed result was absent from it.
4. Switched back to A. Its live count was three with no selected result; its trash still held `3f04f29b`. Restored the history, verified no automatic selection, then explicitly selected it. The original minimum-200 conditions, `replicate` mapping, 24,562 retained and 111 excluded returned.
5. Exported from the restored history using the native save dialog to `/private/tmp/causalmed-trash-restored.h5ad`. Independent Python/anndata checks confirmed shape **24,562 × 15,706**, exact sparse matrix equality to original retained rows, exact observation IDs and exact donor-column values. Output SHA-256: `b98223dd53ee530570657be178b11196f5dd71676e7daf54ec181862f175e06e`.
6. Verified source SHA-256 unchanged, all validation tombstones removed after the round trip, A active and its previous result selected. A final app restart preserved the restored state. Checked the explicit empty-trash message and returned to the normal four-history list; captured the rendered desktop screen.

Kang lacks matching mitochondrial features and remains partial QC. These checks establish storage/workflow behavior, not biological validity. Native Windows/Linux and dedicated narrow-window QA remain separate roadmap work. Project discovery and changed-directory reconciliation still enumerate IDs; short-only searches can scan indexed metadata. Trash consumes local disk space; current schema-2 portable packages include its QC histories.

Run the opt-in Rust checks with the same public paths as [index validation](SEARCH_INDEX_VALIDATION.md):

```sh
CAUSALMED_TEST_H5AD=/private/tmp/causalmed-pbmc3k.h5ad \
CAUSALMED_TEST_DONOR_H5AD=/private/tmp/causalmed-kang2018.h5ad \
cargo test -p causalmed --lib --offline -- --include-ignored
```
