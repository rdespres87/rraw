# Rraw v0.4.0

## Highlights

- **Pure-Rust Expanded RAW & Legacy Sensor Decoding** — Added pure-Rust decoders for GoPro `.gpr` (VC-5 / SMPTE ST 2073-1 wavelet), Sigma Foveon `.x3f`, Canon PowerShot Pro70 4-color CYGM CIFF, 12-bit JPEG/DNG fallback, monochrome sensors, linear `cpp=3` RGB TIFF/DNG, Fujifilm SuperCCD 45-degree rotation, and headerless naked RAW / CinemaDNG / YUV fallbacks with camera alias resolution.
- **Color Science & Lighting Refinements** — Added D65 white-balance derivation, recalibrated Slight smart-lighting mode, improved highlight clipping, and `f32` sensor normalization with strict CFA guards.
- **Performance & Modularity** — Cached lens profile override lookups (`LENS_OVERRIDE_CACHE`) and UI lens search results (`cached_lens_search`) to eliminate per-frame clones and searches; decomposed `ViewportController`, `dng_fallback`, `special_sensors`, `optics::render`, `export_runner`, and preview pipeline helpers.
- **Fuzzing & Quality Baseline** — Added 5 `cargo-fuzz` targets (`fuzz_dng_fallback`, `fuzz_jpeg12`, `fuzz_opcodes`, `fuzz_vc5`, `fuzz_x3f`) for pure-Rust binary parsers; **586 tests passing across 99 suites**, 0 Clippy warnings (`-D warnings`), 0 `unwrap`/`expect`/`panic!` in production code, and **85.00%** 4-crate CPU line coverage.

## Installation

| File | Description |
|------|-------------|
| `Rraw-v0.4.0.dmg` | macOS installer (Apple Silicon) |
| `Rraw-v0.4.0.dmg.sha256` | SHA-256 checksum for integrity verification |

**Requirements:** macOS 14 (Sonoma) or 15 (Sequoia) on Apple Silicon.

```bash
# Verify download integrity
shasum -a 256 -c Rraw-v0.4.0.dmg.sha256
```

## Upgrade Notes

No migration steps required. All `.rraw.json` sidecar recipes, SQLite cache databases, layout preferences, and grading presets remain fully backward-compatible.
