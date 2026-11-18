# HANDOFF.md — Next Action & Session Continuity

## Last Completed Milestone

- **Phase 2 — Task 2.1 (`rraw-gpu`) Committed (`4eb20a4`)**:
  - Implemented `GpuContext::new_headless()` with robust adapter fallback and queue acquisition on Apple Silicon Metal.
  - Implemented Stage 1 WGSL compute shader with signed `R32Float` normalization (preserving sub-black read noise without DC bias), outlier hot-pixel defect repair, and saturation `clip_mask`.
  - Implemented Stage 2 WGSL compute shader with unclamped pre-demosaic white balance ($\text{sat}_c = w_c$), Hamilton-Adams directional edge-adaptive Bayer demosaicing ($\text{PSNR} > 42\text{ dB}$ on Siemens star), and chromatic-ratio highlight reconstruction ($R=G=B$ above $1.0$ without magenta cast).
  - 52 unit and integration tests passing cleanly across the workspace with zero clippy warnings.

## Exact Next Logical Action

1. **Next Development Task (Task 2.2 — `rraw-gpu`)**:
   - Follow Security Gate 1: Draft and present architectural plan for the VRAM-budgeted LRU DAG cache (`L0`/`L1`/`L2`), 120 Hz RoI scrubbing with `32px` halo, mipmap pyramid generation ($L \in \{0, 1, 2\}$), and Stages 5–7 WGSL compute shaders (`ACEScg` colorimetry, $N_{\text{octaves}} - L$ Laplacian pyramid local contrast, `AgX`/`OpenDRT`, and GPU histogram/scopes).
   - TDD Gate: Scale-invariance RMSE $< 0.5\%$ between $1:1$ ($L=0$) and $1:4$ ($L=2$); 5th photo into a 2-photo LRU cache cleanly evicts oldest VRAM textures.
2. **Subsequent Phase 2 Tasks**:
   - Task 2.3 (`rraw-macos-edr` & `rraw-app`): Safe `objc2` `DisplayHeadroomProvider` bridge, permanently fixed `Rgba16Float` surface, and dual-pass SDR UI + EDR viewport compositor.
