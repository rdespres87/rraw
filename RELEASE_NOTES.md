# Rraw v0.3.0

## Highlights

- **Pure-Rust JPEG XL Export** — 8-bit and 16-bit modular lossless encoding via `zune-jpegxl` (zero C/C++ bindings, `#![forbid(unsafe_code)]`), exposed in batch and timelapse export with a zero-copy constant-run RLE guard verified against macOS ImageIO.
- **Atomic Export Writes & Cancellation** — Output is written to a temporary file and atomically renamed on completion; cooperative cancellation checks before Render and Encode ensure cancelled exports never leave partial or corrupt files on disk.
- **WGSL GPU Mask Parity** — Full `temperature_shift`, `invert`, and `feather` support in `stage6_masks.wgsl` and `rraw-core::GeometricMask`, lifting CPU fallbacks in the WGPU/Metal preview backend (verified ΔE00 < 1.0 vs CPU).
- **Modularity & Explicit i18n APIs** — All source files now under 1,000 lines after splitting `app_library.rs` and `preview/render.rs`; all 9 legacy French default-label wrappers replaced with explicit `Language`-aware APIs.
- **CI Reliability & Coverage** — 462 tests across 95 suites, 0 Clippy warnings (`-D warnings`), 87.17% deterministic 4-crate CPU line coverage (gate raised to 85%), and a single warm-cache `ci-macos` job with concurrency cancellation.

## Installation

| File | Description |
|------|-------------|
| `Rraw-v0.3.0.dmg` | macOS installer (Apple Silicon) |
| `Rraw-v0.3.0.dmg.sha256` | SHA-256 checksum for integrity verification |

**Requirements:** macOS 14 (Sonoma) or 15 (Sequoia) on Apple Silicon.

```bash
# Verify download integrity
shasum -a 256 -c Rraw-v0.3.0.dmg.sha256
```

## Upgrade Notes

No migration steps required. Existing `.rraw.json` sidecar recipes, user layout preferences (`layout.json`), and grading presets are 100% backward compatible. The new `JXL` export format is additive; all previously supported output formats remain unchanged. Mask definitions serialized before v0.3.0 deserialize correctly via `#[serde(default)]` on the new `temperature_shift`, `invert`, and `feather` fields.
