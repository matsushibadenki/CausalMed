# Streamed H5AD export validation

Validated on macOS on 2026-11-05 with Python 3.12, anndata 0.12.19, h5py, NumPy and SciPy. This extends the original [export validation](EXPORT_VALIDATION.md).

## Priority and resulting behavior

The previous retained-cell export called `AnnData.to_memory()` for the complete retained subset before writing it. That can load expression, layers, raw data and cell-pair matrices together. Removing this bottleneck improves the existing real-data workflow without depending on unvalidated research models, a cloud service or a new dataset registry.

`stream_export.py` now reads/writes numeric matrices directly through HDF5. Dense arrays use row/column tiles, including matrices wider than one budget-sized row. CSR/CSC keep their sparse orientation and stream bounded nonzero segments, including an individual row/column larger than the budget. `obsp` is filtered on both axes; X, layers, raw.X and embeddings follow retained row order. Variables, variable-aligned data and unrelated metadata remain attached. Original category levels, nullable annotations, duplicate cell IDs and donor values are preserved.

Default numeric input-block budget: **8 MiB**. Tests can lower it to 1 KiB; receipts include bounded diagnostic metadata (`blockBudgetBytes`, `maxReadBlockBytes`, `readBlocks`). No matrices are transferred to a WebView. The supervised Rust worker embeds the streaming helper; it needs no separate installed module or new runtime dependency.

The block budget is **not a cap on total RSS**. QC decision/cell-ID vectors and annotation tables remain O(cell count), and Python imports, metadata, HDF5 caches/compression and native metadata copying use additional memory. Variable-aligned and unstructured metadata use HDF5 object copying; this work does not establish a global memory limit for arbitrary large metadata. Resource limits and worker distribution remain subsequent work.

## Format and failure boundaries

The copier follows the [AnnData on-disk format](https://anndata.readthedocs.io/en/stable/fileformat-prose.html): numeric datasets, CSR/CSC groups, dataframes, categoricals and nullable annotations. Tested legacy support includes pre-0.7 compound annotation/embedding records, `h5sparse_format`/`h5sparse_shape`, and `raw.X`/`raw.var` layouts. Legacy annotation decoding uses AnnData's compatibility reader; output is normalized to modern encodings without changing selected values.

Unknown row-aligned group encodings and higher-dimensional embedding rows larger than the block budget fail with a localized EN/JA/简体中文 error. There is no fallback to materializing the full matrix. This is not a promise to support every possible extension encoding.

Source and decision checksums remain checked before export; source size/mtime and SHA-256 are checked again afterward. The output is checked for shape, ordered cell IDs and the embedded manifest. The existing exclusive Rust staging/publication boundary is unchanged: cancellation, timeout and failures cannot publish the temporary file or overwrite an existing destination. Source files are opened read-only. Progress advances through matrix writes without regressing.

## Automated checks

Four new Python tests cover:

- Dense/CSR/CSC X, dense and CSC layers, raw with different feature count, dense/dataframe embeddings, dense/CSR/CSC cell-pair matrices, variable-aligned data, categorical and nullable annotations, and unrelated metadata. Each numeric read is at most **1,024 bytes** under a 1 KiB test budget; every selected matrix is compared with the source reference.
- A CSR row with 12,000 entries is split across many reads. Patching `AnnData.to_memory` and `anndata.read_h5ad` to fail proves the export path does not use either eager/backed reader. Existing selection/provenance tests still pass.
- Legacy dotted raw layout is filtered to retained rows rather than copied with its original cell count.
- Unknown row encodings fail closed while the source checksum stays unchanged.

All **31 Rust app tests** passed with public-file integration enabled, including supervised export, history/portable restoration, cancellation/timeouts, source isolation and large-history tests. Rust core: **8**; frontend: **16**; Python: **28**, including all configured public PBMC/Kang inputs. Total: **83 tests**. Frontend/native builds, cargo check, Clippy with warnings denied and diff checks pass. The upstream `block` crate's existing future-compatibility notice remains.

## Memory probe

Run in a fresh process:

```sh
PYTHONPATH=python .venv/bin/python python/benchmarks/stream_export_memory.py
```

The explicitly synthetic HDF5 fill-value matrix is 24,000 × 4,096 float32 (**393,216,000 bytes** dense equivalent); retaining alternate rows produces **196,608,000 bytes**. It avoids allocating the complete matrix during fixture setup. Under a **2,097,152-byte** read budget, the observed maximum explicit block was **2,097,152 bytes**, with **95** reads and process high-water growth **28,475,392 bytes**. Sample values and dimensions were checked. The script has a conservative growth regression threshold, not a production RSS guarantee or a biological evaluation.

## Native public-data check

The previous temporary public inputs had been cleaned; they were re-downloaded from the same documented URLs and verified against their recorded SHA-256. The native source fixture alias was restored with identical public bytes. No source data was edited.

Source: public Kang, 24,673 × 15,706, SHA-256 `e6a5adac64dcdeb36eaba27db49b63e0c64bb0ed4a64c6705971506b41c39830`.

- Active original A and saved minimum-200 / `replicate` result `3f04f29b-3d05-4c55-b62f-f64071836c5b` remained selected.
- Started an export through the native dialog, cancelled during the initial checksum stage and observed “Export cancelled / No new export was published.” Retrying succeeded. A writing-stage progress value of 49% was observed; the retry completed before a later cancel attempt. No claim is made that this native run cancelled mid-write; supervised cancellation/timeout staging is covered by Rust tests.
- A final export to `/private/tmp/causalmed-stream-retained.h5ad` completed. Independent Python checks confirmed **24,562 × 15,706**, exact sparse matrix equality to source retained rows, exact ordered cell IDs, donor-column values, embeddings and embedded QC manifest.
- Output: **41,158,392 bytes**, SHA-256 `b8d57f1eb9fd31294a86737f5c8b21f83c8cc46f155d86d447830a593568797c`. The source SHA-256 was unchanged. This output's HDF5 encoding/compression layout differs from earlier writer output; numerical and provenance equality are the relevant checks.

Kang lacks matching mitochondrial features and remains partial QC. The checks establish file/workflow behavior, not biological validity. Cross-platform native QA, hard process-resource caps and support for additional encodings remain unfinished.
