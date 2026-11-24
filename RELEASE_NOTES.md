# Rraw v0.2.0

## Highlights

- **Live WGPU Interactive Preview Backend** — Real-time WGPU/Metal preview execution (Stages 5–7) in `rraw-app` with automatic CPU/Rayon fallback, pre-Stage-5 linear evaluation for real RAW photos with auto Lensfun profiles & Smart Lighting, canonical `agx_drt` shader parity ($\Delta E_{00} < 1.0$), persisted `gpu_preview_enabled` preference, and a clickable `⚡ GPU` / `⚙ CPU` status bar badge with adapter & fallback diagnostics.
- **Horizontal Keystone Auto-Detection (2-Parallel-Lines Tool)** — The interactive 2-parallel-lines perspective tool automatically detects predominantly horizontal lines (`|dx| > |dy|`) to compute `keystone_horizontal` + `rotation_degrees` while preserving existing `keystone_vertical` adjustments, displaying `Keystone H` / `Keystone V` in the status banner.
- **Cooperative Export Cancellation & Bounded `.cube` LUT I/O** — Added responsive `Cancel` control (`Arc<AtomicBool>`) for background batch/timelapse exports across `en`/`fr`/`es`, a 16 MiB TOCTOU-safe `.cube` LUT file-size guard, and purged all residual non-English strings from `export_runner.rs`.
- **Codebase Hygiene & Zero Clippy Allows** — Eliminated all 3 remaining `#[allow(clippy::too_many_lines)]` (`masks.rs`, `crop_overlay.rs`, `perspective_tools.rs`), fixed `active_local_overlay` stale-index removal bug, replaced 1,907 lines of `i18n/key.rs` boilerplate with a declarative `define_i18n_keys!` macro implementing `core::str::FromStr` (zero `clippy::should_implement_trait` allows), and added 9 `rraw-macos-edr` integration contract tests.
- **CI & Verification** — **449 unit & integration tests passing across 95 suites**, 0 Clippy warnings (`-D warnings`), **86.42%** line coverage across all 5 core DSP/GPU crates on bare-metal Apple Silicon Metal (**82.72%** deterministic 4-crate CPU coverage enforced at `>= 82%` in GitHub Actions CI).

## Installation

| File | Description |
|------|-------------|
| `Rraw-v0.2.0.dmg` | macOS installer (Apple Silicon) |
| `Rraw-v0.2.0.dmg.sha256` | SHA-256 checksum for integrity verification |

**Requirements:** macOS 14 (Sonoma) or 15 (Sequoia) on Apple Silicon.

```bash
# Verify download integrity
shasum -a 256 -c Rraw-v0.2.0.dmg.sha256
```

## Upgrade Notes

No migration steps required. Existing `.rraw.json` sidecar recipes, user layout preferences (`layout.json`), and grading presets are 100% backward compatible.
