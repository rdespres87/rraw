# AGENTS.md — Rraw Repository Governance Entry Point

Welcome to the **Rraw** workspace. Every independent agent or session working in this repository MUST follow the mandatory startup sequence and engineering rules defined below.

## Mandatory Reading Sequence

Before writing code, modifying files, or proposing architectural changes, read the following governance documents in **exact order**:

1. `docs/agent/AGENT_BOOTSTRAP.md` — Toolchain, repository layout, and English-only mandate.
2. `docs/agent/SESSION_PROTOCOL.md` — Session lifecycle, Grill Orchestrator loop, and Security Gates 1/2/3.
3. `docs/agent/SESSION_STATE.md` — Current verified state of the repository, active phase, and test status.
4. `docs/agent/HANDOFF.md` — Exact next action and context left by the previous session.
5. `docs/agent/RULES.md` — Non-negotiable coding, safety (`#![forbid(unsafe_code)]`), TDD, and git commit rules.
6. `docs/agent/CONTRACTS.md` — Frozen trait interfaces, invariants, concurrency tokens, and mathematical guarantees.
7. `docs/agent/TASK_DECOMPOSITION.md` — Phase-by-phase breakdown of deliverables and TDD gates.
8. `docs/agent/DEFINITION_OF_DONE.md` — Checklist required before any task is declared complete or submitted to Gate 3.
9. `docs/agent/TASKS.md` — Live task status tracker across Phases 0–4.
10. `docs/agent/IMPLEMENTATION_PLAN.md` — Crate-level implementation roadmap and dependency policies.

For the full mathematical and architectural specification, consult `docs/studies/ARCHITECTURE.md`.

## Quick Non-Negotiable Rules Summary

- **English-Only Repository**: All code, comments, shaders, tests, commit messages, and documentation inside this repository MUST be written strictly in English.
- **Zero Crashes in Production & Tests**: `.unwrap()`, `.expect()`, `panic!()`, `todo!()`, and `unimplemented!()` are denied workspace-wide (`clippy::unwrap_used`, `clippy::expect_used`, `clippy::panic`, `clippy::todo`, `clippy::unimplemented`).
- **`unsafe` Isolation (`E0453` Compliant)**: 10 of 11 crates enforce `#![forbid(unsafe_code)]`. Only `rraw-macos-edr` uses `#![deny(unsafe_code)]` with minimal, `// SAFETY:`-documented `#[allow(unsafe_code)]` blocks.
- **Strict TDD**: Follow `Red -> Verify Failure -> Green -> Refactor` using mathematically deterministic synthetic fixtures.
- **No Unapproved Commits or Pushes**: Every commit requires user validation (Gate 3); remote pushes are forbidden unless explicitly authorized by the user.
