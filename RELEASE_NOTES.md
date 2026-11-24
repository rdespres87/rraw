# Rraw v0.1.2

## Highlights

- **i18n Integrity & Multilingual Scalability** — Eliminated all 57 hardcoded language checks; unified 620 dynamic JSON translation keys across `en`/`fr`/`es`.
- **Performance & Virtualization** — Virtualized horizontal filmstrip (`ScrollArea::show_viewport`) supporting 10,000+ photos with constant-time layout, plus a 1600 px interactive preview proxy guarding full-res demosaicing.
- **UI & Viewport UX Fixes** — Deferred preview rebuild during crop-box drag for smooth 120 Hz interaction; fixed horizontal overflow in the 360 px right-inspector Masks panel.
- **Architecture & Maintainability** — Decomposed monolithic UI/decoder functions, introduced typed parameter objects (zero `clippy::too_many_arguments`), and de-bloated `PreparedGradingFrame`, `ExportBatchController`, and `DarkroomGradingState`.
- **Colorimetric Regression & CI Coverage** — Added headless WGPU 24-patch ColorChecker visual regression suite (ΔE₀₀ < 0.5 across Stages 1–7, SDR & EDR) and a `cargo-llvm-cov` CI gate (≥ 85 % line coverage on core DSP/GPU crates; currently 86.33 %, 417 tests passing).

## Installation

| File | Description |
|------|-------------|
| `Rraw-v0.1.2.dmg` | macOS installer (Apple Silicon) |
| `Rraw-v0.1.2.dmg.sha256` | SHA-256 checksum for integrity verification |

**Requirements:** macOS 14 (Sonoma) or 15 (Sequoia) on Apple Silicon.

```bash
# Verify download integrity
shasum -a 256 -c Rraw-v0.1.2.dmg.sha256
```

## Upgrade Notes

No migration steps required. Existing user profiles and grading presets are fully compatible.
