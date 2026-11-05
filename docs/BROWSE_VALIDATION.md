# Project and QC browsing validation

Validated on macOS on 2026-11-05 with the native Tauri debug app and Python/anndata environment documented in [QC validation](QC_VALIDATION.md). Search results describe saved metadata, not biological findings.

## Implemented behavior

Both lists support case-insensitive, whitespace-separated AND search, combined filters, deterministic sorting with ID tie-breaks, result counts and previous/next paging. Search includes names, IDs, source names/checksums, donor columns and QC options where available. Page sizes are 2, 10, 25 and 50. Query changes reset the page; out-of-range pages clamp to the last valid page. Empty results display page 1 of 1.

Project filters cover active projects, QC presence, local source connection and donor mapping. History filters cover the selected result, donor mapping, QC completeness, exclusions and empty retention. Name, date and retained-count sorts are available; projects also support active-first and history-count sorts. Two comparison choices remain pinned across pages and filters, and can be cleared independently.

Rust performs filtering/sorting/paging and returns at most 50 bounded summary rows. Matrices, histograms and per-cell decisions are absent from list responses. This initial validation preceded the persistent SQLite/FTS5 implementation; current indexed behavior, incremental updates and 10,000-history checks are documented in [search index validation](SEARCH_INDEX_VALIDATION.md). The frontend debounces queries and discards stale responses. Scope keys bind history queries, selections and comparisons to the current Rust project. Switching remounts the history projection and clears searches and pinned choices.

## Public-file native checks

Used the public Kang dataset: 24,673 cells × 15,706 features, SHA-256 `e6a5adac64dcdeb36eaba27db49b63e0c64bb0ed4a64c6705971506b41c39830`. The two projects use separate byte-identical source copies and separate local bindings. See [multi-project validation](MULTI_PROJECT_VALIDATION.md) for their creation and restart checks.

- Project list: three local entries; two-row paging showed ranges 1–2/3 and 3–3/3. The active-project rename field remained correct while its row was off-page. Searching `unmapped` returned only project B. Combining this with the missing-source filter returned zero. Reset restored all entries. Name order was IPF, Kang A, Kang B.
- Project A: three real saved runs, donor column `replicate`. A new QC run with minimum 201 genes retained 24,562 and excluded 111 cells. The minimum-200 and minimum-300 histories retained 24,562 and 24,297 respectively.
- History paging: two rows per page produced two pages. Pinned the minimum-300 candidate on page 1 and the minimum-200 baseline on page 2. Native comparison returned 24,297 retained in both, 265 retained→excluded, zero excluded→retained and 111 excluded in both. Restored the minimum-200 selection.
- Searching A for B's full selected analysis ID returned zero of three, while A's selected result and charts remained intact.
- Searching projects for `unmapped` and switching to B showed only its two histories (`bb8af9f9`, `33b6dfb8`), restored minimum 300 with 24,297 retained, and cleared A's search and comparison choices. Searching B for `replicate` returned zero of two.

After rebuilding and restarting the native app, project A, its three histories, the minimum-200 selected result, donor mapping and source connection were restored. The rendered toolbar and history rows were visually checked.

No source dataset was edited. The new QC run is retained as immutable validation history. The dataset lacks matching mitochondrial features and remains partial QC; counts are workflow checks, not universal biological thresholds.

## Automated checks

- Rust app: 19 tests passed, including public-file worker integration, 503-history pagination/stable ties, 23-project filtering, zero results, page clamping, invalid queries and cross-project scope rejection.
- Rust core: 8 tests passed.
- Frontend: 10 tests passed, including EN/JA/简体中文 label parity.
- Python: 24 tests passed with the configured public files.
- Production frontend build, native debug bundle and workspace Clippy with warnings denied passed. The toolchain separately reports a future-compatibility notice for the upstream `block` crate.

Real-file tests are opt-in and require the local public H5AD paths. Hundreds-of-histories tests use synthetic metadata solely to test list mechanics. Windows/Linux native QA and very large real-world archive performance remain unverified.
