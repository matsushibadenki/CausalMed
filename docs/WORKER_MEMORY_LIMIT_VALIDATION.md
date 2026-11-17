# Worker process memory limits

Each supervised H5AD worker receives the project memory budget through its private launch environment. Before loading scientific libraries, it sets both soft and hard `RLIMIT_AS`, preserving any stricter inherited limit. Linux uses the configured budget as the address-space ceiling. macOS adds the budget to the interpreter's address space measured with Mach `TASK_VM_INFO` at startup. Darwin rejects an absolute ceiling below existing mappings, including the shared cache and allocator reservations. See [Apple XNU's address-space limit implementation](https://raw.githubusercontent.com/apple-oss-distributions/xnu/main/bsd/kern/kern_resource.c).

The kernel limit covers new mappings requested by HDF5 during whole-value attribute reads and compatibility copies. It applies to address space. Already reserved mappings can still become resident. Rust keeps the live RSS watchdog, and the worker checks the OS-recorded peak RSS before sending a successful result. This covers transient RSS peaks missed by sampling at the publication boundary. On allocation failure the worker reports the existing localized memory error. Rust accepts a staged result only after valid protocol completion and a clean exit.

## Validation

- A 128 MiB budget denied a 1 GiB anonymous mapping before any pages were touched. The worker reported `worker_memory_limit` without relying on the Rust watchdog.
- Two copies of public Kang 2018 used a 288 MiB variable-length UTF-8 attribute. One copied the attribute through native HDF5 attribute I/O. The other attached it to a null-dataspace dataset to force the compatibility object copy. Both reached H5AD writing and returned `worker_memory_limit` with a 240 MiB budget. The Rust supervisor regression verified that no destination or partial file survived, and that the public and augmented sources, saved QC manifest and cell artifact, prior QC ZIP and its receipt were preserved. Both augmented inputs exported successfully at 1,024 MiB with all 18 strings intact.
- Standard public Kang QC retained 24,562 of 24,673 cells. The Rust save/restore regression relocated and reconnected the source by SHA-256, restarted the worker and exported the restored QC selection. AnnData compared the exported cell IDs, every retained expression value and the donor column with the corresponding source rows.
- Linux runtime verification remains pending. The macOS budget covers address-space growth and denies over-budget results at publication. It is separate from an instantaneous physical-memory quota.

The staged bundled macOS runtime also passed the public-H5AD resource-stop, retry, restart and project-isolation test. That run used no development Python override.

## Linux verification

The Linux check uses a Debian 12 ARM64 container running Python 3.12.16 and the repository's locked scientific dependencies. The test input is public Kang 2018 H5AD, SHA-256 `e6a5adac64dcdeb36eaba27db49b63e0c64bb0ed4a64c6705971506b41c39830`. The 384 MiB limit on Linux includes interpreter and library mappings. It leaves room to begin HDF5 writing before the large attributes trigger allocation failure. The 64 MiB test covers early dependency loading, including the Linux loader's mapping error wrapped by NumPy.

The previous macOS Rust app suite completed with 36 passing tests and 11 opt-in tests skipped. Both 10,000-history regressions passed, covering package save, restore and resave, and incremental search index recovery and isolation.
