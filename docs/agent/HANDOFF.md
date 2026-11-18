# HANDOFF.md — Next Action & Session Continuity

## Last Completed Milestone

- **Phase 3 — Task 3.2 (`rraw-optics` & `rraw-gpu`) Committed (`cd5474f`)**:
  - Implemented CPU reference `execute_richardson_lucy` with radial Gaussian PSF and sensor noise regularizer mask $M_{\text{edge}}$.
  - Implemented Stage 3 WGSL compute shader (`stage3_rl.wgsl`) with `Stage3RlPipeline` executing on Apple Silicon Metal.
  - Enforced deringing envelope constraints $[m \cdot (1 - \delta), M \cdot (1 + \delta)]$ and guaranteed $< +1\%$ flat noise amplification.
  - 65 unit and integration tests passing cleanly across the workspace with zero clippy warnings.
  - **Phase 3 is 100% complete**.

## Exact Next Logical Action

1. **Next Development Task (Task 4.1 — `rraw-ai`)**:
   - Follow Security Gate 1: Draft and present architectural plan for CoreML AI Tile Denoiser:
     - Phase-preserving $504 \times 504$ tiling with $48\text{ px}$ $\cos^2$ smooth spatial blend overlap.
     - Raw Bayer pattern alignment preserving RGGB phase across tile boundaries.
     - FP16 Anscombe and log conditioning inputs for the neural network.
     - Monotonic `(PhotoId, Stage0Epoch)` cancellation check ensuring superseded tiles are dropped without committing to L0 cache.
     - Abstract `NeuralTileDenoiser` engine trait supporting simulated/ONNX/CoreML execution.
2. **Subsequent Phase 4 Tasks**:
   - Task 4.2 (`rraw-gpu` & `rraw-app`): Post-warp geometric & ACEScg parametric masks.
   - Task 4.3 (`rraw-storage` & `rraw-app`): Async SQLite DAM, sidecars, and browser UI.
   - Task 4.4 (`rraw-export`): Multi-threaded batch exporter + ISO 21496-1 HDR Gain Maps.
