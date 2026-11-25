# Rraw v0.3.1

## Highlights

- **Fused GPU Pipeline & Shared CLUT** — WGSL stages 5–7 now submit in a single command buffer with a shared 3D CLUT held via `Arc`, eliminating per-stage submission overhead and redundant GPU memory cloning.
- **Session DAM & Non-Blocking Ratings** — Session DAM initializes on startup with decoupled directory-index epoch tracking; SQLite rating writes are offloaded to a background worker so the UI thread never blocks on persistence.
- **Unsafe-Code Isolation** — `#![forbid(unsafe_code)]` enforced in `rraw-macos-edr` (11 of 12 workspace crates now forbid unsafe); redundant `unsafe` allows removed and `sysctlbyname` buffer bounds hardened in `rraw-coreml`.
- **Codebase Hygiene & Modularity** — Obsolete `XmpEditor` and legacy `.xmp` sidecar paths removed (ISO 21496-1 `.hdrgm.xmp` HDR gain-map export retained); darkroom inspector sections decomposed, JXL guard deduplicated, and high-arity viewport/CFA helpers wrapped in context structs.
- **Cross-Platform CI & Deterministic Tests** — Linux (`ubuntu-latest`) CI job and `cargo-audit` security workflow added; baseline ADRs introduced, test sleeps replaced with deterministic synchronization, and DAG coverage expanded (467 tests, 95 suites, 0 Clippy warnings, 87.68% 4-crate CPU line coverage).

## Installation

| File | Description |
|------|-------------|
| `Rraw-v0.3.1.dmg` | macOS installer (Apple Silicon) |
| `Rraw-v0.3.1.dmg.sha256` | SHA-256 checksum for integrity verification |

**Requirements:** macOS 14 (Sonoma) or 15 (Sequoia) on Apple Silicon.

```bash
# Verify download integrity
shasum -a 256 -c Rraw-v0.3.1.dmg.sha256
```

## Upgrade Notes

No migration steps required. The fused GPU pipeline and shared CLUT are internal render-path changes with no effect on saved `.rraw.json` recipes, layout preferences, or grading presets. Session DAM and background rating persistence are additive. Note: legacy `.xmp` sidecar files (non-HDR) are no longer generated or consumed; ISO 21496-1 `.hdrgm.xmp` HDR gain-map export remains fully supported. All previously supported output formats are unchanged.
