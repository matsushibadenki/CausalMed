# QC history validation

Validated on macOS on 2026-11-05 with the native Tauri app, managed Python worker, AnnData 0.12.19 and the public Kang 2018 H5AD already used in [QC validation](QC_VALIDATION.md).

## Real-data scenario

Source: `/private/tmp/causalmed-kang2018.h5ad`, 24,673 cells × 15,706 genes. SHA-256: `e6a5adac64dcdeb36eaba27db49b63e0c64bb0ed4a64c6705971506b41c39830`.

| Run | Minimum genes | Donor column | Retained | Excluded |
| --- | ---: | --- | ---: | ---: |
| `3f04f29b-3d05-4c55-b62f-f64071836c5b` | 200 | `replicate` | 24,562 | 111 |
| `812d0a28-c394-4a8e-b273-856e6f9d799c` | 300 | `replicate` | 24,297 | 376 |

Both runs use X, feature-index gene symbols, `MT-`, maximum genes 6,000, minimum counts 0, no maximum counts and maximum mitochondrial percentage 20. No mitochondrial features match in this file, so both remain explicitly partial QC. Thresholds are validation examples, not universal biological recommendations.

The first run predates history support and was recovered from its saved manifest and matching private source reference. The second was created through the native QC settings. Both appeared in the history list. Baseline/candidate selection displayed the 200 → 300 condition difference and the two retention/exclusion totals. Cell-level comparison returned:

| Original-row decisions | Cells |
| --- | ---: |
| Retained in both | 24,297 |
| Retained → excluded | 265 |
| Excluded → retained | 0 |
| Excluded in both | 111 |

These counts were independently recomputed from the checksummed saved arrays. They sum to all 24,673 original rows. Cell identity is source SHA-256 plus original row order and obs_name; duplicate names are not collapsed. Different source files permit conditions/totals comparison but no cell-transition claims.

## Historical export

The older result was reselected without rerunning QC. Its original conditions, donor mapping, 24,562 / 111 counts and export actions returned. After quitting and reopening the app, the same older analysis remained selected and both histories remained available.

The native save dialog produced `/private/tmp/causalmed-kang-history-baseline.h5ad` (38,358,072 bytes), SHA-256 `b98223dd53ee530570657be178b11196f5dd71676e7daf54ec181862f175e06e`.

Independent AnnData reopening verified shape 24,562 × 15,706, exact original retained-cell order, equality of the complete sparse expression matrix to the original row subset, preserved `replicate` values and all 8 donors, and an embedded manifest identical to the selected historical run. The source checksum remained unchanged. Export receipts are session-only; the saved run and selected pointer persist.

The final native comparison screen was inspected at 1,380 × 900 and at the macOS half-width size of 1,025 × 1,242. The condition table, highlighted threshold change, transition bar and four count cards remained readable. This does not claim minimum-width or all-platform coverage.

## Automated checks

- Python: 24 tests, including exact forward/reverse transitions, different-source rejection, corrupted selection rejection, public-data QC/export and sparse/annotation preservation.
- Rust app: 11 tests with all three opt-in public-file integrations enabled. The history integration creates two conditions, enumerates them, compares exact transitions, restores the older run across restart, rejects reselection while busy and exports that older run. Existing cancellation/timeout/no-overwrite tests also pass.
- Frontend: 8 tests, including identical nonempty history/compare translation keys in English, Japanese and Simplified Chinese.
- Rust core: 8 tests.
- `npm run build`, `cargo check -p causalmed`, workspace Clippy with warnings denied and the macOS debug application build pass.

Reproduce with the public files described in [export validation](EXPORT_VALIDATION.md):

```sh
PYTHONPATH=python \
CAUSALMED_TEST_H5AD=/private/tmp/causalmed-pbmc3k.h5ad \
CAUSALMED_TEST_RAW_H5AD=/private/tmp/causalmed-pbmc3k-raw.h5ad \
CAUSALMED_TEST_DONOR_H5AD=/private/tmp/causalmed-kang2018.h5ad \
.venv/bin/python -m unittest discover -s python/tests

CAUSALMED_TEST_H5AD=/private/tmp/causalmed-pbmc3k.h5ad \
CAUSALMED_TEST_DONOR_H5AD=/private/tmp/causalmed-kang2018.h5ad \
cargo test -p causalmed --lib --offline -- --include-ignored
```

## Limits

History contains compatible completed QC runs; invalid-count reports have no saved cell-selection archive. Legacy archives without a matching current source retain their manifest conditions and summaries but cannot recover a source path that was never stored. Their H5AD export needs that binding; the source-relink control can now restore it after full SHA-256 verification. Invalid archive metadata is omitted from enumeration and is never deleted. Comparison and H5AD export materialize saved arrays in Python memory. History search/pagination, archive deletion and comparison export are future work. Project-package inclusion is validated in [project validation](PROJECT_VALIDATION.md). Comparison target choices and computed transition counts are session-local. Scientific validity and donor independence remain unverified. Windows/Linux native behavior and packaged Python distribution remain pending.
