# TASKS.md — Phase & Task Progress Tracker

Legend: `[ ]` Not started | `[/]` In progress | `[x]` Done & verified (post-Gate 3)

## Phase 0 — Governance, Documentation & 11-Crate Workspace Initialization
- [x] **Task 0.1**: Repository governance (`AGENTS.md`, `README.md`, `LICENSE`, `docs/studies/ARCHITECTURE.md`, 10 files in `docs/agent/`, `.gitignore`, `rust-toolchain.toml`, `.git/hooks/pre-commit`)
- [x] **Task 0.2**: Root `Cargo.toml` workspace lints + 11 compilable crates (`10` `#![forbid(unsafe_code)]`, `rraw-macos-edr` `#![deny(unsafe_code)]`) with contract/smoke unit tests

## Phase 1 — Mathematical Foundations, Pure-Rust Decoding, Colorimetry & CLI
- [x] **Task 1.1 (`rraw-core`)**: Versioned `.rraw.json` recipe DAG, canonical serialization, stage cache hashing (`L0`/`L1`/`L2`/display), `NoiseProfile`, `PhotoMetadata`, `CfaPattern`
- [x] **Task 1.2 (`rraw-decode`)**: `RawDecoder` (`rawler` + synthetic RAW), `NoiseProfile` (`0xc761` + MAD fallback), binary DNG `OpcodeList1..3` & `GainMap` (`vignette_handled_by_gainmap`)
- [x] **Task 1.3 (`rraw-color`)**: Iterative CCT solver, 1D mired interpolation, `ForwardMatrix1/2` & `ColorMatrix1/2` -> Bradford -> `ACEScg`, identity-core Gamut Guard, `AgX`/`OpenDRT`
- [x] **Task 1.4 (`rraw-cli`)**: Shared CPU reference pipeline (signed norm -> unclamped WB `sat_c = w_c` -> Bayer demosaic -> HLR -> colorimetry -> 16-bit PNG)

## Phase 2 — Headless `wgpu` GPU Pipeline, LRU DAG Cache & 120 Hz EDR Viewport
- [x] **Task 2.1 (`rraw-gpu`)**: `GpuContext::new_headless()`, Stage 1 WGSL (signed `R32Float`, defect repair, Anscombe À-Trous wavelets), Stage 2 WGSL (Hamilton-Adams directional Bayer demosaic + HLR)
- [x] **Task 2.2 (`rraw-gpu`)**: VRAM-capped LRU DAG cache (`L0`/`L1`/`L2`), 120 Hz RoI + `32px` halo scrubbing, mipmap pyramid ($L \in \{0, 1, 2\}$), Stage 5–7 WGSL (`ACEScg`, $N_{\text{octaves}} - L$ Laplacian, `AgX`/`OpenDRT`, GPU scopes)
- [x] **Task 2.3 (`rraw-macos-edr` & `rraw-app`)**: Audited `objc2` `DisplayHeadroomProvider`, permanently fixed `Rgba16Float` surface, dual-pass SDR UI + EDR viewport compositor

## Phase 3 — 4-Tier Optical Engine & Regularized Capture Sharpening
- [x] **Task 3.1 (`rraw-optics` & `rraw-gpu`)**: 4-tier optical resolver (MakerNotes, `Lensfun` XML, `.lcp`, manual), `256×256` UV LUTs, Catmull-Rom `WARP` shader with fused vignette (`!vignette_handled_by_gainmap`) + `>= 0.0` clamp
- [x] **Task 3.2 (`rraw-optics` & `rraw-gpu`)**: Damped Richardson-Lucy Capture Sharpening ($\epsilon = 10^{-6}$, `NoiseProfile` edge mask, radial $\text{PSF}_r$, local overshoot deringing)

## Phase 4 — CoreML AI (`504×504` Tiling), Local Masks, DAM & HDR Export
- [x] **Task 4.1 (`rraw-ai`)**: Phase-preserving `504×504` (`48px` $\cos^2$ overlap) tile denoiser, FP16 Anscombe/log conditioning, `(PhotoId, Stage0Epoch)` cancellation, `ort` CoreML EP
- [x] **Task 4.2 (`rraw-gpu` & `rraw-app`)**: Post-warp `[0, 1]²` geometric masks and `ACEScg` parametric Luminance/Chroma/Hue masks
- [x] **Task 4.3 (`rraw-storage` & `rraw-app`)**: Async `rusqlite` DAM index, namespace-preserving `.xmp` sidecars, disk thumbnail mipmap cache, browser/filmstrip UI
- [x] **Task 4.4 (`rraw-export`)**: Multi-threaded batch exporter (16-bit TIFF/PNG, JPEG, AVIF, JXL) + ISO 21496-1 HDR Gain Maps (binary `tmap` + XMP `hdrgm:*`)

