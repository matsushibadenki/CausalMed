# Search index validation

Validated on macOS on 2026-11-05. This extends [browse validation](BROWSE_VALIDATION.md); search counts are workflow metadata, not biomedical findings.

## Design and update boundaries

`search_index.rs` maintains a disposable `history-search.sqlite3` inside each project and a `project-search.sqlite3` catalog at the application root. SQLite FTS5 uses a case-sensitive trigram tokenizer over Rust-normalized lowercase text. Tokens of at least three Unicode characters select indexed candidates. Bound `instr` predicates verify every token and preserve literal punctuation, quotes, `%`, `_`, Japanese/Chinese text and short-token substring behavior. There is no user-supplied SQL or FTS expression. B-tree indexes support date, name, retained-count, mapping and history-count order/filter access. SQL returns counts and only the requested bounded page.

The first history query validates saved reports/manifests and builds the projection transactionally. Warm queries check a directory timestamp and a commit marker, without opening archived reports. After a successful QC commit, the worker replaces `.history-revision`. Reconciliation enumerates analysis IDs, reads only new reports, removes deleted IDs and updates the signature in the same transaction. A racing commit changes the signature again and is reconciled by the next query. The selected-result filter uses the current worker snapshot, so reselection requires no history rewrite. No worker completion takes the index mutex.

Catalog discovery still enumerates projects and reads their bounded authoritative metadata, so it is O(project count), not constant time. Its history counts use the per-project indexes instead of reopening every archive. Summary changes update the catalog table and FTS projection in one transaction; SQL performs project search/filter/order/paging. Unchanged payloads do not rewrite their index rows. Source presence is rechecked, and active/selected/name/binding changes are refreshed on query.

Index files and commit markers are excluded from portable packages by the existing whitelist. Restored projects rebuild solely from verified artifacts under their new project directory. Missing, unreadable-schema or obsolete-version caches are replaced through a staged database. Cache failure is localized in EN/JA/简体中文 and never replaces QC artifacts. History commands retain project-key checks; WebViews never own the index or receive matrices/per-cell decisions.

## Ten-thousand-history checks

The Rust test creates **10,000 separate on-disk report/manifest pairs**, explicitly named synthetic load fixtures. They test list mechanics and are not copied into the user's projects.

- Initial indexing validates all 10,000 reports. A 50-row page reports 200 pages.
- `REPLICATE 200` plus mapped filtering returns exactly 5,000 histories. The final retained-descending page has the expected 50 rows, including stable tie ordering.
- Thread-local read instrumentation proves warm search opens **zero** archive reports. Appending the 10,001st run reads **one** new report; existing reports are not reloaded.
- Addition, removal, worker restart and a second project with a different history set return their own exact counts and IDs.
- Japanese two-character text, literal wildcard characters, embedded quotes and FTS-looking input preserve substring semantics.
- SQLite `EXPLAIN QUERY PLAN` confirms virtual-table index access for trigram candidate search.
- Damaged cache bytes and an obsolete schema version rebuild from the artifacts. Live selected-result filters do not retain a previous selection.

Observed local test runs took roughly 3.4–9.3 seconds for the cold build and 53–106 milliseconds for the warm filtered final page. These are diagnostic wall-clock samples under varying concurrent test/build load, not performance guarantees. The durable assertion is the zero/one archived-report read boundary.

## Public H5AD and native checks

Used the public Kang dataset, **24,673 cells × 15,706 features**, SHA-256 `e6a5adac64dcdeb36eaba27db49b63e0c64bb0ed4a64c6705971506b41c39830`.

Automated public-file integration warms the index after the first QC run, adds a second run with minimum 300 genes, and verifies a new `replicate 300` result with 24,297 retained cells. The already-built project catalog changes its history count from one to two. Reselection updates the selected filter. Save/restore excludes cache files, rejects the old scope, rebuilds two histories, and returns the same filtered result. The restored project becomes the sole active catalog result. Existing same-SHA two-project tests verify donor settings, selected IDs, source bindings and searches remain isolated across repeated switches. Historical H5AD export remains byte/content checked against the source retained rows.

Native Tauri debug app checks:

1. Opened project `project-Hoeqja` (Kang A), indexing its three existing histories.
2. Searched `replicate "minGenes":202`: zero of three. Ran actual QC with minimum 202 genes and donor column `replicate`. Search automatically changed to one of four: `b6328bc4-aa4e-4696-9082-19354deab1f5`, retaining 24,562 and excluding 111.
3. Saved `/private/tmp/causalmed-index-validation.causalmed` (1,188,346 bytes), then restored through the native dialog to `project-VVIfRh`. All four histories returned; the same search returned exactly the new run. Source bindings correctly remain external to the package.
4. Catalog listed four projects with separate history counts. Searching `unmapped` found B alone. Switching to `project-sTLPOd` restored only its two histories; searching for A's new full ID returned zero of two.
5. Returned to original A by its project ID, restored its previous minimum-200 selection, and verified the minimum-202 search still returns the new run independently of the selected report.

No source H5AD was modified. The additional run and restored project remain available as validation artifacts. Kang lacks matching mitochondrial features and remains partial QC. A screenshot records the final native search.

## Checks and limits

Rust app: 21 tests; Rust core: 8; frontend: 10; Python: 24, including configured public files. Frontend/native builds, cargo check and workspace Clippy with warnings denied pass. The upstream `block` dependency still emits its existing future-compatibility notice.

Initial construction and changed-directory reconciliation are linear in the number of archive IDs; short-only searches and exact count queries can scan indexed metadata. A persistent index does not make every filter combination constant time. Immutable artifacts are the authority; direct external edits to existing archive contents are not a supported update workflow. Selection still checks report/manifest correspondence; packaging and exports retain their independent artifact/source verification. Windows/Linux native behavior and million-history production-scale measurements remain unvalidated.

Reproduce with:

```sh
CAUSALMED_TEST_H5AD=/private/tmp/causalmed-pbmc3k.h5ad \
CAUSALMED_TEST_DONOR_H5AD=/private/tmp/causalmed-kang2018.h5ad \
cargo test -p causalmed --lib --offline -- --include-ignored --nocapture

CAUSALMED_TEST_H5AD=/private/tmp/causalmed-pbmc3k.h5ad \
CAUSALMED_TEST_RAW_H5AD=/private/tmp/causalmed-pbmc3k-raw.h5ad \
CAUSALMED_TEST_DONOR_H5AD=/private/tmp/causalmed-kang2018.h5ad \
PYTHONPATH=python .venv/bin/python -m unittest discover -s python/tests
```
