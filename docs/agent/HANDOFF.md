# HANDOFF.md — Next Action & Session Continuity

## Last Completed Milestone

- **Phase 6.1b.1 — Tone Curve Ergonomics, Sqrt Histogram Underlay & In/Out Readout (`4b0c724`)**:
  - Implemented square-root scaled, channel-tinted live histogram underlay in tone curve canvas (`tone_curves.rs`).
  - Added master Combined RGB ghost curve displayed when editing individual color channels or luma.
  - Implemented live In/Out percentage coordinate readout and vertical hairline cursor guide.
  - Added dynamic activity dot indicator on modified curve channel selector tabs.
  - Added unit test verifying curve channel isolation and activity detection.
  - 96 unit/integration tests passing workspace-wide.

- **Phase 6.1 — Interactive PCHIP Tone Curves & Mathematical Hardening (`64921a7`, `d75eafc`, `00ab991`, `99bd8a7`)**:
  - Implemented Fritsch-Carlson Monotone Cubic Hermite Spline (PCHIP) evaluator and 1024-entry LUT evaluation in `rraw-color` and `rraw-core`.
  - Hardened PCHIP derivative formulation using Brodlie / Fritsch-Carlson weighted harmonic mean for non-uniform knot spacing, preventing overshoot or gradient reversals on tight control points.
  - Precompiled 5-spline `PchipSpline` structure, dropping heap allocations from 12,288 to 0 in the 1D LUT evaluation loop.
  - Refactored Rayon row preview evaluation in `preview.rs` to write directly in-place via `par_chunks_exact_mut`, eliminating all `preview_h` per-row `Vec` allocations.
  - Fixed tone curve UI interaction to immediately grab newly inserted points into active drag state on click/press, allowing continuous placement without re-clicking, and added double-click point deletion.
  - Multi-threaded preview evaluation via `rayon` across CPU cores with identity-geometry fast path and `opt-level = 3` in dev profile for silky-smooth 60-120 FPS slider and curve scrubbing.
  - Added native JPEG and PNG raster loading with on-the-fly sRGB linear conversion and thumbnails.
  - Implemented instant untouched raw A/B inspection via 0 ms GPU texture swapping on photo click-and-hold or `\` key.
  - Mapped vertical mouse wheel deltas to horizontal scrolling over the filmstrip.
  - GitHub Actions CI workflow configured on `macos-13`.
  - 95 unit/integration tests passing workspace-wide.

## Next Logical Action: Phase 6.1b.2 — WGSL Stage 5 GPU Tone Curve Pipeline Integration

- **Target crates**: `rraw-gpu`, `rraw-app`
- **Scope**:
  1. Add 1D tone curve LUT uniform/storage buffer binding to WGSL Stage 5 compute shader.
  2. Evaluate tone curve directly on GPU texture in VRAM for real-time EDR display at 120 FPS.
  3. Wire GPU pipeline dispatch in `rraw-app` when GPU compute path is active.

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

---

## Future Extensions & Roadmap Backlog (Phase 6 Candidates)

Stored in project memory for future development cycles:
1. **Interactive Parametric Tone Curves (`rraw-gpu` & `rraw-app`)**:
   - Parametric splines (RGB, Luma) with control points evaluated directly in WGSL Stage 5.
   - Interactive curve widget in the right Inspector with histogram underlay.
2. **Extended Film Emulation & Creative Presets (`rraw-color`)**:
   - Extended LUT / recipe library (Classic Chrome, Portra, Tri-X 400 emulations via 3D CLUT / parametric recipes).
3. **AI Semantic Segmentation Masks (`rraw-ai` & `rraw-app`)**:
   - CoreML subject, sky, background, and portrait segmentation models feeding the local mask pipeline.
4. **Color Management & Soft Proofing (`rraw-color` & `rraw-app`)**:
   - Target print ICC profile simulation and out-of-gamut clipping visualization.
5. **Timelapse & DNG Sequence Batch Exporter (`rraw-export`)**:
   - Parallel frame rendering to video codecs (ProRes 4444 XQ / AV1 HDR) or image sequences.

