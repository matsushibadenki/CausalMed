# Workspace design

Audience: medical and life-science researchers. Primary action: inspect provenance and formulate a falsifiable hypothesis. Tone: compact, technical, quiet.

The primary screen uses a left navigation rail, relationship ledger and detachable right inspector. Warm gray paper, navy text and restrained orange actions follow the supplied design. System typography follows the requested Apple conventions. All text is native UI, never baked into images. Data values are monospace; body and controls share a system sans family. `tokens.css` contains the palette, typography and spacing.

The built-in ImageGen tool produced `workspace-concept.png`. Brief: “Complete CausalMed evidence explorer; warm gray / navy / orange; compact left navigation; six synthetic relationship rows; provenance and contradiction inspector; no fabricated biomedical metrics, no marketing hero.”

Implementation adjustments: removed the explanatory subtitle in favor of an information control, corrected generated table content, localized all controls, added actual source checksums, and made the mobile ledger stack into two columns. Native system window chrome is not imitated. The browser surface is explicitly read-only.

Primary component boundaries: App shell; EvidenceExplorer; Inspector; HypothesisDialog; Datasets; Hypotheses; BenchmarkView. Locale and document projections are shared across windows. UI-only navigation/search/form draft state remains transient.


H5AD datasets now use a compact visual summary: cell/gene/file-size/duplicate metrics; a categorical composition bar; color-matched counts and percentages; missing/other categories; a donor-annotation absence indicator. The importer shows actual worker phases, a bounded progress bar, elapsed time and a cancel icon. Descriptions remain behind information icons. The main dataset page hides the evidence-only synthetic badge, so imported H5AD provenance is not conflated with the seeded synthetic evidence.


Expression QC adds a saved-analysis heading, explicit donor-column mapping, collapsible settings and three aggregate histograms. Count/gene axes are labeled log10(1 + value); mitochondrial percentages use a linear axis. Saved thresholds are dashed markers with excluded regions shaded. A retention strip complements the exact retained/excluded counts. Donor rows compare total and retained cells; a missing mitochondrial measurement is shown as unavailable rather than a 0% chart. Draft settings only affect results after a successful QC run.


Saved QC decisions now have an analysis ID/timestamp and two export actions: complete QC results (ZIP), or retained rows (H5AD). Draft edits disable both actions until QC is rerun. Export-specific status/progress labels and an expandable checksum receipt distinguish writing from analysis. Per-cell vectors remain outside WebViews. Native save dialogs select a new output; existing destinations are not replaced.

QC history lists timestamped runs with retention bars and a selected-run marker. Baseline/candidate controls show changed settings in a comparison table and four cell-decision transition counts with a stacked bar. Selecting a past run restores its charts, donor mapping and export actions. English, Japanese and Simplified Chinese labels are provided.

The top bar has icon-led project save/restore controls with an information tooltip and localized operation feedback. The H5AD panel shows source-linked/missing status, the linked filename and a native source-relink picker. H5AD export is disabled until the source is linked; QC ZIP export remains available. Verification uses the existing progress/cancel/timeout strip.

The project-list icon opens a compact native WebView dialog showing active project, QC history count, selected result, retention totals, donor mapping and source connection. New project creation and active-project naming use two inline forms. Switching returns to the saved dataset state and resets temporary QC edits. All controls are localized in English, Japanese and Simplified Chinese.

## Searchable lists

Project and QC lists have compact search, filter, sort, page-size and reset controls, localized in English, Japanese and Simplified Chinese. Result ranges and filtered totals make empty searches explicit. Search/filter changes return to page one. QC comparison chips preserve baseline/candidate choices across pages and support independent removal; project switches clear them. Flexible control wrapping and horizontally aligned search icons keep the dialog readable. See [browse validation](BROWSE_VALIDATION.md).

## Reversible organization

Both list toolbars have a Trash toggle and icon-led move/restore actions. Trash uses the same bounded search, filters, ordering and pages. Active projects show a disabled move action with a switch-first explanation. Selected-history moves clear comparison targets and saved-result actions; restore requires explicit reselection. Empty trash has its own localized message. Information controls explain reversible storage and inclusion of both QC history buckets in schema-2 packages. The native macOS round trip and final desktop render are recorded in [trash validation](TRASH_VALIDATION.md).

## Multiple selection

Rows and the current eligible page can be selected for organization. The toolbar shows total and off-page selected counts, clear-selection and a bulk move/restore action. Search/filter/sort/page changes preserve selected IDs; project/bucket changes reset them. Active projects are disabled. Receipts show completed/failed counts and expandable per-item errors; only successful IDs are removed. Native screenshots appear in the English README and [bulk validation](BULK_VALIDATION.md).
