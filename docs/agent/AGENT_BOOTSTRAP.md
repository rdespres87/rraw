# AGENT_BOOTSTRAP.md — Session Startup & Environment Guide

This document initializes any independent AI agent or developer session entering the **Rraw** workspace.

## 1. Mandatory Reading Order

Read the following files in exact order before inspecting source code or editing files:

1. `docs/agent/AGENT_BOOTSTRAP.md` (this file)
2. `docs/agent/SESSION_PROTOCOL.md`
3. `docs/agent/SESSION_STATE.md`
4. `docs/agent/HANDOFF.md`
5. `docs/agent/RULES.md`
6. `docs/agent/CONTRACTS.md`
7. `docs/agent/TASK_DECOMPOSITION.md`
8. `docs/agent/DEFINITION_OF_DONE.md`
9. `docs/agent/TASKS.md`
10. `docs/agent/IMPLEMENTATION_PLAN.md`

Refer to `docs/studies/ARCHITECTURE.md` for full mathematical formulas, pipeline diagrams, and design rationale.

## 2. Target Platform & Toolchain

- **Target Host**: macOS on Apple Silicon (M-series unified memory, Metal backend, Apple Neural Engine).
- **Rust Toolchain**: Defined in `rust-toolchain.toml` (`channel = "stable"`, components `rustfmt`, `clippy`).
- **Workspace Resolver**: Cargo workspace with `resolver = "2"` and `edition = "2021"` across 11 crates under `crates/`.
- **License**: `GPL-3.0-or-later` (see `LICENSE`).

## 3. English-Only Mandate

Every artifact tracked inside this repository MUST be written in **English**:
- Documentation (`AGENTS.md`, `README.md`, `docs/agent/*.md`, `docs/studies/*.md`).
- Rust source code, module/struct/function/variable identifiers, docstrings, inline `//` comments, and `// SAFETY:` proofs.
- WGSL compute/render shaders and comments.
- Unit/integration test names, assertions, and fixtures.
- Git commit messages and branch names.

## 4. Verification Baseline Commands

Every session MUST verify workspace health using:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The Git pre-commit hook at `.git/hooks/pre-commit` automatically enforces `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings` prior to any commit.
