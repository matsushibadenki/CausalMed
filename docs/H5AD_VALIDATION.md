# H5AD / worker validation — 2026-11-05

## Environment and input

macOS native Tauri application, 1380 × 900; Python 3.12.15, anndata 0.12.19. Project `.venv` is installed and auto-detected. Exact installed dependencies are in `python/requirements.lock.txt`.

Public biological input: PBMC3k processed, referenced by [Scanpy's dataset loader](https://github.com/scverse/scanpy/blob/1.11.5/src/scanpy/datasets/_datasets.py) and [dataset documentation](https://scanpy.readthedocs.io/en/stable/generated/scanpy.datasets.pbmc3k_processed.html). Download: [cellxgene example H5AD](https://raw.githubusercontent.com/chanzuckerberg/cellxgene/main/example-dataset/pbmc3k.h5ad). Retrieved 2026-11-05 into `/private/tmp/causalmed-pbmc3k.h5ad`; not bundled in the repository or redistributed with the app. Upstream is a mutable URL; SHA-256 pins the exact tested bytes:

`0db367b991dd95809732b218539ede489bea99113807f62ebd7ccc970025fe38`

Observed 2,638 cells × 1,838 genes; eight `louvain` categories, no missing cluster labels, no duplicate cell/gene IDs. Categories total 2,638. Largest: CD4 T cells, 1,144 (43.4%). Donor metadata is absent and explicitly indicated in the UI. This is an ingestion/visualization test, not an IPF validation dataset or a causal evaluation.

## Verification

- Python reads the original H5AD and emits progress plus one bounded result. Dense and sparse generated HDF5 fixtures separately test missing annotations, duplicate IDs and the category tail. A corrupt H5AD returns a structured error and nonzero exit.
- Rust supervision tests cover success persistence, restoring the report, cancellation, concurrent start rejection, timeout, retry, process failure after result, malformed/oversized output and missing Python. A separate integration test uses the actual embedded worker and public PBMC3k file.
- Native app: dataset import displays the shape, file size, eight-color composition bar, labeled counts and percentages. No error overlay or blank state. Japanese, English and Simplified Chinese rendering were observed at 1380 × 900. Native WebView console capture and additional window sizes were not audited in this run.
- Native app: start a test-only copy with a sparse padded tail, observe live progress, stop it using the cancel control. The normal PBMC report remains displayed. Padding is only for predictable hashing duration; it is not scientific test data and is never saved as the report.
- Native app: use the same test-only input with a 1-second deadline. The UI shows time limit reached at 1.0 seconds and retains the normal PBMC report. Rust kill/wait behavior is additionally covered by lifecycle tests.

- Native app: restart restores the original PBMC report; re-importing the original file succeeds in 0.7 seconds in the warmed environment.

## Commands

```sh
npm run build
npm test
cargo test -p causalmed-core --offline
cargo check -p causalmed --offline
cargo clippy --workspace --all-targets --offline -- -D warnings
PYTHONPATH=python CAUSALMED_TEST_H5AD=/private/tmp/causalmed-pbmc3k.h5ad .venv/bin/python -m unittest discover -s python/tests
CAUSALMED_TEST_H5AD=/private/tmp/causalmed-pbmc3k.h5ad cargo test -p causalmed --lib --offline -- --include-ignored
npm run tauri build -- --debug --bundles app
```

## Limits

The input uses a legacy AnnData encoding, which the tested anndata version reads with warnings. Those warnings are drained from stderr; they do not invalidate the successful import. General compatibility with all H5AD encodings is not established. AnnData backed mode retains X on disk but can load annotations and auxiliary metadata into Python memory. No expression quality check or biological provenance verification is performed. The app currently retains one report, and the development Python environment is not bundled for distribution. Windows/Linux lifecycle and UI validation remain pending.
