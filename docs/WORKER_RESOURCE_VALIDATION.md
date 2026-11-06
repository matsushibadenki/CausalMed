# Worker resource validation — 2026-11-06

This increment adds execution budgets to the existing supervised Python worker. It changes execution controls and diagnostics, not QC thresholds, donor identities, observations or biological eligibility. Runtime packaging remains unfinished.

## Implemented boundary

- Rust-owned, project-local settings: 64–65,536 MiB resident-memory stop threshold, 1–3,600 cumulative CPU seconds and 1–8 requested compute threads. Defaults are 2,048 MiB / 300 CPU seconds / two threads.
- Settings use a separate revision CAS, bounded validation and atomic local persistence. Active reservations/jobs reject edits. Switching/restarting workers loads that project's settings; malformed files warn and activate defaults. Portable project packages intentionally omit machine-local settings.
- Every supervised operation uses the same guard: import/reanalysis, QC comparison, source relinking, retained/cohort export and scalar-outcome inspection.
- Rust samples the single worker process about every 50 ms and returns only resident bytes, sampled peak, cumulative CPU milliseconds and sample count. Execution-time limits are retained separately from editable next-run limits.
- Memory or CPU breach terminates/reaps the worker with a specific localized reason. Three consecutive sampling failures stop processing. Existing staging/selection cleanup prevents publication or promotion on failure, cancellation or timeout.
- BLAS/OpenMP/NumExpr/Accelerate/BLIS thread requests are installed before Python starts. Unix `RLIMIT_CPU` provides a fallback at budget + one/two seconds for soft/hard limits.

RSS sampling is **not a hard instantaneous allocation cap**. Brief peaks may exceed the threshold before sampling; the guard does not aggregate descendant-process memory or guarantee avoidance of every OS OOM kill. Thread requests are not an OS thread-count limit. Strict memory containment and annotation-memory bounds remain roadmap work. Unsupported monitoring platforms explicitly withhold execution. Linux sampling code exists but is unvalidated here; Windows process accounting is not implemented.

macOS reads `proc_pid_rusage` and converts Mach time using the host timebase before comparing/displaying CPU milliseconds. The native busy-loop test exposed the Apple-silicon timebase distinction and passed after correction. Apple's [XNU Recount documentation](https://github.com/apple-oss-distributions/xnu/blob/main/doc/observability/recount.md) describes the process-accounting interfaces and Mach-time counters; the local macOS SDK/libc and cached `mach2` declarations were also inspected. Linux reads resident kB from `/proc/<pid>/status` and CPU ticks from `/proc/<pid>/stat` with `_SC_CLK_TCK` conversion.

## Real-H5AD checks

