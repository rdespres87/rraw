# AGENTS.md — Rraw Repository Context & Domain Guide

Welcome to the **Rraw** workspace.

> **Global Engineering Standards Apply Automatically**:  
> All engineering methodology (strict TDD, 5-phase execution tracking, zero-panic/unwrap prohibition, mandatory merciless local audit via LM Studio, Headroom context compression, and atomic Git commit gates) are **enforced globally** by Antigravity rules (`idiomatic-craft`, `tiered-router`, `efficiency-router`).

---

## 1. Domain & Workspace Architecture

Before working on the codebase, review the domain-specific documentation:
1. `docs/agent/SESSION_STATE.md` — Current verified state of the repository, active crate, and test status.
2. `docs/agent/HANDOFF.md` — Exact next task and context left by the previous session.
3. `docs/agent/CONTRACTS.md` — Frozen mathematical trait interfaces and color science invariants.
4. `docs/agent/TASKS.md` — Task roadmap across Phases 0–4.
5. `docs/studies/ARCHITECTURE.md` — Complete architectural and DSP specification.

---

## 2. Project-Specific Constraints (Rraw Only)

- **Language Mandate**: 100% English repository (code, comments, tests, commit messages, and docs).
- **`unsafe` Isolation (`E0453` Compliant)**: 10 of 11 crates enforce `#![forbid(unsafe_code)]`. Only `rraw-macos-edr` uses targeted `#[allow(unsafe_code)]` with documented `// SAFETY:` comments.
- **Pure-Rust Parsing**: No C/C++ parser bindings (`libraw`, `exiv2` are forbidden). All file parsing (TIFF, ISOBMFF, DNG opcodes, Lensfun) must be pure safe Rust.
- **GPU Testing**: Offscreen headless execution via `GpuContext::new_headless()`.
