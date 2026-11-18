# HANDOFF.md — Next Action & Session Continuity

## Last Completed Milestone

- **Phase 1 — Task 1.3 (`rraw-color`) Committed (`bed1a7f`)**:
  - Implemented McCamy CCT solver from CIE 1931 $(x, y)$ chromaticity coordinates.
  - Implemented separated ForwardMatrix (D50 -> Bradford D60 -> `ACEScg`) and ColorMatrix inverse matrix pipelines to `ACEScg` (AP1 D60).
  - Added identity-core Gamut Guard strictly preserving 85% AP1 inner core with zero color shift.
  - Implemented AgX with parametric analytical shoulder for EDR headroom (strictly monotonic and bounded).
  - Validated ColorChecker 24-patch CIEDE2000 metric ($\Delta E_{00} < 0.01$ across all patches on both matrix paths).
  - 45 unit and integration tests passing cleanly across the workspace with zero clippy warnings.

## Exact Next Logical Action

1. **Next Development Task (Task 1.4 — `rraw-cli`)**:
   - Follow Security Gate 1: Draft and present architectural plan for the shared CPU reference pipeline and 16-bit PNG export CLI.
   - Planned work:
     - Shared pure-Rust CPU reference pipeline: signed black/white normalization -> unclamped WB ($\text{sat}_c = w_c$) -> RCD demosaicing -> Highlight Reconstruction -> matrix colorimetry -> AgX DRT -> 16-bit PNG export.
     - CLI flags with `clap` (`--input`, `--output`, `--exposure`, `--wb-kelvin`, `--headroom`).
   - TDD Gate: Synthetic Bayer linear luminance ramp up to physical saturation with $w_R = 2.1, w_G = 1.0, w_B = 1.6$ runs through `rraw-cli` reference pipeline with zero false color or magenta halo up to $\text{sat}_c$ ($R = G = B$ after matrix within $10^{-4}$).
2. **Subsequent Phase 2 Tasks**:
   - Task 2.1 (`rraw-gpu`): Headless `wgpu` context & Stages 1–2 WGSL compute shaders.
   - Task 2.2 (`rraw-gpu`): LRU DAG cache, 120 Hz RoI scrubbing, mipmaps & Stages 5–7.
   - Task 2.3 (`rraw-macos-edr` & `rraw-app`): macOS EDR bridge & dual-pass compositor.
