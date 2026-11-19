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
- [ ] **Task 8.1 (`rraw-optics` & `rraw-app`)**: Chromatic Aberration Auto-Detection & Axial/Lateral Defringe (Purple and green fringing suppression with edge-threshold and hue span controls)
- [ ] **Task 8.2 (`rraw-optics` & `rraw-app`)**: Lens Profile Matching & Metadata Linking (Exif LensModel/FocalLength matching with Lensfun / DNG OpcodeList3 formulas and strength sliders)
- [ ] **Task 8.3 (`rraw-app`)**: Enhanced Optical Inspector UI in `DarkroomSection::OpticalCorrections` (Auto-Profile vs Manual mode, interactive defringe preview, sidecar persistence and reset)

---

## Phase 9 — Local Masking & Selective Grading Engine (Gradients, Radial & Brush Strokes)
- [ ] **Task 9.1 (`rraw-core` & `rraw-app`)**: Local Masking DAG & Data Model (`LinearGradientMask`, `RadialGradientMask`, `BrushStrokeMask` with local EV, Contrast, Temperature, Saturation, Sharpness)
- [ ] **Task 9.2 (`rraw-app` & `rraw-gpu`)**: Interactive Viewport Overlay & Canvas Tooling (Direct mouse drag for linear/radial gradients with rotation & feather handles, brush stroke painting with radius/hardness)
- [ ] **Task 9.3 (`rraw-app`)**: Local Mask Layer Stack Inspector & Pipeline Compositor (Layer list, enable/disable, invert, opacity, blend with CoreML AI semantic masks, multi-scale export)

