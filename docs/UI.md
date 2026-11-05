# Workspace design

Audience: medical and life-science researchers. Primary action: inspect provenance and formulate a falsifiable hypothesis. Tone: compact, technical, quiet.

The primary screen uses a left navigation rail, relationship ledger and detachable right inspector. Warm gray paper, navy text and restrained orange actions follow the supplied design. System typography follows the requested Apple conventions. All text is native UI, never baked into images. Data values are monospace; body and controls share a system sans family. `tokens.css` contains the palette, typography and spacing.

The built-in ImageGen tool produced `workspace-concept.png`. Brief: “Complete CausalMed evidence explorer; warm gray / navy / orange; compact left navigation; six synthetic relationship rows; provenance and contradiction inspector; no fabricated biomedical metrics, no marketing hero.”

Implementation adjustments: removed the explanatory subtitle in favor of an information control, corrected generated table content, localized all controls, added actual source checksums, and made the mobile ledger stack into two columns. Native system window chrome is not imitated. The browser surface is explicitly read-only.

Primary component boundaries: App shell; EvidenceExplorer; Inspector; HypothesisDialog; Datasets; Hypotheses; BenchmarkView. Locale and document projections are shared across windows. UI-only navigation/search/form draft state remains transient.
