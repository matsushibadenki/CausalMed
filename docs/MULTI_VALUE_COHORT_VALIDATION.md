# Multi-value cohort validation — 2026-11-07

This increment expands frozen cohort definitions to sets of exact condition and perturbation values. It verifies extraction and lineage software; it makes no claim about biological comparability or predictive performance.

## Selection contract

The two distinct annotation roles each accept 1–20 explicitly selected observed values from complete, untruncated category inventories. Values use OR within a role; the two role masks use AND with the immutable parent-QC retained mask. Missing annotations do not match. Rust canonicalizes sets, rejects duplicates/unknown/empty selections and stores the frozen source SHA, QC references, provenance metadata and donor mappings. Search affects visible choices, not selection. Select-all and clear affect all values in that role. Displayed category counts are source counts before QC, not joint retained counts.

Multi-value arrays are sorted, unique, bounded and anchored by the legacy scalar field. Singleton definitions omit the new fields, preserving their serialized payload bytes and SHA. Portable restoration validates canonical arrays and hashes. Changing a filter creates another definition; exported definitions remain immutable.

## Public H5AD results

Input: [Kang 2018 distributed by Pertpy](https://exampledata.scverse.org/pertpy/kang_2018.h5ad), 24,673 cells × 15,706 genes. SHA-256: `e6a5adac64dcdeb36eaba27db49b63e0c64bb0ed4a64c6705971506b41c39830`. Test roles: donor `replicate`, condition `cell_type`, perturbation `label`. The parent QC retained 24,562 cells, excluded 111 and explicitly reported partial QC because mitochondrial features were unavailable.

| Frozen selection, intersected with parent QC | Exported cells | Genes |
| --- | ---: | ---: |
| CD4 T cells; ctrl (legacy singleton regression) | 5,560 | 15,706 |
| CD4 T cells; stim (legacy singleton regression) | 5,678 | 15,706 |
| CD4 T cells OR CD8 T cells; ctrl OR stim | 12,859 | 15,706 |

An independent anndata calculation verified exact row IDs, source-row indices, unchanged expression values, raw donor labels, common donor IDs and reviewed/draft flags for every output. Original input SHA remained unchanged. Dense, CSR and CSC synthetic fixtures additionally exercise a matching zero-library row excluded by QC, an unselected third condition/perturbation and missing annotations. Synthetic fixtures are software tests only.

## Persistence and eligibility

The Rust public-data integration test saves all three definitions and receipts, validates a portable package, relocates and reconnects parent/derived files by SHA, restarts the worker and re-exports the multi-value definition. A separate project cannot adopt these outputs using the first project's definition or donor reviews. Invalid selections leave the document revision unchanged.

Adoption requires a project-local recorded output SHA, embedded frozen definition, fresh committed donor QC and a complete child annotation inventory contained in the frozen sets. Missing child annotations are rejected. Donor mappings retain their original review states; source/semantic review and partition reset. Mixed observed condition and perturbation values remain `condition_unresolved` and `perturbation_unresolved`; eligibility stays blocked. Extraction never turns a mixed cohort into an automatically eligible evaluation input.

## Native verification

Computer Use exercised the actual Tauri/WebView app with bundled Python 3.12.16 and anndata 0.12.19. A saved parent QC was selected, the relocated original was reconnected by SHA, two conditions and both perturbations were checked, search was changed/cleared without losing hidden checks, and the new definition was saved and exported through the native save dialog. The export receipt showed 12,859 matching/retained cells. Independent anndata readback confirmed its exact matrix/rows and embedded sets. Its output SHA was `09be9d7fd11506f089e7bee87eb22b6a6dbe8c80381204fcaea8e6d3e6f40060` (definition IDs/timestamps make output hashes specific to each run).

English, Japanese and Simplified Chinese controls and help were inspected. A 1,024-pixel native window showed the selectors, condition chips, warning and export receipt without overlap. The first native run revealed oversized checkbox rows; the final stylesheet fixes checkbox dimensions and makes the logical grouping explicit with parentheses. Restarting the final bundle preserved the saved definition and export receipt, reset transient unchecked inputs, and confirmed the corrected compact layout at 1,024 pixels. Full 720-pixel native capture and keyboard regression remain separate roadmap work.

## Checks

- `npm run build` and `npm test`: passed, 21 frontend tests.
- `cargo test -p causalmed-core --offline`: passed, 23 tests including legacy byte compatibility and malformed canonical sets.
- `cargo check -p causalmed --offline`: passed.
- `cargo test -p causalmed --offline --lib`: 36 passed, 10 opt-in integrations ignored in the general run, including the 10,000-history regression.
- Public mixed-cohort integration: passed with all three public outputs, adoption, portable restore, re-link and re-export.
- Bundled Python test suite with `CAUSALMED_TEST_DONOR_H5AD`: 34 tests, 32 passed, two unrelated PBMC fixtures skipped because their source paths were not supplied.

The existing annotation inventory limit (20 categories per role) remains deliberate. Annotation objects are still memory-resident; strict OS memory containment, broader adapters and multivariate evaluation remain unfinished roadmap work.
