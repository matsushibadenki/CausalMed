# Multi-project isolation validation

Validated on macOS on 2026-11-05 using the native Tauri application, the managed Python worker and the public Kang 2018 H5AD from [QC validation](QC_VALIDATION.md). This extends [project-package validation](PROJECT_VALIDATION.md).

## Native test projects

Both projects use the exact same source SHA-256, `e6a5adac64dcdeb36eaba27db49b63e0c64bb0ed4a64c6705971506b41c39830` (24,673 cells × 15,706 genes). Using identical bytes deliberately tests that checksum equality does not merge projects or their settings.

| State | Kang A · replicate | Kang B · unmapped |
| --- | --- | --- |
| Local directory ID | `project-Hoeqja` | `project-sTLPOd` |
| Research document | 6 existing synthetic relationships and 1 hypothesis | Empty document, 0 relationships and 0 hypotheses |
| QC histories | 2 original A runs | 2 newly computed B runs |
| Selected analysis | `3f04f29b-3d05-4c55-b62f-f64071836c5b` | `bb8af9f9-5bc7-4aa9-8065-c6be4c6efa0f` |
| Minimum genes | 200 | 300 |
| Donor mapping | `replicate`, 8 labels | Unmapped |
| Retained / excluded | 24,562 / 111 | 24,297 / 376 |
| Verified source path | `/private/tmp/causalmed-relocated/kang-original.h5ad` | `/private/tmp/causalmed-project-b/kang-b-relocated.h5ad` |

The original legacy-root project remains listed separately as `local`. Creating B through the project dialog produced an empty research document, no source link and no QC histories. Importing the public file into B did not reuse A's history IDs or donor mapping. B's initial run was `33b6dfb8-a194-4e76-bdd5-02ffc8c65663` (minimum genes 200, donor unmapped); the minimum-genes-300 run became the selected result.

B's original test copy was actually renamed from `kang-b.h5ad` to `kang-b-relocated.h5ad`. Native source relinking accepted its full SHA-256 and saved the binding only in B. A's independently verified original-file path remained unchanged. The original public input and both project source copies retain the same original checksum.

## Switch and restart checks

The native project dialog lists project name/ID, active marker, QC-history count, selected result, retention totals, donor column and source connection. A was named through the rename form; B was created through the new-project form. Both controls persisted the names in their own research documents.

A → B → A switching restored the corresponding selected result, history rows, source filename, donor mapping and count summary without rerunning QC. The sets of stored history IDs for A and B are disjoint. An unsaved B form edit (minimum genes 999) did not appear in A; returning to B restored the saved value 300, rather than retaining the temporary 999. A restored 200 and `replicate`.

After quitting/reopening with B active, B's selected run, two histories, unmapped donor and relocated B source returned. Switching back to A and restarting again restored A's own selected past run, histories and source binding. Project revisions advanced monotonically across switches, while the saved scientific settings and cell decisions remained unchanged. Existing local data were never deleted.

## Downstream export verification

After returning to B, the native export dialog produced:

- `/private/tmp/causalmed-project-b/project-b-retained.h5ad`.
- Shape: 24,297 × 15,706.
- SHA-256: `81beb3738e974727dbf84f8ebe4320a5510a75fda40166217d42799d1b9fef35`.

Independent AnnData reopening verified that the complete sparse expression matrix and cell order exactly match B's saved retained-row mask. Its embedded manifest identifies B's selected analysis, minimum genes 300 and an unmapped donor column. A's historical output remains the separately verified 24,562-cell export from [project-package validation](PROJECT_VALIDATION.md). Retention is a QC candidate selection, not a biological finding; neither project has matching mitochondrial features, so both explicitly remain partial QC.

## Automated checks

All 57 tests pass: 16 Rust application tests, 8 Rust core tests, 24 Python tests and 9 frontend tests. New coverage includes legacy-root discovery, empty-project creation, active-marker uniqueness, invalid/path-traversal targets, worker-busy switching rejection, names and monotonic revisions. The public same-SHA integration creates two independent stores and source copies, uses different donor/threshold mappings, relocates B's source, switches repeatedly, reconstructs workers from disk, rejects foreign-project selection IDs and exports the correct retained counts from both projects.

```sh
CAUSALMED_TEST_H5AD=/private/tmp/causalmed-pbmc3k.h5ad \
CAUSALMED_TEST_DONOR_H5AD=/private/tmp/causalmed-kang2018.h5ad \
cargo test -p causalmed --lib --offline -- --include-ignored

cargo test -p causalmed-core --offline
cargo check -p causalmed --offline
cargo clippy --workspace --all-targets --offline -- -D warnings
npm run build
npm test

PYTHONPATH=python \
CAUSALMED_TEST_H5AD=/private/tmp/causalmed-pbmc3k.h5ad \
CAUSALMED_TEST_RAW_H5AD=/private/tmp/causalmed-pbmc3k-raw.h5ad \
CAUSALMED_TEST_DONOR_H5AD=/private/tmp/causalmed-kang2018.h5ad \
.venv/bin/python -m unittest discover -s python/tests
```

Frontend/build commands used the already-installed Node 22.17.1 as documented in project-package validation. The macOS debug app build and workspace Clippy with warnings denied pass.

## Limits

The local catalog lists direct managed project directories and the legacy workspace. Search, pagination, ordering preferences, deletion/archive cleanup and externally managed project folders are future work. Unreadable or invalid databases are omitted from the list. Source-linked status checks path availability; exports and explicit relinking verify source bytes again. Only the active project owns an executable worker, and a busy worker must finish or be cancelled before switching. Comparison choices, unsaved form inputs and export receipts are session-local. Windows/Linux, minimum-width native regression coverage and packaged Python distribution remain pending.