Input: [public Kang 2018 H5AD](https://exampledata.scverse.org/pertpy/kang_2018.h5ad), 24,673 × 15,706; SHA-256 `e6a5adac64dcdeb36eaba27db49b63e0c64bb0ed4a64c6705971506b41c39830`. These are software checks, not biomedical validation.

| Check | Observed result |
| --- | --- |
| Normal import/QC | 1,024 MiB / 60 CPU seconds / one requested thread succeeded, retaining 24,562 cells with `replicate` donor mapping |
| Actual usage | Automated import observed about 173 MiB sampled peak and 1.8 CPU seconds. These are environment-specific samples, not a performance guarantee |
| Low memory budget | 64 MiB stopped the real import/reanalysis with `worker_memory_limit`; previous report/selection remained unchanged and only the original committed QC directory remained |
| Blocked export | A real retained-H5AD export under 64 MiB stopped without a destination or published partial output |
| Retry/export/readback | Restoring 1,024 MiB succeeded. Independent anndata readback found 24,562 × 15,706 and eight original donor labels |
| Original preservation | Source SHA-256 was unchanged after failure, retry and export |
| Restart and separation | Project A retained its configured 64 MiB across worker restart while B retained its 2,048 MiB default. Changing A back to 1,024 MiB did not affect B |
| Portable restoration | A saved/validated project package restored QC artifacts and used default local budgets, rather than importing A's machine preferences |
| CPU guard | A continuously busy Python process with a one-second CPU budget stopped with `worker_cpu_limit`, measured CPU ≥1,000 ms and no remaining child |
| Staging cleanup | A controlled memory-allocation process was stopped while an export staging file existed; staging and destination were absent afterwards, prior QC stayed intact, and a retry succeeded |
| Thread requests | A child process asserted all six numerical-library environment requests before returning a valid report |
| Outcome launch | The caller releases its read gate before background write acquisition; short-lived read contention no longer aborts launch. The operation is visible from reservation and supports cancellation before spawn |
| CAS/validation | Invalid bounds, stale settings revisions and active-job edits were rejected atomically |

## Native UI QA

Target flow: native `tauri://localhost` Benchmark → expand Worker resources → save 64 MiB → inspect actual ISG15 outcome → show resource stop and retain the saved report → restore ordinary budgets → reinspection succeeds.

During final native retry QA, an existing outcome-launch race was found: background write acquisition could race the command’s still-held read gate. The command now reserves the operation, explicitly drops its read gate, and waits for short-lived project readers before processing. Operation-specific errors are visible from reservation, pre-spawn cancellation is enabled, and completion notification occurs after releasing the write gate. Prior snapshots were untouched during failed launches. After relaunching the corrected final app, one click started inspection, progress/cancel appeared, and the result committed at revision 60 (sampled peak 146.7 MiB, CPU 2.012 seconds).

The actual macOS Tauri debug application was exercised through Computer Use. Browser skill was unavailable; native Rust IPC and process accounting were tested instead of a browser mock. At 1380 × 900, the app rendered meaningful controls without a framework overlay. The 64 MiB run showed a sampled 73.4 MiB peak, a localized stop reason and unchanged research revision 55. Restoring 2,048 MiB permitted a new inspection at revision 56; observed peak was 147.5 MiB, CPU about 0.7 seconds, and all eight saved donor means remained available. The derived temporary H5AD was still absent and retained its explicit missing-source row, independently of resource status.

Final native checks also confirmed the red warning meter on a new 64 MiB run (64.5 MiB sampled peak), persistence of a distinct 1,536 MiB setting through application quit/relaunch, and readable EN/简体中文 controls at 1024 × 1245. The app was returned to Japanese and default budgets. Screenshots are outside the repository: `/private/tmp/causalmed-worker-memory-stop-final.jpg`, `/private/tmp/causalmed-worker-narrow-en.jpg` and `/private/tmp/causalmed-worker-narrow-zh.jpg`. Native console logs are not instrumented. The 720-pixel minimum viewport, full cross-platform execution and clean-machine runtime installation remain pending.

## Automated checks

- Frontend build and debug macOS app bundle: passed.
- Frontend tests: 21 passed, including EN/JA/简体中文 resource-control/error key parity.
- Rust core: 22 passed.
- Rust app: 32 default tests passed, 10 opt-in real-file tests ignored by default, including existing 10,000-history index/package regressions.
- Explicit `public_h5ad_resource_stop_retry_restore_and_isolation -- --ignored --nocapture`: passed with public Kang, independent output readback and original SHA verification.
- App doctests: passed when run serially. A concurrent Tauri bundle build initially replaced feature-specific dependency artifacts during rustdoc; rerunning `cargo test -p causalmed --offline --doc` after the build resolved the missing-crate error.
- `cargo check -p causalmed --offline`: passed; existing `block v0.1.6` future-compatibility warning remains.
- Python: 33 discovered, 31 passed, 2 processed/raw PBMC tests skipped; public Kang supplied.
- `git diff --check`: passed.

Synthetic allocation and busy-loop processes test enforcement, not biological findings. Live samples reset at the next reservation/restart and are not scientific project history.