## Phase 5 — Native macOS Interactive GUI Shell
- [x] **Task 5.1 (`rraw-app`)**: `eframe` / `egui` native macOS window shell with live EDR headroom query, Darkroom grading sliders (Exposure, WB, Contrast, Denoise, Optics), RAW file selector (`rfd`), and `--summary` headless mode fallback

---

## Phase 6 — Extended Color, Presets, AI Masks & Timelapse Engine (Complete)
- [x] **Task 6.1 (`rraw-gpu` & `rraw-app`)**: Interactive Parametric Tone Curves (PCHIP splines evaluated in WGSL Stage 5 with live sqrt histogram underlay & in/out readout, commit `64921a7`, `e084390`, `4b0c724`)
- [x] **Task 6.2 (`rraw-color` & `rraw-app`)**: Extended Film Emulation & 3D CLUT / Creative Presets library (tetrahedral & trilinear CLUT, `.cube` parser/exporter, film profiles & grading presets, commit `a607876`)
- [x] **Task 6.2b (`rraw-gpu` & `rraw-app`)**: GPU WGSL 3D CLUT & User `.cube` Import (hardware trilinear `texture_3d<f32>` in Stage 5 at 120 FPS, UI `.cube` file import, intensity slider, sidecar persistence)
- [x] **Task 6.3 (`rraw-core`, `rraw-ai`, `rraw-gpu` & `rraw-app`)**: CoreML AI Semantic Segmentation Masks (`Subject`, `Sky`, `Background`, `Portrait` with edge-aware guided refinement, WGSL Stage 6 bilinear evaluation, live viewport overlay & local grading)
- [x] **Task 6.4 (`rraw-color` & `rraw-app`)**: Color Management & Soft Proofing (ICC display/printer profiles, Perceptual & Relative Colorimetric intents, paper white & black ink simulation, out-of-gamut zebra alerts)
- [x] **Task 6.5 (`rraw-export` & `rraw-app`)**: Timelapse & High-Bitrate Video Exporter (Temporal RAW log2 luminance deflicker, Holy Grail keyframe ramping, 5-digit sequence assembly & ProRes/HEVC manifest)

---

## Phase 7 — Advanced Demosaicing & Noise Reduction Engine (Edge-Aware / Guided Wavelet / Luma & Chroma Denoise)
- [x] **Task 7.1 (`rraw-core` & `rraw-app`)**: Edge-Aware Adaptive Demosaicing (Directional green gradient interpolation / Hamilton-Adams & RCD color difference filter eliminating zippering & false color on high-contrast edges)
- [x] **Task 7.2 (`rraw-core` & `rraw-app`)**: Guided Bilateral & Wavelet Chrominance Denoise (Luminance-guided chrominance bilateral filtering eliminating color speckles/mottle in shadows and high ISO without edge blurring)
- [x] **Task 7.3 (`rraw-core` & `rraw-app`)**: Luminance Denoise & Viewport Before/After Ergonomics (Edge-preserving luma grain filter, active sliders in `DarkroomSection::DetailDenoise`, right-click/shortcut key original comparison in 1:1 zoom, sidecar persistence and export integration)

---

## Phase 8 — Complete Optical Correction Suite (Distortion, CA Defringe, Vignetting & Profile Matching)
- [x] **Task 8.1 (`rraw-optics` & `rraw-app`)**: Chromatic Aberration Auto-Detection & Axial/Lateral Defringe (Purple and green fringing suppression with edge-threshold and hue span controls, creative post-crop vignetting, commit `0ac3a54` / `165ba90`)
- [x] **Task 8.2 (`rraw-optics` & `rraw-app`)**: Lens Profile Matching & Metadata Linking (Exif LensModel/FocalLength matching with Lensfun / DNG OpcodeList3 formulas, static catalog, continuous focal/aperture interpolation, auto profile correction toggle, and strength sliders)
- [x] **Task 8.3 (`rraw-app`)**: Enhanced Optical Inspector UI in `DarkroomSection::OpticalCorrections` (Auto-Profile vs Manual mode, interactive defringe preview, sidecar persistence and reset)

---

