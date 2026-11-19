# SESSION_PROTOCOL.md — Standard Engineering Lifecycle

The standard development lifecycle in **Rraw** is governed globally by Antigravity rules (`idiomatic-craft`, `tiered-router`). Every task executes through the standardized 5-phase protocol:

1. `[PHASE 1/5 : CONCEPTION & DÉVELOPPEMENT]` : Architecture, strict TDD (`Red -> Verify Failure -> Green -> Refactor`), domain contracts.
2. `[PHASE 2/5 : VALIDATION NATIVE & TESTS CI]` : Full workspace verification (`cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`).
3. `[PHASE 3/5 : AUDIT LOCAL IMPITOYABLE (0 TOKENS)]` : Adversarial audit of `git diff` via LM Studio (`execute_local_task`, skill `merciless-local-audit`).
4. `[PHASE 4/5 : AUTO-HEALING & CORRECTIFS]` : Resolving blocking flaws locally before proceeding.
5. `[PHASE 5/5 : RAPPORT DÉTAILLÉ & PROTOCOLE DE TEST]` : Final restitution, automated test commands, and manual test scenario before Gate 3 (Commit).

---

## State Updates at Phase Boundary
Upon completing a milestone:
1. Update `docs/agent/SESSION_STATE.md`.
2. Update `docs/agent/TASKS.md`.
3. Update `docs/agent/HANDOFF.md` with the next logical action.
