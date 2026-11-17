# IMPLEMENTATION_PLAN.md — Crate Dependency Architecture & Phased Crate Roadmap

## 1. Crate Dependency Graph (Strictly Acyclic)

```text
rraw-app ──► { all 9 library crates }
rraw-cli ──► { rraw-core, rraw-decode, rraw-optics, rraw-color, rraw-gpu, rraw-ai, rraw-storage, rraw-export }

rraw-export    ──► { rraw-core, rraw-gpu, rraw-color }
rraw-gpu       ──► { rraw-core, rraw-optics, rraw-color }
rraw-color     ──► { rraw-core, rraw-optics }
rraw-optics    ──► { rraw-core, rraw-decode }
rraw-storage   ──► { rraw-core, rraw-decode }
rraw-decode    ──► { rraw-core }
rraw-ai        ──► { rraw-core }
rraw-macos-edr ──► { rraw-core }
rraw-core      ──► { } (no internal crate dependencies)
```

## 2. Crate-by-Crate Implementation & Approved Dependencies Plan

| Crate | `unsafe` Attribute | Phase Target | Planned External Crates (Introduced on Demand in Phases 1–4) |
| :--- | :---: | :---: | :--- |
| `rraw-core` | `#![forbid(unsafe_code)]` | Phase 0 / 1 | `serde`, `serde_json`, `thiserror`, `blake3` (or `sha2`) for deterministic stage hashes |
| `rraw-decode` | `#![forbid(unsafe_code)]` | Phase 0 / 1 | `rawler` (`LGPL-2.1` / `GPL-2.0+`), `byteorder` / `nom` (pure Rust binary parsing) |
| `rraw-optics` | `#![forbid(unsafe_code)]` | Phase 0 / 3 | `quick-xml` (Lensfun XML), `glam` (2D/3D math) |
| `rraw-color` | `#![forbid(unsafe_code)]` | Phase 0 / 1 | `glam` (3x3 spectral matrices), `png` (CPU reference golden export) |
| `rraw-gpu` | `#![forbid(unsafe_code)]` | Phase 0 / 2 | `wgpu`, `bytemuck`, `pollster`, `arc-swap` |
| `rraw-ai` | `#![forbid(unsafe_code)]` | Phase 0 / 4 | `ort` (`MIT`, CoreML Execution Provider), `ndarray` |
| `rraw-storage` | `#![forbid(unsafe_code)]` | Phase 0 / 4 | `rusqlite` (bundled), `quick-xml` (`.xmp` round-trip), `notify`, `tokio` |
| `rraw-export` | `#![forbid(unsafe_code)]` | Phase 0 / 4 | `tiff`, `png`, `jpeg-encoder`, `ravif` / `rav1e`, `avif-serialize`, `jpegxl-rs` (`BSD-3-Clause`), `rayon` |
| `rraw-macos-edr` | `#![deny(unsafe_code)]` | Phase 0 / 2 | `objc2`, `objc2-app-kit`, `objc2-quartz-core`, `objc2-core-graphics`, `objc2-foundation` |
| `rraw-cli` | `#![forbid(unsafe_code)]` | Phase 0 / 1 | `clap` |
| `rraw-app` | `#![forbid(unsafe_code)]` | Phase 0 / 2–4 | `eframe`, `egui`, `egui-wgpu`, `wgpu`, `rfd` |

## 3. Phase 0 Baseline Policy

During **Phase 0**, all 11 crates use **pure Rust `std` only** (zero external crate downloads) so the workspace skeleton, cross-crate dependency topology, `#![forbid(unsafe_code)]` / `#![deny(unsafe_code)]` enforcement, `[workspace.lints.clippy]` rules, and foundational contract tests compile and verify deterministically offline. External dependencies are added incrementally in Phases 1–4 as each module enters TDD implementation, subject to license verification (`GPL-3.0-or-later` compatibility) and `CONTRACTS.md`.
