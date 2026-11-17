# HANDOFF.md — Next Action & Session Continuity

## Last Completed Milestone

- **Phase 0 — Governance, Documentation & 11-Crate Workspace Initialization (Committed)**:
  - Initialized local Git repository (`main`) and installed executable `.git/hooks/pre-commit`.
  - Commit 1 (`2adfa28`): `.gitignore`, `rust-toolchain.toml`, `LICENSE` (`GPL-3.0-or-later`), root `Cargo.toml` workspace lints (`unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented` denied), `Cargo.lock`, and 11 compilable crates (`10` `#![forbid(unsafe_code)]`, `rraw-macos-edr` `#![deny(unsafe_code)]`) with `32` passing contract/smoke unit tests.
  - Commit 2: `AGENTS.md`, `README.md`, `docs/studies/ARCHITECTURE.md`, and complete 10-file governance suite in `docs/agent/`.

## Exact Next Logical Action

1. **Next Development Task (Task 1.1 — `rraw-core`)**:
   - Follow strict TDD (`Red -> Verify Failure -> Green -> Refactor`) to implement the full versioned `.rraw.json` recipe DAG, canonical deterministic serialization, and stage-scoped cache hashing (`L0`, `L1`, `L2`, display hash) in `crates/rraw-core`.
   - Gate test: Verify that mutating exposure invalidates the display hash while preserving `L0`, `L1`, and `L2` cache hashes identically, and mutating optical distortion invalidates `L2` and display hashes while preserving `L0` and `L1`.
2. **Subsequent Phase 1 Tasks**:
   - Task 1.2 (`rraw-decode`): `rawler` behind `RawDecoder` + synthetic RAW generator + `NoiseProfile` (`0xc761` & MAD fallback) + binary DNG `OpcodeList1..3` / `GainMap` parsers.
   - Task 1.3 (`rraw-color`): Iterative CCT solver, 1D mired interpolation, `ForwardMatrix1/2` & `ColorMatrix1/2` -> Bradford -> `ACEScg`, identity-core Gamut Guard, `AgX`/`OpenDRT`.
   - Task 1.4 (`rraw-cli`): Shared CPU reference pipeline & 16-bit PNG export.
