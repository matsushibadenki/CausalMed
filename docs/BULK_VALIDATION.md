# Bulk organization validation

Validated on macOS on 2026-11-05. Extends [trash validation](TRASH_VALIDATION.md), [portable trash](PORTABLE_TRASH_VALIDATION.md) and [search indexes](SEARCH_INDEX_VALIDATION.md).

## Selection and operation contract

Project and QC lists provide row checkboxes, a page-selection checkbox, a selected count, an explicit count outside the current page, clear-selection and bulk move/restore controls. Page selection includes eligible visible rows only; active projects cannot be selected. Selected IDs remain across search, filters, sorting and pagination. Switching normal/trash views or projects clears the temporary selection. Selection is not saved in research documents or portable packages. These transient WebView IDs do not own research data; Rust revalidates every command.

`set_trash_batch` holds the Rust project write gate and accepts 1–10,000 distinct IDs, a project key, item kind and action. Before changing any item it checks the complete selection: scope, worker state, project eligibility, existing classification, report/manifest consistency and tombstone ownership. Restore also checks the SHA-256 of every cell-decision payload. Invalid, duplicate, stale, busy, active-project or changed-artifact selections fail before mutation.

The apply phase reports `succeeded` IDs and per-item `failed` IDs/error codes. Filesystem errors can produce a partial result; this is deliberately **not** an all-or-nothing filesystem transaction. The UI removes only successful IDs, retains failures for retry, shows counts and exposes individual errors. A preflight error retains the complete selection. Already moved single-row items are also removed from the bulk selection.

A selected QC result is cleared before its marker is published, preserving the established no-dangling-selection boundary. Comparisons clear after organization. Successful restoration does not auto-select. Index invalidation is persisted **before** history mutations, as well as after completion, so a process interrupted mid-batch cannot reuse a stale warm index. Restart reconciles the markers that actually committed, even if no final receipt was delivered. Project catalogs rediscover classification on each query. Original H5ADs, immutable archives, donor/QC options and per-project source bindings are not modified.

No permanent-delete operation was added. Package schema 2 continues to preserve the resulting normal/trash classification.

## Automated checks

- Rust app: **31 unique tests** verified, including the existing full suite and the added interruption check. All configured public-file tests passed.
- **10,000 on-disk histories:** one batch moves 1,000 into trash; normal/trash counts become 9,000/1,000. Existing-report rereads during reconciliation remain zero. Restart preserves the bucket counts; a second project's trash remains empty. One restore batch returns counts to 10,000/0 with exact success receipts.
- **23 inactive projects:** a batch containing the active legacy root rejects without moving any item; the valid 23-project batch moves/restores all and preserves the active pointer and project names.
- Empty, duplicate, oversized, foreign-scope, missing and busy requests fail before mutation. Changed cell-decision bytes stop an entire restore selection before the first marker removal.
- Injected apply-phase filesystem failure returns exactly one success and one failure. The failed selected history remains live but unselected; restart has no dangling selection. An interrupted batch between first marker publication and final notification rebuilds counts correctly on restart.
- Public Kang integration batches a mapped minimum-200 and minimum-300 pair, verifies selected-result clearing and restoration, rejects the same IDs in another same-source-SHA project, then exercises portable restoration and exact retained-matrix/cell-ID/donor export checks.
- Frontend: **16 tests**, including page union/toggle behavior, search-filter subset retention, project/bucket reset, stale-receipt rejection, partial success removal, 10,000-selection bounds and EN/JA/简体中文 parity.
- Rust core: **8**; Python: **24** with all public-file inputs enabled. Total verified coverage: **79 tests**.
- Frontend/native builds, cargo check, Clippy with warnings denied and diff whitespace checks pass. The existing upstream `block` future-compatibility notice remains.

Synthetic load fixtures test storage mechanics only and are explicitly named as such. The real-file checks establish workflow reproducibility, not biological validity or causal performance.

## Native public-data QA

Dataset: public Kang H5AD, 24,673 cells × 15,706 features, SHA-256 `e6a5adac64dcdeb36eaba27db49b63e0c64bb0ed4a64c6705971506b41c39830`.

1. In original A (`project-Hoeqja`), used two rows per page, selected page 1's two histories, then added selected QC result `3f04f29b` on page 2. The toolbar showed three selected, including items outside the page.
2. Searched `replicate "minGenes":300` and filtered donor-mapped results. Only unselected `812d0a28` remained visible; the toolbar still showed **3 selected / 3 outside this page**. Bulk move returned completed 3 / failed 0. The selected QC result and comparison/export state cleared, while the unselected minimum-300 history remained live.
3. Changed to trash, confirming the prior bulk selection cleared. Reset the query, selected two rows on page 1 and the third on page 2, then bulk-restored all three. Receipt: completed 3 / failed 0; trash became empty. Explicitly reselected the previous minimum-200 QC result, restoring 24,562 retained cells and its charts.
4. Selected one A history temporarily, switched to B (`project-sTLPOd`) and verified zero selected, only B's two IDs and its own saved result. Searching A's full analysis ID returned zero of two. Switching back to A still showed zero selected, without resurrecting the old temporary selection.
5. In the project dialog, two-row pages excluded active A from page selection. Selected `project-n4fVw4` on page 1, then `project-VVIfRh` and B on page 2. Searching B showed **3 selected / 2 outside this page**. Bulk move returned completed 3 / failed 0, leaving two normal projects.
6. In project trash, selected the same three across two pages and bulk-restored them. Receipt: completed 3 / failed 0; trash empty; normal project count returned to five. Active A and its previous selected result remained unchanged.
7. Read back the native indexes: A and both restored A copies each have four normal histories; B has two. All have zero trash histories. No project/history tombstones remain after the validation round trip. A is active with `3f04f29b` selected. Original source SHA-256 remains unchanged.

## Screenshots

[QC page-spanning selection](images/qc-bulk-selection.jpg) and [project selection](images/project-bulk-selection.jpg) are native 1380×900 JPEG captures, also embedded in the English README. They show public dataset metadata and QA project names. Desktop rendering and the target interactions were inspected; dedicated narrow-window and other-platform QA remain roadmap work.

Initial preflight/hash verification and per-item marker writes can take time for large real NPZ archives. Operations currently use an in-flight disabled UI rather than per-item progress/cancellation. A process interruption may leave a recoverable partial batch; classification and selection remain consistent on restart. Search discovery/reconciliation still has the documented metadata/ID scan costs.
