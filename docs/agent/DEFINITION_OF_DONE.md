# DEFINITION_OF_DONE.md — Completion Checklist

A task in **Rraw** MUST satisfy every criterion below before requesting Security Gate 3 commit authorization or marking the task `[x]` in `TASKS.md`.

## 1. Governance & Linguistic Hygiene
- [ ] Every modified or created file (code, comments, shaders, tests, docs) is **100% in English**.
- [ ] No extraneous temporary files, debug prints, or unrelated workspace modifications exist.

## 2. Safety & Static Analysis Compliance
- [ ] `cargo fmt --all -- --check` exits `0` with zero formatting diffs.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` exits `0` with zero warnings.
- [ ] Zero `.unwrap()`, `.expect()`, `panic!()`, `todo!()`, `unimplemented!()` in production or test code.
- [ ] All 10 non-FFI crates compile with `#![forbid(unsafe_code)]`; `rraw-macos-edr` compiles with `#![deny(unsafe_code)]` and every `#[allow(unsafe_code)]` block has an explicit, accurate `// SAFETY:` proof.
- [ ] No silent error suppression (`let _ = ...`) on fallible operations.

## 3. TDD & Contract Verification
- [ ] Tests were authored first (`Red -> Verify Failure -> Green -> Refactor`) using deterministic synthetic fixtures.
- [ ] `cargo test --workspace` executes cleanly with 100% passing tests (no `#[ignore]`, no relaxed assertions).
- [ ] Headless GPU tests (`rraw-gpu`) run via `GpuContext::new_headless()` without requiring an interactive display server.
- [ ] All invariants in `docs/agent/CONTRACTS.md` touched by the task remain intact and explicitly covered by regression tests.

## 4. Adversarial Review & State Synchronization
- [ ] Inquisitor review completed (`APPROVED` verdict or Gate 2 user arbitration resolved).
- [ ] Post-implementation adversarial diff review completed (polarity, concurrency, queue saturation, edge cases).
- [ ] Security Gate 3 user approval obtained before running `git commit`.
- [ ] `docs/agent/SESSION_STATE.md`, `docs/agent/TASKS.md`, and `docs/agent/HANDOFF.md` updated cleanly.
