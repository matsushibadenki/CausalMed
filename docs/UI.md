# Workspace design

Audience: medical and life-science researchers. Primary action: inspect provenance and formulate a falsifiable hypothesis. Tone: compact, technical, quiet.

The primary screen uses a left navigation rail, relationship ledger and detachable right inspector. Warm gray paper, navy text and restrained orange actions follow the supplied design. System typography follows the requested Apple conventions. All text is native UI, never baked into images. Data values are monospace; body and controls share a system sans family. `tokens.css` contains the palette, typography and spacing.

The built-in ImageGen tool produced `workspace-concept.png`. Brief: “Complete CausalMed evidence explorer; warm gray / navy / orange; compact left navigation; six synthetic relationship rows; provenance and contradiction inspector; no fabricated biomedical metrics, no marketing hero.”

Implementation adjustments: removed the explanatory subtitle in favor of an information control, corrected generated table content, localized all controls, added actual source checksums, and made the mobile ledger stack into two columns. Native system window chrome is not imitated. The browser surface is explicitly read-only.

Primary component boundaries: App shell; EvidenceExplorer; Inspector; HypothesisDialog; Datasets; Hypotheses; BenchmarkView. Locale and document projections are shared across windows. UI-only navigation/search/form draft state remains transient.


H5AD datasets now use a compact visual summary: cell/gene/file-size/duplicate metrics; a categorical composition bar; color-matched counts and percentages; missing/other categories; a donor-annotation absence indicator. The importer shows actual worker phases, a bounded progress bar, elapsed time and a cancel icon. Descriptions remain behind information icons. The main dataset page hides the evidence-only synthetic badge, so imported H5AD provenance is not conflated with the seeded synthetic evidence.


Expression QC adds a saved-analysis heading, explicit donor-column mapping, collapsible settings and three aggregate histograms. Count/gene axes are labeled log10(1 + value); mitochondrial percentages use a linear axis. Saved thresholds are dashed markers with excluded regions shaded. A retention strip complements the exact retained/excluded counts. Donor rows compare total and retained cells; a missing mitochondrial measurement is shown as unavailable rather than a 0% chart. Draft settings only affect results after a successful QC run.
