# Project package and source relocation validation

Validated on macOS on 2026-11-05 with the native Tauri app, Python 3.12.15 and AnnData 0.12.19. This extends [QC history validation](HISTORY_VALIDATION.md).

## Saved package

The native save control created `/private/tmp/causalmed-kang-project.causalmed`:

- Schema: 1, CausalMed streaming archive.
- Size: 599,113 bytes.
- SHA-256: `715c704abf886d46853d559fb30e22edbaf4353b70b0e904b37c621db81a302b`.
- Eight entries: research document, selected-result metadata, and report/manifest/compressed decisions for each of two saved QC runs.
- The research document includes the existing six synthetic evidence relationships, one hypothesis and the synthetic benchmark. These remain labeled synthetic/unvalidated.
- QC histories retain the public Kang 2018 source identity, exact settings, `replicate` donor mapping, all per-cell decisions, metrics, reason flags and original-row identities.
- Portable metadata has `sourcePath: null`; no source H5AD or local source binding is included.

Every indexed entry length and SHA-256 was independently checked by reading the package bytes. Both compressed decision archives match their original manifests. The selected past analysis is `3f04f29b-3d05-4c55-b62f-f64071836c5b`: minimum genes 200, 24,562 retained and 111 excluded. The other run, `812d0a28-c394-4a8e-b273-856e6f9d799c`, uses minimum genes 300: 24,297 retained and 376 excluded. Both retain `replicate` as their donor column.

## Native restore and source verification

The package was restored through the native file picker into a fresh local project generation. The two histories, selected older result, donor mapping, plots, hypothesis and research records returned. The research revision advanced from 14 to 15. The previous local research database and QC archives remained intact. After quitting and reopening the final native build, the restored generation, both histories, older selected run, `replicate` mapping and verified relocated-source binding were still present, with H5AD export enabled.

Immediately after restore, the source was explicitly unlinked, QC reanalysis and H5AD export were disabled, and QC ZIP export remained available. Choosing the different public PBMC3k H5AD failed with a localized SHA-256 mismatch; it did not connect that file or replace the selected report.

A byte-identical public Kang source was placed in a different directory and filename, `/private/tmp/causalmed-relocated/kang-original.h5ad`. The native relink action hashed it and accepted source SHA-256 `e6a5adac64dcdeb36eaba27db49b63e0c64bb0ed4a64c6705971506b41c39830`. Selecting both histories confirmed that one verified binding enables both without rerunning QC. The automated public-file integration additionally performs an actual rename of its owned source file, so the previous source path is absent, then restores/relinks/exports successfully. A second mismatched attempt after a valid link leaves that valid link unchanged.

A deliberately damaged copy of the package (one payload byte changed) was also selected in the native restore dialog. Restore failed with a localized integrity error; the active generation, revision, selected history and source binding remained unchanged. The damaged file is a test artifact, not a valid saved project.

## Export from restored history

The older run was reselected and exported through the native H5AD save dialog, without recomputing QC:

- Output: `/private/tmp/causalmed-relocated/causalmed-restored-history.h5ad`.
- Shape: 24,562 cells × 15,706 genes.
- SHA-256: `b98223dd53ee530570657be178b11196f5dd71676e7daf54ec181862f175e06e`.
- The complete sparse expression matrix equals the original retained-row subset.
- Cell IDs/order and all variable IDs/order match exactly.
- All 8 original donor labels remain and agree row-for-row with source `replicate` values.
- The embedded manifest equals the restored older run's manifest, including its QC options and donor mapping.
- Both the relocated source and the original public source retain the original SHA-256.

## Automated coverage and commands

All 55 tests pass: 14 Rust application tests (including all public-file integrations), 8 Rust core tests, 24 Python tests and 9 frontend tests. Coverage includes package round trips, byte corruption, trailing data, unsupported versions, duplicate/unsafe names, size bounds, document-reference validation, monotonic restore revisions, relocated source mismatch/acceptance, persistent bindings, and restored historical H5AD export. Existing worker cancellation/timeout/no-overwrite and expression-preservation tests continue to pass.

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

The system Homebrew Node executable had an unresolved `libsimdjson.34.dylib` dependency during this run. Frontend checks and the macOS application build used the already-installed Node 22.17.1 by prepending its bin directory to PATH; no system installation was changed.

## Current limits

Source H5AD matrices are external and must be reconnected after each portable restoration. The package format is versioned but not signed or encrypted; checksum equality proves matching bytes, not scientific validity. Saving/restoring has a busy indicator and bounded input sizes, but no byte-level progress/cancellation UI. It uses temporary disk space. Prior local generations are retained and can now be selected through the local project catalog; cleanup remains future work. See [multi-project validation](MULTI_PROJECT_VALIDATION.md). H5AD export still materializes selected data in Python memory. Future schema migrations, minimum-width native regression coverage, Windows/Linux behavior and packaged Python distribution remain staged work.
