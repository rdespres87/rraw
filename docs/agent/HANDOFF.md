# HANDOFF.md — Next Action & Session Continuity

## Last Completed Milestone

- **Phase 3 — Task 3.1 (`rraw-optics` & `rraw-gpu`) Committed (`71635cc`)**:
  - Implemented `OpticalResolver` providing prioritized 4-tier lens correction (Tier 1 Embedded Opcode -> Tier 2 Lensfun XML -> Tier 3 LCP -> Tier 4 Manual).
  - Implemented `UvLutGenerator` creating $256 \times 256$ float lookup tables with differential radial scaling for lateral chromatic aberration (TCA).
  - Implemented Stage 4 Catmull-Rom bicubic `WARP` compute shader with positive half-space clamping ($\max(\text{rgb}, 0.0)$) and fused radial vignetting.
  - Verified that when `vignette_handled_by_gainmap == true`, radial vignette correction is skipped without double-vignetting.
  - 61 unit and integration tests passing cleanly across the workspace with zero clippy warnings.

## Exact Next Logical Action

1. **Next Development Task (Task 3.2 — `rraw-optics` & `rraw-gpu`)**:
   - Follow Security Gate 1: Draft and present architectural plan for Regularized Richardson-Lucy Capture Sharpening:
     - Non-negative input clamping: $I^{(0)}(x) = \max(I_{\text{HLR}}(x), 0.0)$.
     - 5 to 10 iterations with $\epsilon = 10^{-6}$ denominator division-by-zero protection.
     - Regularization edge mask $M_{\text{edge}}(x)$ indexed on Stage 1's `NoiseProfile` protecting 100% of flat regions against noise amplification.
     - Radial blur point spread function $\text{PSF}_r$ and local min/max deringing limiter.
   - TDD Gate: Combined $0.0$ floor + Gaussian edge ($\sigma = 1.1\text{ px}$) + Poisson-Gaussian noisy flat region yields zero `NaN`/`Inf`, $\ge +35\%$ MTF50 improvement, $< +1\%$ noise variance increase in flat region, and zero ringing.
2. **Subsequent Phase 4 Tasks**:
   - Phase 4: CoreML AI (`504×504` Tiling), Local Masks, DAM & HDR Export.