## Phase 9 — Local Masking & Selective Grading Engine (Gradients, Radial & Brush Strokes)
- [x] **Task 9.1 (`rraw-core` & `rraw-app`)**: Local Masking DAG & Data Model (`LinearGradientMask`, `RadialGradientMask` with local EV, Contrast, Temperature, Saturation, continuous analytical falloff, ruby overlay, sidecar persistence)
- [x] **Task 9.2 (`rraw-app` & `rraw-gpu`)**: Interactive Freehand Brush Masking & Viewport Tooling (Vector dabs, flow, additive/subtractive erase, radius slider down to 0.001, bracket step scaling, scroll wheel zoom, Spacebar/middle-click pan, double-click collision fix)
- [x] **Task 9.3 (`rraw-app`)**: Local Mask Layer Stack Inspector & Pipeline Compositor (Layer list, enable/disable, invert, opacity, blend with CoreML AI semantic masks, multi-scale export)


---

## Phase 10 — Professional UI Overhaul (Sidebar, Viewport HUD & Comparison)
- [x] **Task 10.1 (`rraw-app`)**: Step 1 — Fixed-width side panels (320/360 px), category auto-unrolling, multi-line control stacking, glyph fallback cleanup, layout persistence (`layout.json`), French terminology harmonization
- [x] **Task 10.2 (`rraw-app`)**: Step 2 — Top bar decluttering (EXIF + rating moved out), translucent viewport HUD (focal · aperture · shutter · ISO + rating), Before/After comparison (hold `D` + draggable split divider with edge clamping and conflict-free clicks), shadow/highlight clipping overlays (red blown highlights, blue crushed shadows), collapsible filmstrip (`F9`), keyboard shortcuts modal guide (`F1`)

---

## Phase 11 — Internationalization (i18n)
- [x] **Task 11.1 (`rraw-app`)**: Typed, zero-allocation string tables (`Language { Fr, En }` enum + `&'static str` lookup per key, compile-time exhaustiveness via `match`), module `crates/rraw-app/src/i18n/`
- [x] **Task 11.2 (`rraw-app`)**: Migrate all hard-coded UI labels (top bar, inspector, filmstrip, EXIF, viewport prompts, histogram) to i18n keys; unit test asserting every key resolves to a non-empty string in every language
- [x] **Task 11.3 (`rraw-app`)**: Compact `FR | EN` toggle in the top bar, persisted in `layout.json` (backward-compatible `#[serde(default)]`), live switch without restart

---

## Phase 12 — Batch Recipe Clipboard & Multi-Image Development (Copy/Paste Settings)
- [x] **Task 12.1 (`rraw-core` & `rraw-app`)**: Color & Tone Recipe Clipboard Model (Snapshot core colorimetric & tonal grading parameters: Exposure, Contrast, White Balance CCT/Tint, Tone Curves PCHIP, Saturation, Film Emulations/Styles; strictly excluding photo-dependent settings like Crop, Straighten/Geometry, Local Brush/Gradient Masks, Lens Optics, and ISO Denoise)
- [x] **Task 12.2 (`rraw-app`)**: Filmstrip Multi-Selection Batch Paste & Keyboard Shortcuts (`Cmd+Shift+C` / `Cmd+Shift+V`, contextual right-click menu on filmstrip thumbnails "Copier le rendu / Coller le rendu", batch application to multi-selected photos with immediate thumbnail re-render, sidecar auto-save and bottom status bar notification)


---

## Phase 13 — Application Versioning, Packaging & macOS DMG Installer
- [x] **Task 13.1 (`rraw-app` & Workspace)**: Semantic Versioning & Build Metadata (Semver `VERSION`, git commit hash and build date exposed via compile-time constants, displayed in top bar, "À propos" modal dialog and CLI `--version`)
- [x] **Task 13.2 (Packaging & Distribution)**: macOS App Bundle (`Rraw.app`) & Drag-and-Drop DMG Disk Image Installer (`scripts/package_macos.sh`, `Info.plist`, HiDPI `.icns` application icon, `/Applications` symlink, background art and code signing/notarization preparation)

---

