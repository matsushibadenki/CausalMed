# Portable QC trash validation

Validated on macOS on 2026-11-05. This supersedes the live-only package boundary in the original [trash validation](TRASH_VALIDATION.md).

## Package compatibility and state

New `.causalmed` saves use streaming package schema **2**. They include every committed live and trashed QC history, preserving report/manifest/cell-decision bytes, QC options, donor mapping, original analysis ID and creation date. Private source paths remain stripped from portable report wrappers. Source H5AD matrices, local source bindings and search caches are excluded.

The new checksummed `history-state.json` records each analysis ID once, with `trashedAt: null` for normal histories or the original trash timestamp. It contains no local project owner or absolute path. Its internal metadata version is 1, distinct from package schema 2. Validation requires exact agreement between the classification set and archive IDs: duplicates, missing/orphan references, malformed IDs, unsupported versions, incomplete artifact triplets and a selected trash result are rejected before activation.

After extraction and checksum/semantic validation, Rust creates local tombstones with the **new** project key and the preserved timestamp. It then removes the portable classification file from the staged project. Subsequent operations use only the local tombstones, avoiding a second classification store that could become stale. Re-saving generates fresh portable state from those tombstones. Selected live histories remain selected; an unselected project remains unselected; trash never becomes selected automatically.

| Format | Read support | Classification | Capacity |
|---|---|---|---|
| Schema 1 | Retained | All included histories are normal; old packages did not contain trash | Original 1,000-run limit |
| Schema 2 | Current read/write | Normal and trash, including timestamps | Up to 10,000 total runs |

Both formats retain bounded extraction: 8 MiB JSON/index metadata, 4 GiB per artifact, 16 GiB total payload, strict path whitelist, SHA-256 per entry, and staged activation. The schema-2 maximum is 30,003 entries. Actual capacity also depends on artifact sizes and the total byte limit. Packages are not ZIP and are not encrypted or signed. The original package and original project are not changed by restore.

Saving visits one archived report at a time instead of collecting all complete history reports into a vector. Both normal and trash artifacts are verified. Restoration rebuilds project-local search indexes from reports and newly scoped tombstones; index files and their previous project scope are never imported. Existing worker/project gates protect save/restore from concurrent organization and worker starts. EN/JA/简体中文 information controls now describe inclusion of both history buckets.

## Automated checks

The 10,000-history package test creates explicitly synthetic on-disk report/manifest/payload fixtures to test storage mechanics, not biomedical cell decisions. It verifies:

1. Normal 9,900 / trash 100 with a selected normal history.
2. Schema-2 save and restore into a fresh project, including all 10,000 artifacts, exact trash timestamps and selected ID.
3. Fresh indexes and separate bucket counts; mapped `REPLICATE 200` trash search returns 50 entries over two 25-row pages.
4. The original project key is rejected at the restored worker. Restoring one trash item changes only the new project to 9,901 / 99; the original remains 9,900 / 100.
5. Explicitly select that restored item, save again and restore a second generation. Counts remain 9,901 / 99 and the selection is preserved. The portable state file is not left as active state.
6. Reject a selected trash reference and duplicated classification entries. Separate tests reject orphan state, unsupported state versions, corrupted packages and incomplete/unsafe entries, while preserving the active pointer.
7. Read a constructed schema-1 package containing a history as normal, with no fabricated trash entries.

One diagnostic run produced a 14,075,445-byte package and completed two save/restore generations in approximately 45 seconds, excluding fixture setup/cleanup. This is an I/O test sample, not a throughput guarantee for real large NPZ artifacts.

Public-file integration additionally creates mapped minimum-200 and minimum-300 Kang runs in A and an unmapped minimum-300 run in B, all from the same source checksum. It saves A with one normal and one trashed run and no selection, restores those buckets and the unselected state, verifies owner/timestamp rebinding, restores a trash item without changing A/B, relinks the original, selects/exports it, then saves/restores again with both histories normal and the selected ID preserved.

Final coverage: Rust app **27**, Rust core **8**, frontend **11**, Python **24** = **70 tests**. Public-file tests were enabled with the documented local Kang/PBMC paths. Production frontend/native debug builds, cargo check and Clippy with warnings denied pass. The existing upstream `block` future-compatibility notice remains.

## Native public-data round trip

Source: Kang public H5AD, **24,673 × 15,706**, SHA-256 `e6a5adac64dcdeb36eaba27db49b63e0c64bb0ed4a64c6705971506b41c39830`.

- Original project: `project-Hoeqja`. Kept selected normal run `3f04f29b-3d05-4c55-b62f-f64071836c5b` (minimum 200, donor `replicate`, 24,562 retained) and moved `812d0a28-c394-4a8e-b273-856e6f9d799c` (minimum 300, donor `replicate`, 24,297 retained) into trash. Counts were normal 3 / trash 1.
- Saved `/private/tmp/causalmed-portable-trash.causalmed`: **1,188,763 bytes**, schema 2, 15 entries. Inspected the checksummed state: four IDs, exactly one trash timestamp, no owner key or path binding.
- Restored via native dialogs to `project-n4fVw4`. The normal list showed three histories with `3f04f29b` still selected; trash showed only `812d0a28` with its original count/donor summary. The local tombstone had the new owner key and exactly the original timestamp `1793862693140`. Source connection was correctly absent until explicit SHA-256 relinking.
- Restored `812d0a28` from the recipient's trash, explicitly selected it, and relinked `/private/tmp/causalmed-relocated/kang-original.h5ad`. Exported `/private/tmp/portable-trash-retained.h5ad` through the native save dialog.
- Independent Python/anndata checks confirmed output shape **24,297 × 15,706**, exact sparse matrix equality to the source retained rows, exact cell IDs and donor-column values. Output SHA-256: `13dea2d85b31d7f6d9a5b6fec6e507c8d797979df5045043c43fd020e70e94ec`. Source SHA-256 was unchanged.
- Before cleanup, queried both native index databases: original A remained 3 normal / 1 trash with `3f04f29b` selected; recipient was 4 normal / 0 trash with `812d0a28` selected. Switched back to original A and confirmed its trash still contained the run, proving recipient operations had not altered it.
- Restored the validation trash item in original A and returned to its normal four-history list with the previous selection preserved. The recipient project, package and output H5AD remain available for review.

Kang has no matching mitochondrial features, so these remain partial QC results. The checks validate reproducible storage and export behavior, not biological thresholds or causal claims. Windows/Linux native QA, signing/encryption, progress during package copying, and larger byte-volume performance remain separate roadmap work.

Use the public-file commands in [trash validation](TRASH_VALIDATION.md); the package load test is included in the Rust app suite without external data.
