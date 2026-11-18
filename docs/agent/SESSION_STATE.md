# SESSION_STATE.md — Live Repository State

> Keep this file strictly synchronized with verified workspace state, test counts, and commit history. Never claim a check passed unless executed after the last edit.

## Current Status

- **Active Phase**: Phase 2 — Headless `wgpu` GPU Pipeline, LRU DAG Cache & 120 Hz EDR Viewport
- **Active Tasks**:
  - Task 0.1 & Task 0.2: **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized)
  - Task 1.1 (`rraw-core`): **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized, commit `acedebb`)
  - Task 1.2 (`rraw-decode`): **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized, commit `8fa0a80`)
  - Task 1.3 (`rraw-color`): **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized, commit `bed1a7f`)
  - Task 1.4 (`rraw-cli`): **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized, commit `486aff9`)
  - Task 2.1 (`rraw-gpu`): **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized, commit `4eb20a4`)
  - Task 2.2 (`rraw-gpu`): **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized, commit `8e922ad`)
  - Task 2.3 (`rraw-macos-edr` & `rraw-app`): Ready to start under strict TDD (safe `objc2` `DisplayHeadroomProvider` bridge, permanently fixed `Rgba16Float` surface, dual-pass SDR UI + EDR viewport compositor)
- **Git Branch**: `main` (Task 2.2 committed in `8e922ad`)
- **Last Approved Gate**: Gate 3 (Task 2.2 authorized by user on 2026-11-18)

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

- Task 2.2 Complete (commit `8e922ad` with 56 passing tests).
- Next up: Task 2.3 (`rraw-macos-edr` & `rraw-app`).

## Last Observed Verification Run

- `./.git/hooks/pre-commit` (`cargo fmt --all -- --check` + `cargo clippy --workspace --all-targets -- -D warnings`): passed (exit `0`).
- `cargo test --workspace`: passed (`56` unit/integration tests across all `11` crates: `rraw-core`: 14, `rraw-gpu`: 12, `rraw-color`: 7, `rraw-decode`: 7, `rraw-cli`: 3, `rraw-ai`: 4, `rraw-app`: 2, `rraw-macos-edr`: 2, `rraw-optics`: 2, `rraw-storage`: 2, `rraw-export`: 1; `0` failed, `0` ignored).
