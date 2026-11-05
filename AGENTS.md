# CausalMed development

- Use Tauri 2 + Rust. Rust owns Document, WorkspaceState, persistence and Renderer. WebViews are projections; never create a second authoritative research store in a window.
- Keep large matrices, textures and frame buffers in Rust/Python/GPU memory. IPC carries commands and bounded metadata. Implement GPU drawing in Rust with wgpu.
- Maintain detachable and dockable panels. Preserve revision-based synchronization across windows.
- English, Japanese and Simplified Chinese are required for every user-facing feature and error.
- Use compact, icon-led controls, with descriptions behind information controls. Follow Apple HIG where practical. Keep semantic line breaks and verify rendered narrow layouts.
- Distinguish observations, intervention evidence and unvalidated hypotheses. Synthetic fixtures must never appear as actual biomedical findings. Preserve source provenance and contradictory records.
- Never claim predictive superiority from synthetic fixtures. Split evaluations by dataset, perturbation and donor; aggregate cells by independent donor. Abstain on insufficient data.
- Break long tasks into completed, verifiable increments. Update `docs/ROADMAP.md` using 🟢 [Done], 🟠 [Next], 🔴 [Later], ⭕️ [Pending]. Pending means validation cannot be performed in the current environment.
- Future production hosting, if needed: XServer VPS Cloud (App VPS, Managed PostgreSQL, NFS). Separate workers communicate through HTTPS/job queues; do not depend on direct worker access to Managed PostgreSQL.

Checks: `npm run build`, `npm test`, `cargo test -p causalmed-core`, `cargo check -p causalmed`, and `PYTHONPATH=python python3 -m unittest discover -s python/tests`.
