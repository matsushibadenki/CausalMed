# QC selection persistence and retained-cell export validation

Date: 2026-11-05. Environment: macOS native Tauri development app, 1380 × 900, Python 3.12.15 / anndata 0.12.19.

## Real input and saved analysis

Input: the public [Kang 2018 H5AD distributed through Pertpy](https://exampledata.scverse.org/pertpy/kang_2018.h5ad), as referenced by the [official loader](https://pertpy.readthedocs.io/en/stable/_modules/pertpy/data/_datasets.html). Local file: `/private/tmp/causalmed-kang2018.h5ad`. These are software verification data, not IPF validation data.

- Original shape: 24,673 cells × 15,706 genes.
- Original SHA-256: `e6a5adac64dcdeb36eaba27db49b63e0c64bb0ed4a64c6705971506b41c39830`.
- Saved settings: X; feature index; mitochondrial prefix `MT-`; min genes 200; max genes 6,000; min counts 0; no max counts; mitochondrial max 20%; donor column `replicate`.
- Analysis ID: `3f04f29b-3d05-4c55-b62f-f64071836c5b`.
- Result: 24,562 retained candidates, 111 exclusions, 8 donor labels. No mitochondrial features match in this input, so the saved QC status is partial; retention reflects the available count/gene rules.

A compatible QC run writes an immutable private `qc-runs/<analysis-id>/` directory with `cells.npz` and `manifest.json`. The archive records source row indices and IDs, a boolean retention mask, reason bits, per-cell metrics, donor labels/missingness and all QC settings. Source and artifact hashes bind the selection. Invalid count matrices do not produce a selection. Existing pre-selection reports require one QC rerun.

## Native UI workflow verified

1. Open Datasets and run QC with the previously mapped `replicate` donor column.
2. Observe the saved-cell-decision indicator, analysis ID and timestamp.
3. Export the complete QC ZIP through the native save dialog.
4. Export retained rows as a new H5AD through the native save dialog; observe progress and completion, filename and output checksum.
5. Change max genes from 6,000 to 6,001 without running QC. Both export controls become disabled with a message requesting QC first. Restore 6,000; export becomes available again.
6. Quit and restart the app. The same analysis ID, donor mapping, counts and enabled export controls are restored without recomputing QC.

The UI writes saved selections, never transient form edits. Existing output files are rejected. Runtime export receipts are session-local; durable QC decisions and output provenance are independent of that receipt.

## Actual outputs and independent checks

**QC ZIP:** `/private/tmp/causalmed-kang-qc-results.zip`

SHA-256: `4eb83856ba4586cfd08b4c615c2f55d8e50c395205c77a4fcb5da75fe5983bee`.

`manifest.json` and `cells.tsv` were read back independently. TSV has 24,673 rows: 24,562 retained and 111 excluded. The manifest records the correct source checksum, analysis ID and `replicate` mapping. The manifest's selection digest identifies the canonical private NPZ artifact; TSV is the portable, typed-by-schema text representation of those decisions, not that NPZ byte stream.

**Retained H5AD:** `/private/tmp/causalmed-kang-retained.h5ad`

SHA-256: `b98223dd53ee530570657be178b11196f5dd71676e7daf54ec181862f175e06e`.

The output was reopened independently with AnnData. Shape is 24,562 × 15,706. Cell IDs and order exactly match the retained source rows; variable IDs and order match the source. The complete sparse expression matrix equals the corresponding source subset. All 8 original donor labels remain present, and the namespaced donor-ID column agrees with `replicate`. The embedded manifest matches the ZIP manifest, and source-row columns agree with the saved selection. All output retention flags are true.

The original H5AD was hashed again after writing and still equals its original SHA-256. No source data were modified.

## Automated coverage

- Python: all-cell retention/exclusion flags and reasons; duplicate IDs distinguished by source rows; missing donor/undefined mitochondrial values; ZIP contents; retained-cell order; X/layers/raw/embeddings/pairwise preservation; embedded conditions and donor mapping; empty selections; invalid counts; source changes; source-path/hard-link aliases; corrupted selection artifacts; and public Kang round-trip export.
- Rust: actual embedded-worker selection persistence and restart restoration; stale selection rejection; ZIP/H5AD exports; retained original QC state; failed/timed-out/cancelled exports publish no destination and clean staging; and a destination appearing at publication is not overwritten.
- UI translations: all export controls, phase labels and integrity errors have English/Japanese/Simplified Chinese text.

```sh
PYTHONPATH=python \
CAUSALMED_TEST_H5AD=/private/tmp/causalmed-pbmc3k.h5ad \
CAUSALMED_TEST_RAW_H5AD=/private/tmp/causalmed-pbmc3k-raw.h5ad \
CAUSALMED_TEST_DONOR_H5AD=/private/tmp/causalmed-kang2018.h5ad \
.venv/bin/python -m unittest discover -s python/tests

CAUSALMED_TEST_H5AD=/private/tmp/causalmed-pbmc3k.h5ad \
CAUSALMED_TEST_DONOR_H5AD=/private/tmp/causalmed-kang2018.h5ad \
cargo test -p causalmed --lib -- --include-ignored
```

## Limits

Selected arrays are materialized in Python memory for H5AD writing. Very large exports need sufficient memory; no memory quota/chunked export is implemented. Deadlines and cancellation supervise the child process. A crash may leave hidden pending/staging files; they are not completed outputs. All committed run artifacts are retained and exposed in the QC history. Past runs can be reselected and exported; see [history validation](HISTORY_VALIDATION.md). Project-package save/restore is validated in [project validation](PROJECT_VALIDATION.md). Original representation is preserved: selecting a counts layer for QC does not replace output X with that layer. Donor identity/independence and biological validity remain unverified. Windows/Linux export behavior and packaged Python distribution remain pending.
