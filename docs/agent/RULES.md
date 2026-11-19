# RULES.md — Rraw Domain-Specific Constraints

> **Note**: General software engineering rules (Zero `.unwrap()`/`.expect()`, strict TDD, zero-warning CI, 5-phase execution tracking, merciless local audit, and atomic Git commit gates) are enforced **globally** by Antigravity rules (`idiomatic-craft`, `tiered-router`).

The following constraints are specific to the **Rraw** domain:

---

## 1. Language Mandate
- **100% English Repository**: All code, shaders, tests, commit messages, and documentation inside this repository MUST be strictly in English.

---

## 2. `unsafe` Isolation (`E0453` Compliant)
- **10 of 11 crates MUST declare `#![forbid(unsafe_code)]`** at the top of `src/lib.rs` or `src/main.rs`:
  - `rraw-core`, `rraw-decode`, `rraw-optics`, `rraw-color`, `rraw-gpu`, `rraw-ai`, `rraw-storage`, `rraw-export`, `rraw-cli`, `rraw-app`.
- **Only `rraw-macos-edr`** may declare `#![deny(unsafe_code)]` with minimal, targeted `// SAFETY:`-documented `#[allow(unsafe_code)]` blocks for AppKit/Metal FFI.

---

## 3. Pure-Rust Parsing & Dependencies
- **No legacy C/C++ parsing bindings**: `libraw`, `exiv2`, `rexiv2` are strictly forbidden. All raw formats (TIFF, ISOBMFF, DNG opcodes, Lensfun XML) must be parsed in pure safe Rust.
- **Isolated FFI Frameworks**: Only `wgpu` / Apple Metal/AppKit in `rraw-gpu` / `rraw-macos-edr`, `ort` in `rraw-ai`, and `jpegxl-rs` in `rraw-export`.
- **License**: Strictly respect `GPL-3.0-or-later` compatibility.
