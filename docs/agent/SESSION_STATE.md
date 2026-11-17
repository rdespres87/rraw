# SESSION_STATE.md — Live Repository State

> Keep this file strictly synchronized with verified workspace state, test counts, and commit history. Never claim a check passed unless executed after the last edit.

## Current Status

- **Active Phase**: Phase 0 complete (`2adfa28` + governance docs committed); transitioning to Phase 1 — Mathematical Foundations, Pure-Rust Decoding, Colorimetry & CLI
- **Active Tasks**:
  - Task 0.1 & Task 0.2: **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized)
  - Task 1.1 (`rraw-core`): Ready to start under strict TDD (versioned `.rraw.json` recipe DAG and deterministic stage-scoped cache hashing)
- **Git Branch**: `main` (Commit 1 `2adfa28` committed; Commit 2 adding `AGENTS.md` and `docs/` ready)
- **Last Approved Gate**: Gate 1 (`architecture_plan.md`) & Gate 3 (Phase 0 commits authorized by user on 2026-11-17)

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

- Phase 0 Inquisitor audit: **APPROVED** (`32` passing unit tests across all `11` workspace crates).
- External dependencies (`rawler`, `wgpu`, `eframe`, `ort`, `rusqlite`, `objc2`, `rav1e`, `jpegxl-rs`, `thiserror`, `serde`, `quick-xml`, etc.) are intentionally deferred to Phases 1–4 as scheduled in `IMPLEMENTATION_PLAN.md`.

## Last Observed Verification Run

- `./.git/hooks/pre-commit` (`cargo fmt --all -- --check` + `cargo clippy --workspace --all-targets -- -D warnings`): passed (exit `0`).
- `cargo test --workspace`: passed (`32` unit tests across all `11` crates: `rraw-core`: 9, `rraw-ai`: 4, `rraw-color`: 3, `rraw-decode`: 3, `rraw-gpu`: 3, `rraw-app`: 2, `rraw-macos-edr`: 2, `rraw-optics`: 2, `rraw-storage`: 2, `rraw-cli`: 1, `rraw-export`: 1; `0` failed, `0` ignored).
