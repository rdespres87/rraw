# SESSION_STATE.md — Live Repository State

> Keep this file strictly synchronized with verified workspace state, test counts, and commit history. Never claim a check passed unless executed after the last edit.

## Current Status

- **Active Phase**: Phase 1 — Mathematical Foundations, Pure-Rust Decoding, Colorimetry & CLI
- **Active Tasks**:
  - Task 0.1 & Task 0.2: **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized)
  - Task 1.1 (`rraw-core`): **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized, commit `acedebb`)
  - Task 1.2 (`rraw-decode`): Ready to start under strict TDD (`RawDecoder` over `rawler` + synthetic RAW generator, `NoiseProfile` extraction & wavelet MAD fallback, DNG `OpcodeList1..3` & `GainMap`)
- **Git Branch**: `main` (Task 1.1 committed in `acedebb`, docs synchronized in `d3b0c3d`)
- **Last Approved Gate**: Gate 1 & Gate 3 (Task 1.1 authorized by user on 2026-11-18)

## Workspace Structure Verified

- 11 Cargo workspace members under `crates/`:
  - `rraw-core` (`lib`, `#![forbid(unsafe_code)]`)
  - `rraw-decode` (`lib`, `#![forbid(unsafe_code)]`)
  - `rraw-optics` (`lib`, `#![forbid(unsafe_code)]`)
  - `rraw-color` (`lib`, `#![forbid(unsafe_code)]`)
  - `rraw-gpu` (`lib`, `#![forbid(unsafe_code)]`)
  - `rraw-ai` (`lib`, `#![forbid(unsafe_code)]`)
  - `rraw-storage` (`lib`, `#![forbid(unsafe_code)]`)
  - `rraw-export` (`lib`, `#![forbid(unsafe_code)]`)
  - `rraw-macos-edr` (`lib`, `#![deny(unsafe_code)]`)
  - `rraw-cli` (`bin`, `#![forbid(unsafe_code)]`)
  - `rraw-app` (`bin`, `#![forbid(unsafe_code)]`)

## Known Open Items

- Task 1.1 Inquisitor audit: **APPROVED** (`37` passing unit tests across workspace).
- Next up: Task 1.2 (`rraw-decode`).

## Last Observed Verification Run

- `./.git/hooks/pre-commit` (`cargo fmt --all -- --check` + `cargo clippy --workspace --all-targets -- -D warnings`): passed (exit `0`).
- `cargo test --workspace`: passed (`37` unit tests across all `11` crates: `rraw-core`: 14, `rraw-ai`: 4, `rraw-color`: 3, `rraw-decode`: 3, `rraw-gpu`: 3, `rraw-app`: 2, `rraw-macos-edr`: 2, `rraw-optics`: 2, `rraw-storage`: 2, `rraw-cli`: 1, `rraw-export`: 1; `0` failed, `0` ignored).
