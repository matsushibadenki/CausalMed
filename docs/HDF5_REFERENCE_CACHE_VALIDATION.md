# Variable-length attribute cache and reference fields

Native variable-length attribute transfer temporarily caps the source and destination HDF5 metadata cache configurations at 1 MiB when their existing settings permit eviction and use a larger maximum. The previous configurations are restored after the transfer, including on error. HDF5 still allocates its full native attribute payload, and entries that cannot be evicted can exceed the cache target. This change reduces observed memory use without imposing a whole-worker allocation cap.

A fresh-process probe copied a single attribute containing 64 Unicode strings totaling about 32 MiB. The already-native transfer with the default metadata cache increased peak RSS by 46.7 MiB. The scoped cache settings reduced this to 38.8 MiB. Both readbacks produced SHA-256 `6a17af797b7a6f553dd30887222db81c27461323cdf0badc4a0929f64978e6a6`.

Compound datasets whose reference fields can be projected separately now copy in tiles. A top-level reference field can hold object references, region references, fixed arrays or a compound subtree with fixed-width and reference leaves. Separate variable-length fields retain the existing payload budget. Reference fields retain the reference values emitted by HDF5's default compound copy. Their targets are not relocated or expanded. A nested member that combines reference and variable-length leaves retains the compatibility copy.

Regression coverage verifies compound values and reference nullness against native HDF5 copy, forces byte-budget subdivision, and prevents whole-object copy for the supported fixture.

QC on a temporary copy of the public Kang H5AD augmented with compound references and a variable-length attribute retained 12,729 cells. Export preserved selected observation IDs, expression values, attribute values, and compound reference nullness relative to native transfer. The input digest was unchanged. The separate Rust restart integration test `public_selection_archives_and_exports_survive_restart` passed with the public dataset, exercising project archive, restart, restore, and re-export. The Python suite ran 75 tests with two opt-in PBMC cases skipped.
