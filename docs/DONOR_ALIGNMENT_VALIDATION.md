# Donor alignment and eligibility validation — 2026-11-06

This feature records explicit project-wide donor identities and performs a conservative protocol preflight. It does not run a real-data model or independently establish biological donor identity, causality, licensing rights, or clinical validity.

## Environment and inputs

macOS native debug Tauri/WebView app; Node 22.17.1; Rust; Python 3.12 / anndata 0.12.19 in `.venv`. Browser skill unavailable: native Computer Use exercised the real app and native save/open dialogs, rather than browser IPC mocks.

The public [Kang 2018 H5AD distributed through Pertpy](https://exampledata.scverse.org/pertpy/kang_2018.h5ad) has 24,673 cells × 15,706 genes. Tested SHA-256: `e6a5adac64dcdeb36eaba27db49b63e0c64bb0ed4a64c6705971506b41c39830`. Its `replicate` column produces 8 donor labels and 24,562 retained / 111 exclusion candidates under the tested default thresholds. Missing mitochondrial features produce **partial QC**. `cell_type` and `label` are mixed-valued annotations: this whole-file cohort must remain on hold.

Processed PBMC3k (`0db367b991dd95809732b218539ede489bea99113807f62ebd7ccc970025fe38`) also ran through the Python ingestion regression tests. It supplies no reviewed cross-study donor identity. Public inputs remained unchanged; only disposable copies were altered in integrity tests. These are software verification inputs, not validation of an IPF hypothesis.

## Behavior verified

| Check | Outcome |
| --- | --- |
| Exact census | Fresh supervised Kang QC captured 8 original labels, retained counts, missing counts, donor column, source SHA and immutable QC/manifest reference. No cell arrays crossed IPC |
| Older reports | Native capture from an older report was rejected with a translated request for fresh donor QC; no silent use of truncated labels |
| Draft mapping | Native UI saved `patient_1015` → `kang2018:patient_1015` with evidence explicitly stating cross-study identity remains unverified; confirmation stayed off |
| Invalid confirmation | Mixed-condition/perturbation source could not be marked semantically reviewed, even with IDs/evidence filled. The draft remained saveable |
| Visual status | Original/common IDs, draft/review labels, coverage, independent donor counts, QC ID, observed context and individual hold reasons displayed. Timestamp and document revision identify each check |
| Conservative abstention | Kang showed provenance, partial-QC, mixed annotation, unreviewed identity/semantics and undefined-split issues; cell count never substituted for independent donors |
| Project isolation | Native A held one cohort; switching to B with the same Kang SHA showed zero cohorts. Rust integration edited B's review without changing A |
| Native portable restore | Saved A through the native dialog, restored into a new project from B, and reopened its saved study ID, common donor ID, evidence and unchecked flags |
| Original binding | Restored project initially reported source integrity unverified; full-SHA native reconnection succeeded. Restart preserved the connection and mapping |
| Restart | Restored project reopened with saved review data but **no cached readiness**. A new check regenerated its hold reasons |
| Portable integrity | Rust integration validated restored donor observations/settings against their included QC artifacts; a forged donor label in the project document was rejected |
| Trash | Moving/restoring the referenced QC run made eligibility unavailable/available while keeping its mapping record |
| Historical export | Integration reconnected a relocated source and exported the restored historical retained H5AD successfully |
| Changed original | A disposable bound source was modified; a new eligibility check rejected its integrity. A remembered path alone never passed |
| Bounds and conflicts | Core tests covered stale revisions, duplicate common IDs within one source, missing evidence, census total inconsistency, recapture resets, old-document defaults and SQLite restart/restore |
| Split logic | Explicitly synthetic protocol fixtures covered the ready path and abstention for shared studies, perturbations, contexts, donors, insufficient controls/test donors, stale metadata, partial QC and missing reviews. Equal original labels alone did not merge identities |

Native layout was checked at 1380 × 900 and approximately 1024 × 1240. The mapping form was made denser at the latter width; narrower views stack fields. Temporary evidence images are outside the repository: `/private/tmp/causalmed-donor-alignment-desktop.jpg`, `/private/tmp/causalmed-donor-alignment-narrow.jpg` and `/private/tmp/causalmed-donor-alignment-editor.jpg`. Meaningful content and interactions were visible without a framework overlay; native console logs were not instrumented. The 720-pixel minimum width and Windows/Linux rendering were not checked.

## Verification commands

- `npm run build`, `npm test`: passed (18 frontend tests).
- `cargo test -p causalmed-core --offline`: passed (15 core tests).
- `cargo test -p causalmed --lib --offline`: default suite passed; public-file tests are opt-in. The focused final `cohorts::tests` run with `--include-ignored --nocapture` passed both file-integrity and real-H5AD round-trip tests. Current app coverage comprises 33 tests, including 7 opt-in tests.
- `cargo check -p causalmed --offline` and debug macOS app bundle: passed.
- `PYTHONPATH=python ... .venv/bin/python -m unittest discover -s python/tests`: 29 discovered, 28 passed, 1 skipped (raw PBMC input not supplied); public processed PBMC and Kang paths were supplied. Exact-donor census and single-value annotation-context fixtures also passed.
- `git diff --check`: passed. Input SHA-256 checks matched the pinned values above after testing.

The real integration test is `cohorts::tests::real_donor_alignment_package_restart_isolation_and_abstention`; set `CAUSALMED_PYTHON` to the checkout's `.venv/bin/python` and `CAUSALMED_TEST_DONOR_H5AD` to the public Kang file. It uses temporary project stores and copies of the original, without editing the user's research stores.

## Deliberate limits

Each project has at most 32 cohorts / 1 MiB. There is one cohort per source identity and at most 30 exact donor labels of 200 UTF-8 bytes each. Category tails and long labels cannot become identities. Annotation context is bounded to inspected columns and stores a value only for complete, single-valued columns; absence or mixed values abstain. Sources with multiple conditions/perturbations need lineage-preserving cohort definitions before evaluation.

Preflight targets a cross-study control baseline: reviewed source provenance and semantics, complete QC, no missing retained donors, reviewed mappings, disjoint studies/common donors/held-out perturbation, one comparable context/test perturbation and at least two independent training-control and test donors. The user supplies scientific evidence for identity and comparability; the app does not infer it from similar labels. Excluded cohorts remain saved without participating in the plan.

Integrity checks hash source and decision files in 64 KiB blocks with a shared 15-second verification budget. Exceeding it abstains. Readiness is a snapshot, not continuous file monitoring, and is never persisted. Raw-file resource limits/distribution packaging, mixed-cohort generation, independent scientific review and real-data model execution remain separate work.
