# HANDOFF.md — Next Action & Session Continuity

## Last Completed Milestone

- **Phase 1 — Task 1.4 (`rraw-cli`) Committed (`486aff9`)**:
  - Implemented `CpuReferencePipeline` and `CpuRgbImage` in `crates/rraw-color/src/pipeline.rs`: signed normalization -> unclamped WB ($\text{sat}_c = w_c$) -> Bayer demosaicing -> Highlight Reconstruction -> AP1 gamut guard & AgX DRT -> 16-bit PNG export.
  - Implemented golden synthetic ramp CLI runner in `crates/rraw-cli/src/main.rs` with `clap` argument parsing.
  - Verified neutrality and highlight saturation integrity ($R = G = B$ within $10^{-4}$ with zero magenta cast).
  - 47 unit and integration tests passing cleanly across the workspace with zero clippy warnings.
  - **Phase 1 is 100% complete**.

## Exact Next Logical Action

1. **Next Development Task (Task 2.1 — `rraw-gpu`)**:
   - Follow Security Gate 1: Draft and present architectural plan for headless `wgpu` initialization and WGSL compute shaders for Stage 1 (signed normalization, defect repair, Anscombe À-Trous wavelets) and Stage 2 (demosaicing & HLR).
   - TDD Gate: Red integration test using headless `wgpu` device verifying bitwise/numerical parity with CPU reference pipeline outputs.
2. **Subsequent Phase 2 Tasks**:
   - Task 2.2 (`rraw-gpu`): LRU DAG cache, 120 Hz RoI scrubbing, mipmaps & Stages 5–7.
   - Task 2.3 (`rraw-macos-edr` & `rraw-app`): macOS EDR bridge & dual-pass compositor.
