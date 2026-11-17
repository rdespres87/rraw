# RULES.md — Non-Negotiable Engineering & Safety Rules

Every line of code, shader, test, and documentation committed to **Rraw** MUST comply with the following rules.

## 1. Language & Documentation Rule

- **100% English Repository**: Every file (`AGENTS.md`, `README.md`, `LICENSE`, `docs/agent/*.md`, `docs/studies/*.md`, `Cargo.toml`, `crates/**/*.rs`, WGSL shaders, comments, tests, commit messages) MUST be strictly in English.

## 2. Zero-Panic & Error Handling Rule

- `.unwrap()`, `.expect()`, `panic!()`, `todo!()`, and `unimplemented!()` are **strictly prohibited** across **all workspace targets including unit/integration tests, examples, and binaries** (`cargo clippy --workspace --all-targets -- -D warnings`).
- Enforced workspace-wide in root `Cargo.toml` under `[workspace.lints.clippy]`:
  - `unwrap_used = "deny"`
  - `expect_used = "deny"`
  - `panic = "deny"`
  - `todo = "deny"`
  - `unimplemented = "deny"`
- Every crate's `Cargo.toml` MUST include `[lints] workspace = true`, and every error type (`CoreError`, `AiError`, `DecodeError`, `ColorError`, etc.) MUST implement `std::fmt::Display` and `std::error::Error`.
- In tests, return `Result<(), ...>` with `?`, or use `assert!`, `assert_eq!`, `assert_ne!`, `matches!`, or explicit `match` / `let ... else` — never `.unwrap()`, `.expect()`, or `panic!()`.
- Never silently swallow errors (`let _ = fallible_call();` is forbidden unless explicitly documented why an infallible/best-effort notification drop is intentional).

## 3. `unsafe` Isolation Rule (`E0453` Compliant)

- **10 of 11 crates MUST declare `#![forbid(unsafe_code)]`** at the top of `src/lib.rs` or `src/main.rs`:
  - `rraw-core`, `rraw-decode`, `rraw-optics`, `rraw-color`, `rraw-gpu`, `rraw-ai`, `rraw-storage`, `rraw-export`, `rraw-cli`, `rraw-app`.
- **Only `rraw-macos-edr`** may declare `#![deny(unsafe_code)]` instead of `#![forbid(unsafe_code)]` (because Rust compiler error `E0453` blocks inner `#[allow(unsafe_code)]` when crate-level `forbid` is active).
- Every `unsafe` block inside `rraw-macos-edr` MUST:
  1. Carry a targeted `#[allow(unsafe_code)]` on the smallest possible scope.
  2. Include a `// SAFETY:` comment proving all preconditions, thread-affinity (`MainThreadMarker` / AppKit main thread), pointer validity, and lifetime invariants.
  3. Expose only 100% safe Rust interfaces (`DisplayHeadroomProvider`) to downstream crates.

## 4. Pure-Rust Parsing & Dependency Governance

- **No legacy C/C++ parsing libraries**: `LibRaw`, `exiv2`, `rexiv2`, `gobject`, and similar C/C++ parser bindings are forbidden.
- **All file format parsing and business logic MUST be pure safe Rust**: TIFF/ISOBMFF, DNG `OpcodeList1..3`, MakerNotes, Lensfun XML, `.lcp` profiles, and `.xmp` sidecars.
- **Permissible Isolated FFI Exceptions**:
  - `wgpu` / `objc2` Apple frameworks (`Metal`, `AppKit`, `CoreGraphics`, `QuartzCore`) isolated to `rraw-gpu` / `rraw-macos-edr`.
  - `ort` (ONNX Runtime, `MIT`) for CoreML / ANE isolated to `rraw-ai`.
  - `jpegxl-rs` / `libjxl` (`BSD-3-Clause`) isolated to `rraw-export` (AVIF uses pure-Rust `rav1e` + `avif-serialize`).
- Respect `GPL-3.0-or-later` license compatibility for every new dependency added to `Cargo.toml`.

## 5. Strict Test-Driven Development (TDD)

Every feature and bug fix follows **Red -> Verify Failure -> Green -> Refactor**:
1. Write unit/integration tests using **mathematically deterministic synthetic fixtures** (known CFA patterns, Poisson-Gaussian synthetic noise, analytical PSF blur, analytical distortion grids, analytical ColorChecker patches).
2. Run tests and verify they fail for the expected reason before writing implementation code.
3. Implement clean, minimal production code satisfying the tests and `CONTRACTS.md`.
4. Refactor without duplicating logic, keeping functions focused and shallowly nested.
5. GPU tests in `rraw-gpu` MUST execute headless offscreen via `GpuContext::new_headless()`.

## 6. Atomic & Compilable Commits + State Sync

- Every commit MUST compile cleanly and pass `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`.
- Never commit broken scaffolding, commented-out tests, or failing code.
- Never commit without Security Gate 3 user approval.
- Never push to a remote repository without explicit user approval.
- Synchronize `docs/agent/SESSION_STATE.md`, `docs/agent/TASKS.md`, and `docs/agent/HANDOFF.md` with each completed milestone.
