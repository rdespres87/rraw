# HANDOFF.md — Next Action & Session Continuity

## Last Completed Milestone

- **Phase 1 — Task 1.1 (`rraw-core`) Committed (`acedebb`)**:
  - Implemented versioned `.rraw.json` recipe DAG (`schema_version = 1`) across all photographic stages (`Stage1SensorParams` through `Stage7DisplayParams`).
  - Implemented deterministic stage-scoped SHA-256 cache hashing (`hash_l0`, `hash_l1`, `hash_l2`, `hash_display`).
  - Added canonical JSON serialization and schema version validation.
  - Verified TDD isolation invariants (exposure changes invalidate display hash while keeping L0, L1, L2 bitwise identical; optics changes invalidate L2 and display while keeping L0 and L1 identical).
  - 37 unit and integration tests passing cleanly across the workspace with zero clippy warnings.

## Exact Next Logical Action

1. **Next Development Task (Task 1.2 — `rraw-decode`)**:
   - Follow Security Gate 1: Draft and present architectural plan for pure-Rust decoding in `crates/rraw-decode`.
   - Planned work:
     - Wrap `rawler` behind `RawDecoder` trait with pure-safe Rust fallback / synthetic RAW generator.
     - DNG `NoiseProfile` tag `0xc761` extraction and wavelet MAD fallback estimator.
     - Binary DNG `OpcodeList1..3` parser (`WarpRectilinear`, `FixVignetteRadial`, `GainMap` raising `vignette_handled_by_gainmap`).
   - TDD Gate: Bit-exact decoding of synthetic DNG 1.7 `OpcodeList3` + `GainMap`; MAD recovers synthetic noise $\sigma = 0.02$ within $\pm 2\%$; signed sub-black samples remain $< 0.0$ with zero mean ($|\mu| < 10^{-4}$).
2. **Subsequent Phase 1 Tasks**:
   - Task 1.3 (`rraw-color`): Iterative CCT solver, 1D mired interpolation, `ForwardMatrix1/2` & `ColorMatrix1/2` -> Bradford -> `ACEScg`, identity-core Gamut Guard, `AgX`/`OpenDRT`.
   - Task 1.4 (`rraw-cli`): Shared CPU reference pipeline & 16-bit PNG export.
