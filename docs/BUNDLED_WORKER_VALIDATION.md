# Bundled worker validation — 2026-11-06

The current macOS app contains its own Python interpreter, standard library and numerical/H5AD dependencies. This validation concerns software packaging and preservation of research artifacts, not biological validity or model performance.

## Artifact and reproducible preparation

Target: **aarch64-apple-darwin / Apple-silicon macOS 14+**. Python Standalone CPython **3.12.16**, release **20261026**, archive SHA-256 `353d5f1b4fd2715e7053f12f068435d5bbad16b91e057006b5d01e0925709a7e`. The archive is pinned from [Astral Python Build Standalone](https://github.com/astral-sh/python-build-standalone/releases/tag/20261026). Its [distribution documentation](https://github.com/astral-sh/python-build-standalone/blob/main/docs/distributions.rst) describes install-only archives. The selected wheel versions and hashes were independently matched to PyPI metadata.

`python/runtime/aarch64-apple-darwin.json` and `wheels-aarch64-apple-darwin.json` pin the archive and all 21 dependency wheels, including anndata 0.12.19. `scripts/bundle_worker.py` verifies downloads/cache hashes, installs verified wheels with `--require-hashes --no-deps --no-index`, checks dependencies/imports, retains licenses/notices, removes temporary-path command entrypoints and bytecode caches, then publishes a complete inventory and bounded runtime manifest. Recipe and lock hashes participate in reuse verification. Generated runtime/cache directories are excluded from Git; the Tauri build hook reconstructs them from source pins.

The final debug `.app` is about 277 MiB. Its Rust binary embeds the worker source; `Contents/Resources/python-worker` contains the interpreter/dependencies, `runtime.json`, `files.json` and `THIRD_PARTY_NOTICES.md`. Preparation can reuse a verified offline cache. End-user execution does not run pip, download packages or access the development environment.

## Runtime resolution and isolation

The resolver recognizes an app's `Contents/MacOS` executable layout and selects only its sibling resource runtime, even in a debug `.app`. It validates manifest/target/version/recipe/dependency pins and the interpreter checksum, caching the choice for that process. Missing or invalid bundled resources remain explicit errors; a valid developer override cannot bypass them. An unbundled debug executable may use an explicit override, the prepared runtime or an optional `.venv`. There is no automatic system-Python fallback.

Production worker commands use `-I -B` and remove Python home/path/virtualenv launcher overrides. Runtime status and errors are localized in EN/JA/简体中文. The shared progress, deadlines, cancellation, CPU/RSS budgets, thread requests, immutable selections and staged non-overwrite exports remain in force.

Full artifact inventory is checked by preparation/audit, rather than rehashed on every UI poll. Runtime validation is not an authenticity signature or filesystem sandbox. Signing/notarization and fresh-machine installation remain pending.

## Relocated artifact audit

The complete app was copied to a new path containing spaces outside the repository. `scripts/audit_bundled_worker.py` ran its interpreter with a system-only PATH and invalid Python/virtualenv override variables. CPython reported isolated mode and a prefix inside the moved `.app`; anndata, NumPy, h5py, pandas and SciPy all loaded there. All sys.path entries belonged to the runtime.

The audit verified the full file inventory and **223 Mach-O files**, checking actual load dependencies for system or bundled loader-relative paths and finding **no external Homebrew dependency**. Dylib install IDs were distinguished from load dependencies; some upstream IDs are build-path metadata, not required external libraries.

An initial audit caught bytecode written by test subprocesses. Preparation now removes caches, production always uses `-B`, and validation subprocesses disable bytecode writes. The final inventory passed again after real-data processing. Runtime-manifest tests also caught and corrected a minimum-macOS key spelling mismatch before acceptance.

## Actual H5AD and preservation checks

Public input: [Kang 2018 H5AD](https://exampledata.scverse.org/pertpy/kang_2018.h5ad), **24,673 × 15,706**, SHA-256 `e6a5adac64dcdeb36eaba27db49b63e0c64bb0ed4a64c6705971506b41c39830`.

`scripts/test_bundled_isolation.py` temporarily hid both `.venv` and the prepared source runtime, used a system-only PATH and invalid Python home/path variables, and executed the compiled Rust real-file integration test against the **relocated app's interpreter**. A `finally` block restored both development directories. The test passed:

| Check | Result |
| --- | --- |
| Import and QC | Real H5AD loaded through the supervised bundled Python; `replicate` donor mapping retained 24,562 cells and excluded 111 |
| Resource usage | Normal bundled import observed about 192 MiB peak and 7.8 CPU seconds in one run; environment-specific samples, not a speed claim |
| Resource stop | 64 MiB stopped reanalysis and retained-H5AD export with the specific memory error |
| Prior result preservation | Failed reanalysis kept the exact previous report/selection; no new committed QC directory appeared |
| Export publication | Resource failure left no output destination and cleaned staging; retry under ordinary budgets succeeded |
| Independent readback | The bundled interpreter reopened the output: 24,562 × 15,706, eight original donor labels |
| Original integrity | Input SHA remained unchanged |
| Restart and project separation | Local budgets and prior report survived worker restart; a second project kept its own default budgets |
| Portable restoration | Research/QC package restored correctly and used machine-local default budgets |

The compiled worker lifecycle tests also passed with the bundled interpreter: CPU-budget stop, timeout, cancellation, pre-spawn cancellation, malformed output, non-overwrite publication and thread requests. Resolver tests cover absent/corrupt resources, relocation and refusal to fall back to development Python. Preparation tests cover corrupt offline artifacts, external symlinks and changed file inventories.

## Native app and README screenshots

Computer Use exercised the moved actual Tauri app (`tauri://localhost`), not a browser mock. A separate **Bundled Python · public Kang QA** project kept existing research projects isolated. The runtime indicator showed **Python 3.12.16 · 同梱版**.

The native workflow imported Kang, ran QC with `replicate`, exported `/private/tmp/causalmed-bundled-retained.h5ad`, and displayed the output SHA. Independent bundled-anndata readback confirmed 24,562 × 15,706 and eight donors; the original SHA was unchanged. The native output SHA was `adea789135ddc4589bcf756cf07d7ed57ea6e5de1288c76a518757f73df6edae`.

A native 64 MiB reanalysis stopped at a sampled 64.6 MiB and left selected QC `5af0d928-d628-4979-b332-5fba61f261a0` intact. Restoring 2,048 MiB completed another QC (`7af6a49e…`), while both prior histories remained visible. Partial mitochondrial QC remained labeled as partial; no review or biological eligibility was fabricated.

README images were captured from the 1380 × 900 Japanese app and stored under `docs/images/bundled-worker-{runtime,qc,export}.jpg`. They show the bundled runtime/resource stop, QC/donor graphs and retained-H5AD receipt. Meaningful content and completed interactions rendered without a framework overlay. Native console logs were not instrumented; this increment did not repeat the 720/1024-pixel locale/layout matrix.

## Checks and limits

- `npm run build` and debug `.app` bundle: passed, including automatic runtime verification/preparation.
- `npm test`: 21 passed, including runtime label/error locale parity.
- `cargo test -p causalmed-core --offline`: 22 passed.
- `cargo test -p causalmed --lib --offline` with the moved bundled interpreter: **36 passed / 10 opt-in tests ignored**; includes 10,000-history index/package regressions.
- Explicit isolated real-H5AD resource/import/export integration: passed with development directories absent.
- `cargo check -p causalmed --offline`: passed; existing `block v0.1.6` future-compatibility warning remains.
- Python suite under Python 3.12.16 with pinned packages: 33 discovered, 31 passed, 2 separate PBMC inputs not supplied.
- Preparation tests: 3 passed; source runtime verification and relocated complete/native inventory audit passed.
- `git diff --check`: passed.

Current scope is Apple-silicon macOS 14+. Intel macOS and Windows/Linux artifacts, signed/notarized distribution, a truly fresh machine and strict instantaneous memory containment remain unfinished or pending validation. The absent-development-runtime/relocation test was performed on the development Mac and is not a clean-machine certification. Resource memory thresholds are sampled watchdogs, not allocator limits.
