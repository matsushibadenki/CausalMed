# Feature index and category dictionary memory

Modern feature indices are decoded on demand. Mitochondrial QC reads string indices in blocks of 8,192 features. The import report counts exact duplicate string IDs using a temporary SQLite set with a 1 MiB page cache. Other index encodings use the existing pandas path.

For large string categorical dictionaries, the worker validates uniqueness in blocks once per open frame, then reads the sorted set of category codes referenced by each row block. Local codes preserve missing values, referenced label order and the ordered flag. Full-column callers can still request the complete dictionary. See the subsequent [feature-space validation](FEATURE_SPACE_VALIDATION.md) for numeric dictionaries and scalar outcome matching. Legacy tables and cell indices still load in memory. Row blocks allow individual variable-length strings to consume more memory, and SQLite uses temporary disk space.

Two fresh-process synthetic HDF5 probes measured additional peak RSS after opening the frame. These measurements cover feature preparation only.

| Probe | Full decode | Block reads |
| --- | ---: | ---: |
| 524,288 string feature IDs, matching and duplicate count | 144.1 MiB | 37.2 MiB |
| 524,288 string categories, 16,384 referenced by feature rows | 111.8 MiB | 39.1 MiB |

Both paths produced identical SHA-256 hashes of the mitochondrial masks and the same duplicate count. Regression coverage rejects whole-index and whole-dictionary reads, checks Unicode and duplicate IDs, preserves missing codes and rejects a duplicate in an unused category.

Public Kang 2018 and its previously exported CD4/CD8 cohort both matched independent AnnData calculations for duplicate gene IDs and mitochondrial prefix matches. Parent QC retained 24,562 of 24,673 cells. Derived QC retained 12,859 of 12,859 cells. Both inputs contained 15,706 features and retained their original SHA-256 digests. The large-dictionary path was covered by synthetic HDF5 because these public samples have smaller dictionaries.

Frontend build and 21 tests, Rust core's 23 tests, the app compile check, and the Python suite passed. Python ran 57 tests with two PBMC-dependent cases skipped.

The supervised Rust integration also passed on Kang, including the 12,859-cell multi-value export, project save and restore, source reconnection and re-export.
