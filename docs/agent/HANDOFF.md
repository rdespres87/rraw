# HANDOFF.md — Next Action & Session Continuity

## Last Completed Milestone

- **Phase 6.5 — Timelapse, Keyframe Ramping & Temporal Deflicker Engine (`rraw-export`, `rraw-app`)**:
  - Implemented `TimelapseFrameRate` (`Fps24`, `Fps25`, `Fps30`, `Fps60`), `TimelapseKeyframe`, `TimelapseDeflickerConfig`, `compute_frame_log_luminance`, `compute_deflicker_ev_offsets` (weighted local linear regression / Savitzky-Golay degree-1 temporal trend), `interpolate_keyframes`, `format_sequence_filename`, and `write_timelapse_manifest` in `crates/rraw-export/src/timelapse.rs`.
  - Integrated `ExportTargetMode::TimelapseSequence`, `prepare_timelapse_frame_gradings`, top-bar `🎬 Timelapse...` quick action, and full timelapse sequence UI controls (FPS selector, duration readout, sliding-window temporal deflicker, Holy Grail day-to-night keyframe ramping, 5-digit padded sequence export, and `timelapse_sequence.json` manifest) in `rraw-app`.
  - Verified with 126 unit and integration tests passing across 39 test suites and 0 clippy warnings.

- **Phase 6.4 — Color Management & Soft Proofing (`rraw-color`, `rraw-app`)**:
  - Implemented `SoftProofProfile` (`SrgbIec61966`, `DisplayP3`, `AdobeRgb1998`, `FineArtMattePaper`, `GlossyBarytaPaper`), `SoftProofRenderingIntent` (`Perceptual`, `RelativeColorimetric`), `SoftProofConfig`, `SoftProofResult`, `is_out_of_gamut`, and `evaluate_soft_proof` in `crates/rraw-color/src/soft_proof.rs`.
  - Integrated `DarkroomSection::SoftProofing` (`🖨️ Épreuvage écran & Hors-Gamut (ICC)`) in `rraw-app` right inspector (`inspector.rs`), quick top-bar toggles (`🖨️ Épreuvage` & `⚠️ Hors-Gamut`), live out-of-gamut pixel counter in `HistogramData` and `histogram.rs`, high-visibility magenta/cyan zebra warning overlay in `preview.rs`, and `.rraw.json` sidecar persistence in `state.rs`.
  - Verified with 122 unit and integration tests passing across 38 test suites and 0 clippy warnings.

- **Phase 6.3 — CoreML AI Semantic Segmentation Masks (`rraw-core`, `rraw-ai`, `rraw-gpu`, `rraw-app`)**:
  - Added `SemanticMaskCategory` (`Subject`, `Sky`, `Background`, `Portrait`) and `MaskKind::SemanticAi` to `rraw-core`.
  - Implemented `SemanticMaskSet`, `SemanticSegmenter`, and `CoreMlSemanticSegmenter` in `rraw-ai` (`segmentation.rs`) with edge-preserving 3x3 bilateral luminance guidance, partition-of-unity invariant (`subject + background == 1.0`), bilinear feather sampling, and 4-channel RGBA interleaved GPU packing.
  - Bound 4-channel semantic probability buffer (`@group(0) @binding(4) var<storage, read> semantic_rgba: array<vec4<f32>>`) to Stage 6 compute shader (`stage6_masks.wgsl`) with bilinear interpolation, smoothstep feathering, and mask inversion via `Stage6Pipeline::execute_with_semantics`.
  - Integrated `DarkroomSection::AiSemanticMasks` (`🎭 Masques IA (CoreML)`) in `rraw-app` right inspector (`inspector.rs`), `SemanticMaskLayer` in `state.rs` with `.rraw.json` sidecar persistence, live ruby-magenta mask overlay visualization, and local Exposure, Contrast, Saturation, and Temperature grading in `preview.rs`.
  - Verified with 115 unit and integration tests passing across 37 test suites and 0 clippy warnings.

