# HANDOFF.md — Next Action & Session Continuity

## Last Completed Milestone

- **Phase 1 — Task 1.2 (`rraw-decode`) Committed (`8fa0a80`)**:
  - Implemented `PureRustRawDecoder` wrapping `rawler` behind `RawDecoder` trait.
  - Added binary DNG `OpcodeList1..3` parser with `GainMap`, `FixVignetteRadial`, `WarpRectilinear`, and `WarpFisheye` support, properly configuring `vignette_handled_by_gainmap`.
  - Implemented DNG tag `0xc761` extractor and robust 2D Haar wavelet MAD noise estimator ($\sigma = 0.02$ recovered within $\pm 2\%$).
  - Implemented deterministic `SyntheticRawBuilder` with Box-Muller Gaussian noise and zero-mean sub-black normalizations.
  - 41 unit and integration tests passing cleanly across the workspace with zero clippy warnings.

## Exact Next Logical Action

1. **Next Development Task (Task 1.3 — `rraw-color`)**:
   - Follow Security Gate 1: Draft and present architectural plan for spectral colorimetry in `crates/rraw-color`.
   - Planned work:
     - Iterative Correlated Color Temperature (CCT) solver and 1D mired linear interpolation.
     - Dual-Illuminant matrix pipelines: separated Path A (`ForwardMatrix1/2` -> Bradford `D50 -> D60` -> `ACEScg`) and Path B (`ColorMatrix1/2` -> invert -> Bradford `WP -> D60` -> `ACEScg`).
     - Identity-core Gamut Guard (85% `AP1` inner core strictly identity).
     - Display Rendering Transforms (`AgX` with parametric analytical shoulder for EDR, `OpenDRT`).
   - TDD Gate: $\Delta E_{00} < 0.01$ across all 24 analytical ColorChecker patches on both matrix paths; `AgX(x, H)` strictly monotonically increasing ($df/dx > 0$) and bounded by $H$ on $x \in [0, 100]$.
2. **Subsequent Phase 1 Tasks**:
   - Task 1.4 (`rraw-cli`): Shared CPU reference pipeline & 16-bit PNG export.
