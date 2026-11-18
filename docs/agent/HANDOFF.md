# HANDOFF.md — Next Action & Session Continuity

## Last Completed Milestone

- **Phase 5 — Task 5.1 (`rraw-app`) Implemented**:
  - Implemented `eframe` / `egui` native macOS window shell with `RrawApp`.
  - Added live `NSScreen` EDR headroom detection badge, top control bar with native macOS file dialog (`rfd`) for RAW opening and image export.
  - Added comprehensive right Darkroom inspector panel (Exposure EV, Kelvin WB, Tint, Display Transform combo, Wavelet denoise, Richardson-Lucy capture sharpening, and optical toggles).
  - Maintained `--summary` and `--headless` flags for instant CI/script execution without opening a native window.
  - Maintained strict `#![forbid(unsafe_code)]` on `rraw-app` and zero unwrap / expect across the workspace.
  - Workspace test suite: 83 unit/integration tests passing cleanly.

## Repository Completion State

All planned deliverables across all six development phases (Phases 0–5) are implemented, verified by adversarial TDD, and passing all pre-commit formatting and clippy gates.

### Summary of Completed Workspace Milestones
1. **Phase 0**: Repository governance, `#![forbid(unsafe_code)]` / audited EDR isolation, 11 crates.
2. **Phase 1**:
   - `Task 1.1`: Versioned `.rraw.json` recipe DAG and stage-scoped cache hashing (`acedebb`).
   - `Task 1.2`: Pure-Rust `RawDecoder`, DNG OpcodeList1..3 & GainMap, wavelet MAD noise estimator (`8fa0a80`).
   - `Task 1.3`: CCT solver, Bradford D60 to ACEScg, Gamut Guard, AgX DRT (`bed1a7f`).
   - `Task 1.4`: Shared CPU reference pipeline with unclamped WB and highlight reconstruction (`486aff9`).
3. **Phase 2**:
   - `Task 2.1`: Headless Metal `wgpu`, Stage 1 signed normalization, Stage 2 Hamilton-Adams demosaic (`4eb20a4`).
   - `Task 2.2`: Byte-budgeted LRU VRAM cache, 120 Hz RoI scrubbing, Stage 5 Laplacian, Stage 7 scopes (`8e922ad`).
   - `Task 2.3`: Safe `MacOsDisplayHeadroomProvider` over `objc2`, dual-pass compositor (`7446b08`).
4. **Phase 3**:
   - `Task 3.1`: 4-tier optical resolver, 256x256 UV LUTs, Stage 4 Catmull-Rom WARP shader (`71635cc`).
   - `Task 3.2`: Regularized Richardson-Lucy capture sharpening, radial PSF, deringing envelope (`cd5474f`).
5. **Phase 4**:
   - `Task 4.1`: Phase-preserving 504x504 tiling, Anscombe GAT conditioning, epoch cancellation (`28597cd`).
   - `Task 4.2`: Stage 6 post-warp geometric & ACEScg parametric masks at 120 Hz (`e836632`).
   - `Task 4.3`: SQLite DAM index, non-destructive XMP editor preserving third-party nodes (`c4d4abb`).
   - `Task 4.4`: Multi-threaded batch exporter and ISO 21496-1 HDR Gain Maps (`a3ee723`).
6. **Phase 5**:
   - `Task 5.1`: Native macOS interactive window shell (`eframe` / `egui` + `rfd`) with live EDR headroom, strictly ordered Left Panel (1. `📁 Explorateur de dossiers` with Desktop default, breadcrumbs, and `..` parent navigation; 2. `🎨 Styles & Préréglages` with dropdown combobox and quick reset; 3. `📋 Données EXIF` with full striped summary, complete rawler tag extraction (serial numbers, lenses, color space, exposure mode, scene capture, light source, dates, offsets, GPS, CFA/sensor levels), real-time search filter, and JSON copy/export; 4. `📊 Histogramme Waveform RGB` at the bottom); clean right Inspector; toggles for crop and geometry; 120 FPS mouse-driven vector cropping; bottom filmstrip with scrollbar completely cleared below cards without covering stars; classical keyboard and mouse multi-selection (Click, Cmd/Ctrl+Click, Shift+Click range, Cmd+A, Cmd+D, Esc, Space, Shift+Arrow); instantaneous photo switching with frame-0 thumbnail display, 1600px preview bounding, 30-photo MRU cache, and ±2 adjacent prefetching. All 90 tests passing cleanly.
