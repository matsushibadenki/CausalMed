# Selective H5AD research reading

2026-11-10, Apple-silicon macOS, project Python 3.12 environment.

## Change and scope

AnnData's `backed='r'` leaves primary expression backed but still eagerly loads
layers, embeddings, pairwise arrays and unstructured metadata. Research operations
now use a read-only HDF5 projection (`python/causalmed_worker/backed.py`). QC,
cohort filtering and scalar observations share this reader. Rust embeds it before
the other worker modules; isolated execution requires no installed project package.

At this increment, obs/var/raw-var annotation frames loaded at opening. The subsequent [annotation reading increment](ANNOTATION_READ_VALIDATION.md) replaces modern frames with column-on-demand projections. X/raw-X and selected
layers use HDF5 datasets or AnnData's CSR/CSC backed wrappers. Unused payloads
remain on disk. Cohort provenance loads on demand through the existing eight-origin
report limit. Pre-0.7 categorical annotations and dot-separated raw matrices are
supported. Annotation identities and missing values are preserved.

This is not an annotation-memory cap. Legacy annotation frames, modern individual columns, category inventories,
per-cell QC vectors and accessed provenance can still allocate significant memory.
Variable-length strings and HDF5/library allocations remain outside numeric block
budgets. Sampled worker RSS supervision remains necessary; strict OS containment
and bounded annotation reading remain Next. Modern export annotation reconstruction was subsequently removed; see
[annotation export validation](ANNOTATION_EXPORT_VALIDATION.md). Legacy tables
remain eager.

## Automated evidence

- Three new regression tests cover dense/CSR/CSC selected layers, forbidden reads
  of unused uns/obsm/obsp payloads, legacy categories/raw layout, and the actual
  concatenated worker running with Python `-I` outside package imports.
- `PYTHONPATH=python .venv/bin/python -m unittest discover -s python/tests`:
  37 tests, four optional public-file tests skipped.
- `npm run build` and `npm test`: passed, 21 frontend tests.
- `cargo test -p causalmed-core`: passed, 23 tests.
- `cargo test -p causalmed --lib`: 36 passed, ten opt-in tests ignored;
  three relevant public-Kang opt-in tests were also explicitly run and passed.
- `cargo check -p causalmed`: passed after rebuilding the generated worker runtime
  using `python3 scripts/bundle_worker.py --offline` from retained verified caches.
- Public Kang regression: 37 Python tests, two skipped (PBMC files absent);
  donor QC and retained-H5AD export passed with the original checksum unchanged.
  Use `CAUSALMED_TEST_DONOR_H5AD=.worker-cache/test-data/kang2018.h5ad`.
- Rust public-Kang integration checks passed for donor QC/restart and scalar
  observation restore/relink/abstention/project isolation, and mixed-cohort
  export/adoption/restore/recheck/isolation using the embedded script.
- The host default Python lacks anndata; use `.venv/bin/python` or prepend
  `.venv/bin` to PATH when running the documented `python3` command.

## Allocation probe

Run `PYTHONPATH=python .venv/bin/python python/benchmarks/research_read_memory.py`.
Separate fresh processes open the same synthetic fixture containing virtual dense
arrays in layers/obsm/uns. HDF5 fill values avoid allocating the payload at setup.
The probe verifies dimensions and requires the projection's opening growth below
32 MiB and an improvement above 64 MiB. These are fixture regression thresholds,
not universal memory guarantees.

| Reader | Process high-water increase on opening |
| --- | ---: |
| AnnData backed | 135,610,368 bytes (129.3 MiB) |
| Research projection | 1,097,728 bytes (1.0 MiB) |

The fixture is synthetic and provides no biological or predictive evidence.
