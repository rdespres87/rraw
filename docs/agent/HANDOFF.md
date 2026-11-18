# HANDOFF.md — Next Action & Session Continuity

## Last Completed Milestone

- **Phase 2 — Task 2.2 (`rraw-gpu`) Committed (`8e922ad`)**:
  - Implemented byte-budgeted `LruVramCache` with clean LRU photo eviction order preventing VRAM exhaustion.
  - Implemented `expand_roi_with_halo` supporting 120 Hz interactive scrubbing with 32px boundary protection.
  - Implemented Stage 5 WGSL compute shader with AP1 Gamut Guard and $N_{\text{octaves}} - L$ Laplacian pyramid local contrast, verifying scale-invariance RMSE $< 0.5\%$ between $1:1$ ($L=0$) and $1:4$ ($L=2$).
  - Implemented Stage 7 WGSL compute shader with AgX DRT and parallel workgroup reduction for 256-bin luminance histogram and clipping detection.
  - 56 unit and integration tests passing cleanly across the workspace with zero clippy warnings.

## Exact Next Logical Action

1. **Next Development Task (Task 2.3 — `rraw-macos-edr` & `rraw-app`)**:
   - Follow Security Gate 1: Draft and present architectural plan for safe `objc2` `DisplayHeadroomProvider` implementation (`NSScreen.maximumExtendedDynamicRangeColorComponentValue`, `CAMetalLayer` `extendedLinearDisplayP3`).
   - Implement permanently fixed `Rgba16Float` surface format across display transitions in `rraw-app`.
   - Implement dual-pass compositor: SDR UI (`egui`) + EDR viewport (`wgpu`).
   - TDD Gate: Mock `DisplayHeadroomProvider` transition $H_{\text{active}} = 4.0 \to 1.0$ keeps `Rgba16Float` fixed, clamps viewport output to $1.0$, and preserves `egui` UI color invariance.
2. **Subsequent Phase 3 Tasks**:
   - Task 3.1 (`rraw-optics` & `rraw-gpu`): 4-tier optical resolver, $256 \times 256$ UV LUTs, fused Catmull-Rom `WARP` shader.
   - Task 3.2 (`rraw-optics` & `rraw-gpu`): Regularized Richardson-Lucy Capture Sharpening.