## Phase 14 — Smart Lighting & Adaptive Dynamic Range (Scene-Aware Local Tone Mapping)
- [x] **Task 14.1 (`rraw-core` & `rraw-color`)**: Scene Dynamic Range Analyzer & Local Tone Mapping Core (Automatic histogram/key analysis for backlighting, under/over-exposure; edge-preserving multi-scale base/detail decomposition via guided filter or Laplacian pyramid; halo-free shadow lift & highlight compression in linear ACEScg)
- [x] **Task 14.2 (`rraw-gpu` & `rraw-app`)**: Pipeline & GPU/CPU Integration (Real-time WGSL / CPU preview execution, scale-invariant downsample factor handling $N_{\text{effective}} = N_{\text{octaves}} - L$, chromaticity preservation avoiding desaturation or hue shifts in recovered shadows/highlights)
- [x] **Task 14.3 (`rraw-app`)**: Smart Lighting UI Inspector & Preset Integration (Controls in `DarkroomSection::ExposureTone`: Modes `Off` / `Slight` / `Medium` / `Strong` / `Custom`, intensity slider, sidecar `.rraw.json` persistence, batch clipboard copy/paste in Phase 12 model, Fr/En i18n keys)

---

## Phase 15 — Architectural Hardening: State De-Obesity, UI Modularization & Scalable i18n
- [x] **Task 15.1 (`rraw-core` & `rraw-app`)**: State De-Obesity & Domain Hierarchy
  - Decompose monolithic flat `DarkroomGradingState` (57 fields) into cohesive, composable domain sub-structs:
    - `ToneGradingParams` (exposure, contrast, highlights, shadows, blacks, saturation, vibrance)
    - `ColorGradingParams` (kelvin, tint, display_transform, is_monochrome)
    - `DenoiseSharpenParams` (wavelet_sigma, color_denoise, luma_denoise, capture_sharpening)
    - `OpticsGradingParams` (tab, auto_profile, strengths, manual distortion, CA, defringe, vignetting)
    - `GeometryGradingParams` (crop coordinates/aspect, rotation, flips, keystone, auto_crop)
  - Align `SidecarRecipe` using Serde `#[serde(flatten)]` to ensure 100% strict backward and forward compatibility with existing `.rraw.json` sidecar files without field name collisions.
  - TDD test suite `state_de_obesity_tests.rs` passing, 247 workspace tests passing, 0 clippy warnings, Qwen 3.6 35B Inquisitor APPROVED.

- [ ] **Task 15.2 (`rraw-app`)**: Monolithic File & Function Decomposition
  - [x] Step 1: Decompose `crates/rraw-app/src/ui/inspector.rs` (2,087 lines down to 420 lines) & `render_darkroom_section` (1,773 lines) into isolated modular submodules under `crates/rraw-app/src/ui/sections/`:
    - `sections/exposure_tone.rs`, `sections/white_balance.rs`, `sections/optics.rs`, `sections/detail_denoise.rs`, `sections/geometry.rs`, `sections/crop.rs`, `sections/straighten.rs`, `sections/masks.rs`, `sections/soft_proofing.rs`, `sections/timelapse.rs`, `sections/mod.rs`.
    - Enforce the principle of least privilege: each section widget accepts only its specific domain parameter struct (e.g. `&mut ToneGradingParams`) rather than `&mut RrawApp` or full grading state.
    - Encapsulate perspective tool state in `ParallelLinesToolState` eliminating `clippy::too_many_arguments`.
    - Dedicated test suite `crates/rraw-app/tests/section_modularization_tests.rs` (10 tests) passing, Inquisitor APPROVED.
  - [ ] Step 2: Decompose `crates/rraw-app/src/ui/viewport.rs` (1,231 lines) & `render_central_viewport` (1,134 lines) into modular subcomponents:
    - `viewport/canvas.rs`, `viewport/hud.rs`, `viewport/split_compare.rs`, `viewport/overlay_masks.rs`, `viewport/navigation.rs`.
  - [ ] Step 3: Decompose `crates/rraw-app/src/preview.rs` (2,264 lines) into dedicated processing layers:
    - `preview/pipeline.rs` (rendering execution), `preview/cache.rs` (MRU & texture pooling), `preview/masks.rs` (local adjustments evaluation).

- [ ] **Task 15.3 (`rraw-app`)**: Scalable External Resource-Based i18n Engine
  - Replace the 1,380-line hardcoded `match (self, lang)` table in `crates/rraw-app/src/i18n/mod.rs` with externalized translation catalogs (Fluent `.ftl` or embedded structured TOML/JSON resources under `assets/i18n/`).
  - Implement parameterized interpolation support (`{count}`, `{photo}`, `{zoom}`) and formal pluralization rules, eliminating manual ad-hoc string formatting in UI views.
  - Enable zero-code language addition: adding a new language (e.g. Spanish `es.ftl`, German `de.ftl`, Japanese `ja.ftl`) only requires dropping the translation file into `assets/i18n/` without modifying or recompiling core UI dispatch logic.
  - Preserve zero-allocation runtime lookup for static labels and instant live-switch without application restart.