- **Phase 6.2b — GPU WGSL 3D CLUT & User `.cube` Import (`bd55977`)**:
  - Bound 3D CLUT texture (`texture_3d<f32>`) and filtering sampler to Stage 5 compute shader (`stage5_color_laplacian.wgsl`) with half-texel boundary alignment for 120 FPS hardware trilinear interpolation.
  - Implemented `f32_to_f16` IEEE 754 half-float encoder and dynamic `Rgba16Float` 3D texture creation with a default 2x2x2 identity fallback texture.
  - Integrated custom `.cube` file import via native file dialog (`rfd`) in `rraw-app` Left Panel (`presets.rs`), with active LUT badge (`🎞 <name> (N³)`), intensity slider (`0% .. 100%`), and quick reset button (`✖`).
  - Added full sidecar persistence in `SidecarRecipe` (`custom_lut_path` and `custom_lut_strength`).
  - Added CPU preview integration in `preview.rs` (tetrahedral interpolation with strength blending).
  - Verified with 106 unit and integration tests passing cleanly across 36 suites, 0 clippy warnings, and local adversarial audit.

- **Phase 6.2 — Extended Film Emulation & Creative Presets (`a607876`)**:
  - Implemented 3D CLUT (`ColorLut3d`) in `rraw-color` (`crates/rraw-color/src/clut.rs`) with zero-allocation tetrahedral and trilinear interpolation.
  - Implemented `.cube` file parser (`from_cube_str`) and exporter (`to_cube_str`) adhering to Adobe / DaVinci Resolve specification.
  - Built algorithmic spectral film profile generators (`FilmSimulationProfile`): *Fujifilm Classic Chrome*, *Kodak Portra 400*, *Fujifilm Velvia 50*, *Kodak Tri-X 400*, and *Ilford HP5 Plus*.
  - Added new film simulation presets to `GradingPreset` in `rraw-app`:
    - Full parameter configurations and custom PCHIP tone curves for each film profile.
    - Preserved identity reset on preset switch, round-trip ID string serialization, and bidirectional `detect_preset` recognition.
  - Added adversarial tests across `rraw-color` and `rraw-app`, verifying numerical exactness, neutral axis preservation, finite bounds, and preset activation.
  - Verified with 102 unit/integration tests passing workspace-wide and zero clippy warnings.
  - Verified by adversarial local audit (Qwen 3.6 35B via `execute_local_task`, 0 cloud tokens).

- **Phase 6.1b.2 — WGSL Stage 5 GPU Tone Curve Pipeline Integration (`e084390`)**:
  - Bound 1024-entry 1D tone curves LUT storage buffer (`array<vec4<f32>>`) to Stage 5 compute shader (`stage5_color_laplacian.wgsl`).
  - Implemented WGSL `sample_lut_channel` with linear interpolation and strictly guarded array index bounds (`min(u32(floor(x)), 1022u)`).
  - Ported ACEScg AP1 luma preservation and Gamut Guard into WGSL Stage 5b.
  - Extended `Stage5Params` with `tone_curves_lut: Option<Box<[[f32; 4]; 1024]>>` and zero-cost allocation path.
  - Verified numerical equivalence with CPU reference via `test_stage5_gpu_tone_curves_matches_cpu_reference` (RMSE < 1e-4).

- **Phase 6.1b.1 — Tone Curve Ergonomics, Sqrt Histogram Underlay & In/Out Readout (`4b0c724`)**:
  - Implemented square-root scaled, channel-tinted live histogram underlay in tone curve canvas (`tone_curves.rs`).
  - Added master Combined RGB ghost curve displayed when editing individual color channels or luma.
  - Implemented live In/Out percentage coordinate readout and vertical hairline cursor guide.
  - Added dynamic activity dot indicator on modified curve channel selector tabs.

## Next Logical Action: Task 8.1 — Chromatic Aberration Auto-Detection & Axial/Lateral Defringe (`rraw-optics` & `rraw-app`)

- **Objective**: Implement optical defringing to detect and suppress purple and green fringing along high-contrast backlit edges (branches, windows) with customizable edge threshold and hue span controls.
- **Deliverables**:
  1. Optical CA fringe detection kernel analyzing radial edge gradients and chromatic overshoot.
  2. Sub-pixel lateral chromatic aberration color channel re-alignment.
  3. Interactive sliders in `DarkroomSection::OpticalCorrections` with live 120 FPS feedback and sidecar persistence.
  4. Unit and integration tests verifying chromatic fringe attenuation on synthetic high-contrast step edges.

## Repository Completion State

All planned deliverables across Phases 0–6 and Phase 7 (Tasks 7.1, 7.2, 7.3) are implemented, verified by adversarial TDD, and passing all pre-commit formatting and clippy gates. Phase 8 is the active development horizon.

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

