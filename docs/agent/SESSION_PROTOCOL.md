# SESSION_PROTOCOL.md — Grill Orchestrator Workflow & Security Gates

**Rraw** uses an adversarial multi-role engineering protocol (**Grill Orchestrator**) with strict Human-in-the-Loop (HITL) Security Gates.

## 1. Roles

- **Orchestrator**: Coordinates phases, reads governance documents, manages subagents, runs verification commands, enforces Security Gates 1/2/3, and maintains repository state.
- **Maker**: Designs clean software architecture and writes production Rust/WGSL code and tests following strict TDD (`Red -> Verify Failure -> Green -> Refactor`) and `RULES.md`.
- **Inquisitor**: Adversarial reviewer that inspects Maker implementations and `git diff` for subtle logic inversions, contract violations, `unsafe` leaks, concurrency/epoch bugs, error suppression, and missing test coverage. Does NOT write production code.

## 2. Adversarial Workflow & Security Gates

### Step 1: Initialization
Read `AGENTS.md` and the 10 documents in `docs/agent/` in exact order. Inspect `SESSION_STATE.md` and `HANDOFF.md` to confirm the active task.

### Step 2: Architecture & Security Gate 1 (Plan Validation)
- For new architectural modules, complex protocols, or major milestones, the Maker drafts an architectural plan.
- **SECURITY GATE 1**: Implementation MUST NOT begin until the user explicitly approves the architectural plan.

### Step 3: TDD Implementation & The Grill Loop (Security Gate 2)
1. **Red**: Write deterministic synthetic failing tests first; observe the failure (`cargo test`).
2. **Green**: Implement the minimal clean solution satisfying the tests and `CONTRACTS.md`.
3. **Refactor**: Eliminate duplication, sharpen types, and verify zero Clippy warnings.
4. **Inquisitor Audit**: Submit changes to the Inquisitor. Fix all identified defects.
5. **SECURITY GATE 2 (Arbitration)**: If the Maker and Inquisitor disagree for more than 2 iterations without an `APPROVED` verdict from the Inquisitor, stop and escalate to the user with the exact `diff`/snippets, Maker rationale, and Inquisitor critique for arbitration.

### Step 4: Full CI Verification & Post-Implementation Final Grill
1. Run `cargo fmt --all -- --check`.
2. Run `cargo clippy --workspace --all-targets -- -D warnings`.
3. Run `cargo test --workspace`.
4. Perform a final adversarial diff pass checking:
   - Polarity/range inversions (black-level sign, white-balance clamping, headroom bounds, mask coordinate frames).
   - Concurrency & epoch cancellation (`(PhotoId, Stage0Epoch)`, pointer/drag release consolidation).
   - Silent error suppression (`let _ = ...`), unbounded allocations, or missing edge-case tests.
5. Add targeted regression tests for any edge case discovered and re-run full CI.

### Step 5: Security Gate 3 (Commit Validation)
- **STRICT PROHIBITION**: Agents MUST NEVER run `git commit` autonomously without explicit user approval.
- Before requesting Gate 3 approval, present:
  - The full `git diff` (or summary of new untracked files / staged diff).
  - A concise, factual summary of changes and exact observed verification command outputs.
- Only commit after explicit user authorization ("Yes, proceed with commit").

### Step 6: Remote Push Prohibition
- Agents MUST NEVER execute `git push` or create remote branches/tags/PRs unless explicitly instructed by the user in that turn.

### Step 7: State Synchronization & Handoff
Immediately after every approved commit (or before session termination):
1. Update `docs/agent/SESSION_STATE.md` with verified command outputs, commit SHA, and current branch status.
2. Update `docs/agent/TASKS.md` task statuses.
3. Update `docs/agent/HANDOFF.md` with the exact next logical action and blockers/prerequisites.
