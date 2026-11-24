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

- [x] **Task 15.2 (`rraw-app`)**: Monolithic File & Function Decomposition
  - [x] Step 1: Decompose `crates/rraw-app/src/ui/inspector.rs` (2,087 lines down to 420 lines) & `render_darkroom_section` (1,773 lines) into isolated modular submodules under `crates/rraw-app/src/ui/sections/`:
    - `sections/exposure_tone.rs`, `sections/white_balance.rs`, `sections/optics.rs`, `sections/detail_denoise.rs`, `sections/geometry.rs`, `sections/crop.rs`, `sections/straighten.rs`, `sections/masks.rs`, `sections/soft_proofing.rs`, `sections/timelapse.rs`, `sections/mod.rs`.
    - Enforce the principle of least privilege: each section widget accepts only its specific domain parameter struct (e.g. `&mut ToneGradingParams`) rather than `&mut RrawApp` or full grading state.
    - Encapsulate perspective tool state in `ParallelLinesToolState` eliminating `clippy::too_many_arguments`.
    - Dedicated test suite `crates/rraw-app/tests/section_modularization_tests.rs` (10 tests) passing, Inquisitor APPROVED.
  - [x] Step 2: Decompose `crates/rraw-app/src/ui/viewport.rs` (1,231 lines) & `render_central_viewport` (1,134 lines) into modular subcomponents:
    - `viewport/mod.rs` (479 lines orchestrator with pan/zoom and paper frames), `viewport/crop_overlay.rs` (interactive crop, handles, validation pill), `viewport/perspective_tools.rs` (leveling horizon, 2-parallel lines), `viewport/hud.rs` (live original, soft proofing, local mask badges), `viewport/brush_overlay.rs` (mouse painting dabs, cursor ring), `viewport/split_compare.rs` (draggable divider).
    - Encapsulated parameter states into `CropToolState` and `BrushViewportContext` resolving all argument count bloat (`clippy::too_many_arguments`).
    - Dedicated test suite `crates/rraw-app/tests/viewport_modularization_tests.rs` (6 tests) passing, 263 workspace tests passing across 60 suites, 0 clippy warnings, Qwen 3.6 35B Inquisitor APPROVED.
  - [x] Step 3: Decompose `crates/rraw-app/src/preview.rs` (2,264 lines) into dedicated processing layers:
    - `preview/masks.rs` (109 lines: AI segmentation & adjustments evaluation), `preview/orientation.rs` (314 lines: EXIF orientations, TIFF tags, directory scanning), `preview/types.rs` (414 lines: `LoadedPhotoView`, cam-to-sRGB calibration matrix, `BaseRgbPreview`, `CachedPhotoEntry`, `BackgroundDecodePayload`), `preview/decoder.rs` (370 lines: fast thumbnails, RAW/raster decoder payload, timelapse gradings), `preview/pipeline.rs` (1,112 lines: preview and export color processing, tone curves, LUT, optical corrections), `preview/mod.rs` (15 lines: 100% backward compatible re-exports).
    - Dedicated test suite `crates/rraw-app/tests/preview_modularization_tests.rs` (5 tests) passing, 268 workspace tests passing across 61 suites, 0 clippy warnings, Qwen 3.6 35B Inquisitor APPROVED.

- [x] **Task 15.3 (`rraw-app`)**: Scalable External Resource-Based i18n Engine
  - Replace the 1,380-line hardcoded `match (self, lang)` table in `crates/rraw-app/src/i18n/mod.rs` with externalized translation catalogs under `assets/i18n/` (`fr.json`, `en.json`, `es.json` with 326 keys each).
  - Decompose `crates/rraw-app/src/i18n/` into clean domain submodules: `key.rs` (326 typed keys with O(1) index mapping), `language.rs` (Fr, En, Custom([u8; 8]) supporting arbitrary codes), `catalog.rs` (immutable zero-heap allocation indexed catalogs `Box<[&'static str; Key::COUNT]>`), `engine.rs` (`I18nEngine` thread-safe registry with automatic discovery), `mod.rs` (clean backward-compatible facade).
  - Enable zero-code language addition: dropping any valid JSON catalog into `assets/i18n/` or `~/.config/rraw/i18n/` automatically registers the language and exposes it in the UI status bar selector without recompiling core Rust code.
  - Preserve zero-allocation runtime lookup for static labels during the 120 Hz render loop and instant live-switch without application restart.
  - Dedicated test suite `crates/rraw-app/tests/i18n_catalog_tests.rs` (7 tests) + `i18n_tests.rs` (5 tests) passing, 275 workspace tests passing across 62 suites, 0 clippy warnings, Qwen 3.6 35B Inquisitor APPROVED.

---

## Phase 16 — System Robustness, Concurrency Hardening & Controller Decomposition
- [x] **Task 16.1 (`rraw-app`)**: Concurrency Hardening, Thread Cancellation & VRAM Eviction Engine (Option A)
  - Dedicated background decode worker replacing raw uncontrolled `std::thread::spawn` calls.
  - Atomic generation counter (`active_epoch: Arc<AtomicU64>`) / request cancellation to instantly abort stale decode jobs during rapid filmstrip scrubbing.
  - Bounded GPU thumbnail cache `LruThumbnailCache` (cap of 256 active texture handles) with LRU eviction to prevent VRAM saturation on 5,000+ photo directories.
  - Automatic VRAM purge and sidecar pruning on directory change (`open_directory`).
  - Unit/integration test suite `concurrency_and_cache_tests.rs` verifying epoch abort and bounded VRAM allocation (6 tests).
  - 284 workspace tests passing across 63 suites, 0 clippy warnings under `-D warnings`, zero unwrap/expect in production code, Inquisitor APPROVED via Qwen 3.6 35B A3B.

- [x] **Task 16.2 (`rraw-app`)**: Architectural Decomposition of `RrawApp` into Specialized Domain Controllers (Option B)
  - Decompose monolithic 73-field `RrawApp` into 5 cohesive sub-structures:
    - `PhotoSession`: Active photo view, CFA buffers, base RGB preview, active grading state, preset, textures.
    - `ViewportController`: Camera pan/zoom, guides, split comparison, clipping indicators, histogram, interactive tool states.
    - `LibraryController`: Current directory, photo entries, multi-selection, star ratings, RAW filters, EXIF query, recipe clipboard.
    - `ExportBatchController`: Export dialog settings, formats, bit-depth, subfolder targets, timelapse sequence configuration.
    - `BackgroundWorkers`: Thread channels, active epoch, thumbnail worker, decode worker.
  - Refactor all call-sites in `app.rs`, `ui/`, `preview/` and main event loop to eliminate borrow-checker conflicts and enforce least privilege.
  - Dedicated test suite `controller_modularization_tests.rs` validating independent controller instantiation and state integrity (5 tests).
  - 289 workspace tests passing across 64 suites, 0 clippy warnings under `-D warnings`, zero unwrap/expect in production code, Inquisitor APPROVED via Qwen 3.6 35B A3B.

- [x] **Task 16.3 (`rraw-app`)**: End-to-End Stress Testing, Parity Verification & Adversarial Audit
  - High-speed automated navigation stress test across 250+ synthetic RAW files asserting zero CPU thread leaks, bounded VRAM, and zero stale frame displays (`stress_concurrency_tests.rs`).
  - Validation of atomic epoch cancellation under high contention (>200 obsolete requests dropped per second during rapid scrubbing).
  - Validation of bounded GPU thumbnail cache (`LruThumbnailCache` strictly capped at 256 texture handles).
  - Multi-threaded worker queue dispatch pool stress test and batch clipboard consistency across 200 photos.
  - 294 workspace tests passing across 65 suites, 0 clippy warnings under `-D warnings`, zero unwrap/expect in test and production code.
  - Adversarial local code audit APPROVED via Qwen 3.6 35B Inquisitor at 0 cloud tokens.

---

## Phase 17 — Sensor Fidelity, Dynamic CFA & True Multi-Format Export
- [x] **Task 17.1 (`rraw-decode`)**: Dynamic CFA Pattern Resolution & Phase Alignment
  - Query `raw_image.camera.cfa` from `rawler` and map dynamically to `CfaPattern::Bayer(Rggb | Bggr | Grbg | Gbrg)` or `CfaPattern::fujifilm_xtrans()`.
  - Fix optical black margin cropping modulo alignment (modulo 6 for X-Trans, modulo 2 for Bayer) to preserve sensor phase (`align_crop_to_cfa_phase`).
  - Eliminate hardcoded RGGB in `rraw-decode/src/lib.rs`.
  - Dedicated unit test suite `crates/rraw-decode/tests/cfa_resolution_tests.rs` verifying channel assignment across all 4 Bayer variants and X-Trans (4 tests).
  - 298 workspace tests passing across 66 suites, 0 clippy warnings under `-D warnings`, zero unwrap/expect in production code, Inquisitor APPROVED via Qwen 3.6 35B A3B.

- [x] **Task 17.2 (`rraw-core`)**: Adaptive Edge-Aware Demosaicing for Fujifilm X-Trans (6x6)
  - Replace naive `demosaic_bilinear` in `rraw-core/src/demosaic.rs` with `demosaic_xtrans_edge_aware` (Markesteijn 1-pass variant) evaluating horizontal/vertical gradients on Green channels and reconstructing $(R-G)$ and $(B-G)$ chrominance.
  - Directional green plane reconstruction with Rayon multithreading and color difference guidance preventing zipper artifacts and color bleeding.
  - Dedicated unit test suite in `demosaic_tests.rs` asserting PSNR >= 38 dB on synthetic Siemens star, exact flat-field reproduction, and native photosite preservation (4 tests).
  - 302 workspace tests passing across 66 suites, 0 clippy warnings under `-D warnings`, zero unwrap/expect in production code, Inquisitor APPROVED via Qwen 3.6 35B A3B.

- [x] **Task 17.3 (`rraw-export`, `rraw-app`)**: True AVIF HDR & Safe Multi-Format Batch Export
  - Enable `image` feature `avif` in root `Cargo.toml` utilizing `ravif` encoder.
  - Implement real AVIF encoding (`AvifEncoder::new_with_speed_quality`) in `rraw-export::BatchExportQueue` for `Avif10` and `Avif12`.
  - Eliminate silent PNG fallback: unsupported batch export formats (such as JXL) now return clean, strongly-typed `Err(ExportError::UnsupportedFormat)`.
  - Expose AVIF (`ExportFormatChoice::Avif`) with `.avif` extension, label, and quality slider in `rraw-app` state and UI export modal.
  - Unit tests verifying genuine ISOBMFF AVIF brand `ftyp/avif` signatures and proper error propagation on unsupported formats (`export_tests.rs`).
  - 304 workspace tests passing across 66 suites, 0 clippy warnings under `-D warnings`, zero unwrap/expect in production code, Inquisitor APPROVED via Qwen 3.6 35B A3B.

---

## Phase 18 — High-Performance Directory Session DAM (Option A: `.rraw/cache.db`)
- [x] **Task 18.1 (`rraw-storage`)**: Session-Mode SQLite Engine & High-Concurrency WAL Pragmas
  - Add Session Mode to `SqliteDamIndex`: automatic creation/discovery of `.rraw/cache.db` within opened photo directories.
  - Activate `PRAGMA journal_mode = WAL;`, `PRAGMA synchronous = NORMAL;`, `PRAGMA temp_store = MEMORY;`, `PRAGMA busy_timeout = 5000;` for lockless concurrent reads/writes.
  - Implement incremental sync (`sync_photo`, `sync_sidecar`): index metadata and star/color ratings while keeping `.rraw.json` sidecars as the single source of truth.
  - Dedicated unit and multi-threaded stress tests (`session_dam_tests.rs`). 309 workspace tests passing across 67 suites, 0 clippy warnings under `-D warnings`, Inquisitor APPROVED via Qwen 3.6 35B A3B.

- [x] **Task 18.2 (`rraw-app`)**: Asynchronous Directory Indexing & Sub-Millisecond Filmstrip Filtering
  - Wire `SqliteDamIndex` into `LibraryController` in `rraw-app` via `session_dam: Option<SqliteDamIndex>`.
  - Background worker to scan/index directory EXIF tags into `.rraw/cache.db` on folder open without freezing the UI thread (< 0.05 ms dispatch time).
  - Bounded receiver drain (`drain_dam_indexing_events(128)`) preserving 120 FPS ProMotion rendering without frame drops.
  - Exponential backoff retry loop on SQLite contention (`SQLITE_BUSY`).
  - Bidirectional sync: UI star rating and color label shortcuts write to both `.rraw/cache.db` (UPSERT) and sidecar `.rraw.json`.
  - Dedicated unit and integration tests (`session_dam_app_tests.rs`). 312 workspace tests passing across 68 suites, 0 clippy warnings under `-D warnings`, Inquisitor APPROVED via Qwen 3.6 35B A3B.

---

## Phase 19 — Comprehensive Lens Correction & Dynamic Lensfun Database
- [x] **Task 19.1 (`rraw-optics`)**: Embedded & External Lensfun XML Database Engine
  - Embedded compressed official Lensfun XML catalog (> 1,500 lenses, 511 KB gzip) into `rraw-optics` using streaming pure-Rust `quick-xml` reader (`data/lensfun_database.xml.gz`).
  - Added discovery and merging of host system databases (`RRAW_LENSFUN_PATH`, `~/Library/Application Support/rraw/lensfun/`, macOS Homebrew `/opt/homebrew/share/lensfun/`, `/usr/share/lensfun/`, etc.).
  - Continuous interpolation across focal length and aperture brackets for polynomial distortion (ptlens, poly3, poly5), lateral TCA (poly3, linear), and radial vignetting (pa model converted to inverse gain).
  - Wired into `OpticalResolver` Tier 2 (`Tier2LensfunXml`) with priority fallback chain (curated catalog -> full Lensfun database -> generic fallback).
  - Dedicated unit tests in `crates/rraw-optics/tests/lensfun_tests.rs` (4 tests). 316 workspace tests passing across 69 suites, 0 clippy warnings under `-D warnings`, Inquisitor APPROVED via Qwen 3.6 35B A3B.

- [x] **Task 19.2 (`rraw-decode`, `rraw-app`)**: Universal MakerNotes Lens ID Resolution & Multilingual EXIF Localization
  - Implemented `resolve_universal_lens` in `crates/rraw-decode/src/makernotes.rs` extracting vendor MakerNote tags from native RAW and DNG PrivateData (Pentax `0x003F` `LensRec`, Sony `0x0102`, Canon `0x0016`, Nikon `0x0084`).
  - Integrated with `rawler::lens::LensResolver` and 15,000+ lens database, cross-referencing camera make, mount (`k-mount`, `e-mount`, `ef-mount`, `f-mount`), focal length, and aperture to disambiguate third-party lenses (Sigma, Tamron, Tokina).
  - Wired into `decode_file`: automatically populates `PhotoMetadata.lens_model` and EXIF `Lens Model` string, triggering automatic Lensfun profile resolution. Added manufacturer deduplication `sanitize_lens_name`.
  - Added full multilingual dictionary support in `assets/i18n/{fr,en,es}.json` and `crates/rraw-app/src/i18n/` (62 new typed keys for EXIF tags and enum values in FR/EN/ES, zero heap allocation).
  - Enhanced EXIF UI: removed duplicated tags from the detailed collapsible grid, enforced uniform monoline layout with `.truncate()` and full tooltip `.on_hover_text(...)`.
  - Dedicated unit and synthetic tests in `crates/rraw-decode/tests/decode_tests.rs` and `crates/rraw-app/tests/i18n_tests.rs`. 319 workspace tests passing across 69 suites, 0 clippy warnings under `-D warnings`, Inquisitor APPROVED via Qwen 3.6 35B A3B.

- [x] **Task 19.3 (`rraw-optics`, `rraw-app`)**: Tier 1 DNG Embedded Opcode Bridge & UI Manual Profile Picker Override
  - Bridge decoded DNG `WarpRectilinear` opcodes directly into Stage 4 `UvLutTable` grid.
  - Add UI lens profile picker and manual override in `ui/sections/optics.rs` with fuzzy match confidence indicator and update check.

---

## Phase 20 — Native Apple Silicon Neural Engine Bridge (CoreML)
- [x] **Task 20.1 (`rraw-coreml` / `rraw-ai`)**: Hardware ANE Inference Bridge
  - Isolated macOS CoreML Objective-C runtime FFI in targeted safe crate `rraw-coreml` behind `#![deny(unsafe_code)]` with documented `// SAFETY:` proofs (preserving `#![forbid(unsafe_code)]` across 10 of 12 workspace crates).
  - Implemented Apple Silicon / ANE hardware detection via `sysctlbyname` with safe cross-platform fallback, detecting ANE cores and configuring optimal compute units (`CoreMlComputeUnits::CpuAndNeuralEngine` / `All`).
  - Implemented `AneRawDenoiser` for 504x504 CFA RAW tile denoising on ANE with Generalized Anscombe Transform (GAT) forward variance stabilization and exact inverse reconstruction.
  - Implemented `AneSemanticSegmenter` for 4-class semantic segmentation mask inference (`Subject`, `Sky`, `Background`, `Portrait`) strictly enforcing partition-of-unity invariant `subject + background == 1.0` and bilateral guided refinement.
  - Accelerated fallback engine with precomputed 5x5 Gaussian spatial weight matrix LUT and Rayon row-level parallelization (4.3x speedup).
  - Connected `AneSemanticSegmenter` into `rraw-app` preview masks pipeline (`crates/rraw-app/src/preview/masks.rs`).
  - Dedicated unit and integration tests in `crates/rraw-coreml/tests/ane_bridge_tests.rs` (8 tests). 331 workspace tests passing across 72 suites, 0 clippy warnings under `-D warnings`, Inquisitor APPROVED via Qwen 3.6 35B A3B.

- [x] **Task 20.2 (`rraw-app`, `rraw-gpu`)**: Interactive AI Masking & Real-Time Neural Denoising UI
  - Connected real CoreML segmentation output (`SemanticMaskSet.to_rgba_interleaved()`) directly to GPU Stage 6 mask blending pipeline (`execute_stage6_gpu`).
  - Added interactive toggle, strength slider (0–100%), and dynamic hardware badge (`⚡ ANE Accéléré (16 cœurs)` vs `⚙️ Émulation CPU`) in Darkroom Detail & Denoise section.
  - Integrated ratio-preserving luminance scaling neural denoising in preview and export rendering loops (`apply_ane_neural_denoise_rgb`).
  - Expanded multilingual dictionaries (FR, EN, ES) with 7 new typed keys (453 total typed keys).
  - Added dedicated integration tests in `crates/rraw-app/tests/ane_ui_pipeline_tests.rs` (4 tests). 335 workspace tests passing across 73 suites, 0 clippy warnings under `-D warnings`, Inquisitor APPROVED via Qwen 3.6 35B A3B.

---

## Phase 21 — Final Optimization, Latency Profiling & Production Hardening
- [x] **Task 21.1 (`rraw-core`, `rraw-gpu`, `rraw-app`)**: Memory Footprint Optimization & Zero-Copy Metal Pipeline
  - Converted `SignedCfaBuffer` samples to `Arc<[f32]>` in `rraw-core` with zero-copy constructor `from_arc`, `samples_arc`, and `allocation_size_bytes`, achieving $O(1)$ zero-copy cloning across background decode threads, caches, and export queues (saving ~181 MB per clone on 45.4 MP images).
  - Converted `BaseRgbPreview.samples_rgb` to `Arc<[[f32; 3]]>` in `rraw-app` for $O(1)$ zero-copy preview sharing (saving ~545 MB per clone on full-frame 45.4 MP previews).
  - Eliminated artificial WebGPU 128 MB storage buffer clamp in `rraw-gpu` (`GpuContext::new_headless`) by querying native `adapter.limits()`, enabling allocation of full-resolution multi-gigabyte buffers on Metal Apple Silicon UMA.
  - Implemented dynamic memory budget tracking in `rraw-app` (`insert_cached_photo` with 1.5 GB RAM cap and `memory_footprint_bytes()`), enforcing LRU eviction with underflow-safe `saturating_sub` arithmetic.
  - Dedicated unit and integration tests in `crates/rraw-core/tests/memory_footprint_tests.rs`, `crates/rraw-gpu/tests/zero_copy_metal_tests.rs`, and `crates/rraw-app/tests/memory_optimization_tests.rs` (3 new suites, 6 new tests). 341 workspace tests passing across 76 suites, 0 clippy warnings under `-D warnings`, Inquisitor APPROVED via Qwen 3.6 35B A3B.

- [x] **Task 21.2 (`rraw-core`, `rraw-gpu`, `rraw-app`, `rraw-cli`)**: End-to-End Latency Benchmarks & Stage-by-Stage Profiling
  - Implemented comprehensive rendering pipeline timing instrumentation (`PipelineStageId`, `ExecutionBackend`, `StageTiming`, `PipelineTiming`, `StageTimer`) across all 7 discrete photographic stages in `rraw-core`.
  - Created `GpuPipelineProfiler` in `rraw-gpu` orchestrating and profiling all 7 WGSL compute pipelines (`Stage1Pipeline` through `Stage7Pipeline`) with division-by-zero safe throughput arithmetic (MPx/s) and sub-8.33ms 120 FPS frame time validation.
  - Profiled CPU/Rayon Darkroom preview pipeline (`render_preview_from_base_profiled`) and asynchronous decode coordinator (`decode_photo_file_payload_profiled`) in `rraw-app`.
  - Added CLI profiling and micro-benchmark mode in `rraw-cli` (`--profile` / `-P`) outputting structured ASCII performance reports with per-stage latency, throughput, and 120Hz/50ms pass/fail verdicts.
  - Dedicated integration latency test suites in `crates/rraw-gpu/tests/pipeline_latency_benchmarks.rs` and `crates/rraw-app/tests/end_to_end_latency_tests.rs` (2 new suites, 6 new tests). 352 workspace tests passing across 78 suites, 0 clippy warnings under `-D warnings`, Inquisitor APPROVED via Qwen 3.6 35B A3B.

- [x] **Task 21.3 (`rraw-app`, CI / Packaging)**: Production Hardening, Release Polish & macOS App Packaging
  - Polished keyboard navigation and shortcuts across all views: added `Cmd+O` / `Cmd+Shift+O` (Open Image / Open Folder), `Cmd+E` (Export Dialog), `Cmd+1/2/3` (instant left panel tab switching between Files, Styles, and EXIF Metadata), and `Enter` to confirm export dialog. Dedicated right panel purely to Darkroom development without redundancy.
  - Guarded star rating shortcuts (`0..=5`) against Command/Ctrl/Alt modifiers to prevent accidental rating changes when using system shortcuts.
  - Hardened native macOS application packaging pipeline (`scripts/package_macos.sh`) with Hardened Runtime entitlements (`assets/macos/Entitlements.plist`), codesigning verification, compressed drag-and-drop DMG generation (UDZO), SHA-256 checksums, and Apple Notarization (`--notarize-profile`) and stapling readiness.
  - Comprehensive user-facing documentation in `README.md` including packaging guide and complete keyboard shortcuts cheat sheet.
  - Dedicated integration test suite `crates/rraw-app/tests/keyboard_navigation_tests.rs` verifying tab transitions, rating invariants, entitlements validity, Info.plist attributes, and packaging script attributes.
  - 357 workspace tests passing across 79 suites, 0 clippy warnings under `-D warnings`, Inquisitor APPROVED via Qwen 3.6 35B A3B.

---

## Phase 22 — Deep Maturity Audit #4 Remediation (P0 / P1 / P2)
- [x] **Task 22.1 (`rraw-app`)**: Priority P0 — Parallel Single-Buffer Export, O(1)-RAM Streaming Timelapse, AABB Brush Grid & Atomic Sidecar Persistence
  - Implemented `ExportRasterBuffer` (`U8(Vec<u8>)` | `U16(Vec<u16>)`) and Rayon-parallelized `render_export_image_with_depth`, allocating only the requested bit-depth buffer and guaranteeing mathematical parity with preview rendering.
  - Refactored Timelapse sequence export (`spawn_background_timelapse_export`, `execute_timelapse_export`, `prepare_timelapse_gradings_from_lumas`) into an asynchronous 2-pass $O(1)$-RAM streaming pipeline emitting incremental progress events.
  - Implemented `PrecomputedBrushGrid` (512x512 AABB-localized bilinear lookup grid) and `eval_all_mask_adjustments_with_brush_grids`, reducing per-pixel freehand brush evaluation from $O(D)$ to $O(1)$.
  - Added `atomic_write_str` (temp sibling file + `flush` + `sync_all` + atomic `rename`) for sidecars and layout config, and enforced `.rraw.json` > `.xmp` rating precedence in `load_photo_sidecars`.
  - Dedicated TDD integration suite `crates/rraw-app/tests/p0_audit_remediation_tests.rs` (4 tests). 361 workspace tests passing across 80 suites.
- [x] **Task 22.2 (`rraw-app`)**: Priority P1 — State Machine & Enum Hygiene, Duplicate Field Elimination & Ephemeral Sidecar Field Exclusion
  - Added `#[serde(skip)]` on `lens_search_query` and `defringe_preview_mask` in `OpticsGradingParams` so ephemeral UI fields never pollute `<photo>.rraw.json` sidecar files.
  - Implemented `ActiveModalDialog` (`Export`, `Shortcuts`, `About`), `open_dialog`, `toggle_dialog`, `active_modal_dialog`, and `set_target_mode` on `ExportBatchController`.
  - Implemented `ActiveCanvasTool` (`None`, `Crop`, `Horizon`, `ParallelLines(ParallelToolPhase)`, `FreehandBrush(usize)`) with `active_canvas_tool` and `activate_canvas_tool` on `RrawApp`, enforcing mutual exclusion across interactive viewport tools.
  - Added `set_left_panel_tab`, `set_active_tool_family`, and `set_active_tools_only` on `RrawApp` to maintain strict synchronization with `AppLayoutConfig`.
  - Dedicated TDD integration suite `crates/rraw-app/tests/p1_state_machine_hygiene_tests.rs` (4 tests). 365 workspace tests passing across 81 suites.
- [x] **Task 22.3 (`rraw-app`, workspace)**: Priority P2 — Monolith File Decomposition, Bundle-Aware Parametric i18n & Unused Cargo Dependency Cleanup
  - Removed 9 unused Cargo dependencies across `rraw-color`, `rraw-cli`, `rraw-coreml`, `rraw-storage`, and `rraw-optics`, achieving 0 compiler/clippy warnings across all 12 workspace crates.
  - Added `I18nEngine::candidate_i18n_directories` and `discover_default_catalogs` supporting macOS `.app` bundle `../Resources/assets/i18n`, `<exe_dir>/assets/i18n`, `<cwd>/assets/i18n`, and `~/.config/rraw/i18n`, plus `Catalog::get_custom`, `I18nEngine::translate_custom`, `Language::is_spanish`, and localized Soft Proofing HUD and profiles.
  - Extracted `crates/rraw-app/src/presets.rs` (638 lines), `crates/rraw-app/src/mask_layers.rs` (530 lines), and `crates/rraw-app/src/sidecar.rs` (231 lines) from `crates/rraw-app/src/state.rs` (reduced from 2,122 to 806 lines) with transparent `pub use` re-exports.
  - Dedicated TDD integration suite `crates/rraw-app/tests/p2_architecture_and_i18n_tests.rs` (4 tests). 369 workspace tests passing across 82 suites, 0 clippy warnings.

---

## Phase 23 — Deep Maturity Audit #5 Remediation (P0 / P1 / P2)
- [x] **Task 23.1 (`rraw-app`)**: Priority P0 — Real-Time UI Thread I/O Unblocking, Explorer Subdirectory Cache, Bounded Undo/Redo Stack & Unified Pixel Kernel
  - Eliminated synchronous per-frame `save_photo_sidecars` (`fsync`) from `render_darkroom_section` (`crates/rraw-app/src/ui/inspector.rs`) during continuous 60-120 Hz slider drags and removed the detached racing `std::thread::spawn` in `open_raw_file`, coalescing disk persistence via `PhotoSession::sidecar_dirty` and `RrawApp::flush_dirty_sidecar_if_idle`.
  - Added `LibraryController::cached_subdirectories` and `refresh_subdirectories()` (`crates/rraw-app/src/controllers/library.rs`), eliminating per-frame `std::fs::read_dir` syscalls in `render_explorer_tab` (`crates/rraw-app/src/ui/explorer.rs`) and guaranteeing alphabetical directory ordering.
  - Implemented a bounded 64-step Undo/Redo history stack in `PhotoSession` (`MAX_UNDO_HISTORY = 64`, `undo_stack`, `redo_stack`, `record_undo_snapshot`, `notify_grading_edited`, `undo`, `redo`, `clear_history`) and wired `undo_grading`, `redo_grading`, `Cmd+Z` / `Cmd+Shift+Z` / `Cmd+Y`, and top-bar `↶` / `↷` controls in `RrawApp`.
  - Extracted `PreparedGradingFrame<'a>` in `crates/rraw-app/src/preview/pipeline.rs` (reducing `pipeline.rs` from 1,422 lines to 853 lines), unifying per-pixel optical, tonal, curve, LUT, mask, DRT, and soft-proofing evaluation between interactive preview and 8-bit/16-bit export.
  - Dedicated TDD integration suite `crates/rraw-app/tests/p0_audit5_realtime_undo_pipeline_tests.rs` (4 tests). 373 workspace tests passing across 83 suites, 0 clippy warnings.
- [x] **Task 23.2 (`rraw-app`)**: Priority P1 — Ghost Boolean Eradication (`ActiveModalDialog` & `ActiveCanvasTool` Single Source of Truth), True LRU Thumbnail Recency & Dead Code Purge
  - Added `ExportBatchController::set_dialog_open` and `close_dialog` (`crates/rraw-app/src/controllers/export.rs`), synchronizing `active_dialog: Option<ActiveModalDialog>` and `show_*_dialog` across `ui/about.rs`, `ui/shortcuts.rs`, `ui/export.rs`, `ui/canvas_hud.rs`, `ui/inspector.rs`, and `app.rs` (`handle_escape_key`, top bar, status bar).
  - Added `RrawApp::reconcile_canvas_tool_transitions` (`crates/rraw-app/src/app.rs`), wired across `AiSemanticMasks`, `Crop`, `Straighten`, and `Geometry` in `ui/inspector.rs`, and enforced automatic tool deactivation in `ui/sections/crop.rs` and `ui/sections/geometry.rs` when `crop_enabled` or `geometry_enabled` is unchecked.
  - Upgraded thumbnail lookup in `crates/rraw-app/src/ui/filmstrip.rs` from `get(entry)` to `get_and_touch(entry)`, restoring true LRU recency updates instead of FIFO eviction.
  - Purged ~502 lines of uncalled dead code across `crates/rraw-app/src/ui/export.rs` (synchronous `execute_export`, `execute_timelapse_export`, `write_exported_image`, `build_export_base_from_cfa`, `resolve_photo_preview_and_grading`), `crates/rraw-app/src/ui/exif.rs` (`render_exif_compact_card`, `render_exif_card`, `render_exif_full_panel`), and `crates/rraw-app/src/ui/presets.rs` (`render_presets_tab`).
  - Dedicated TDD integration suite `crates/rraw-app/tests/p1_audit5_typestate_lru_cleanup_tests.rs` (3 tests). 376 workspace tests passing across 84 suites, 0 clippy warnings.
- [x] **Task 23.3 (`rraw-app`)**: Priority P2 — 100% Residual French String Localization & Domain Crate Feature Wiring
  - Added 25 language-aware localization helpers in `crates/rraw-app/src/i18n/mod.rs` (`explorer_*`, `exif_*`, `filmstrip_*`, `left_panel_tab_labels`, `soft_proof_out_of_gamut_badge`, `export_hdr_gain_map_*`) with custom catalog override lookup (`I18nEngine::translate_custom`) and built-in `Language::Fr`, Spanish (`lang.is_spanish()`), and `Language::En` translations, wired across `ui/explorer.rs`, `ui/exif.rs`, `ui/filmstrip.rs`, `ui/export.rs`, and `app.rs` (top-bar Soft Proofing profile/intent/out-of-gamut badge).
  - Wired `rraw_optics::execute_richardson_lucy` (`RichardsonLucyParams`) into `PreparedGradingFrame::prepare` (`crates/rraw-app/src/preview/pipeline.rs`) whenever `capture_sharpening_amount > 0.5`, providing edge-regularized PSF deconvolution sharpening across both interactive preview and 8-bit/16-bit export with 100% mathematical parity.
  - Wired `rraw_export` ISO 21496-1 Gain Map HDR calculation and serialization (`GainMapCalculator::compute`, `Iso21496Params`, `Iso21496XmpPacket::encode`, `Iso21496TmapBox::encode`) into `ExportBatchController` (`export_hdr_gain_map`), `ExportWriteConfig` (`hdr_gain_map`), `RrawApp::write_iso21496_gain_map_sidecars`, and `render_export_dialog` (`crates/rraw-app/src/ui/export.rs`), emitting standardized `.hdrgm.xmp` and 40-byte `.tmap` ISOBMFF sidecars on export.
  - Dedicated TDD integration suite `crates/rraw-app/tests/p2_audit5_i18n_and_domain_wiring_tests.rs` (3 tests). 379 workspace tests passing across 85 suites, 0 clippy warnings.

---

## Phase 24 — Deep Maturity Audit #6 Remediation
- [x] **Task 24.1 (`rraw-app`, `rraw-optics`)**: Non-Blocking Directory Open, 100% Transactional Undo/Redo Coverage, Rayon Parallel Richardson-Lucy & Atomic Binary Persistence
  - Eliminated synchronous per-photo `load_photo_sidecars` loop on the UI thread in `RrawApp::open_directory` (`crates/rraw-app/src/app.rs`), preloading only the active photo synchronously while `spawn_directory_indexer` streams folder ratings asynchronously, and added `library.refresh_subdirectories()` on startup in `RrawApp::new()`.
  - Added `atomic_write_bytes` in `crates/rraw-app/src/sidecar.rs` and switched `.tmap` (`crates/rraw-app/src/ui/export.rs`) and `.exif.json` (`crates/rraw-app/src/ui/exif.rs`) writes to atomic persistence.
  - Extended transactional Undo/Redo coverage (`record_undo_snapshot` / `notify_grading_edited`) across all interactive viewport tools (`crates/rraw-app/src/ui/viewport/mod.rs`: freehand brush, crop overlay, parallel perspective lines, horizon leveling), 3D LUT import/removal/strength slider (`crates/rraw-app/src/ui/presets.rs`), tone curve reset controls (`reset_active_tone_curve`, `reset_all_tone_curves` in `crates/rraw-app/src/ui/tone_curves.rs`), and top-bar Soft Proofing controls (`crates/rraw-app/src/app.rs`).
  - Parallelized `rraw_optics::execute_richardson_lucy` (`crates/rraw-optics/src/rl.rs`) across rows using Rayon `par_chunks_mut`.
  - Dedicated TDD integration suite `crates/rraw-app/tests/p0_audit6_directory_undo_atomic_tests.rs` (4 tests). 383 workspace tests passing across 86 suites, 0 clippy warnings.
- [x] **Task 24.2 (`rraw-app`, `rraw-storage`)**: `DiskThumbnailCache` Integration, `app.rs` & `ui/export.rs` Modular Extraction & Layout State Sync
  - Extracted `crates/rraw-app/src/ui/chrome.rs` (651 lines) from `app.rs` (`render_top_bar`, `render_status_bar`, `handle_global_keyboard_shortcuts`), reducing `app.rs` from 2,041 to 1,422 lines.
  - Extracted `crates/rraw-app/src/ui/export_runner.rs` (756 lines) from `ui/export.rs` (raster disk writers, ISO 21496-1 Gain Map writers, streaming background batch/timelapse threads), reducing `ui/export.rs` from 1,465 to 720 lines.
  - Connected `rraw_storage::DiskThumbnailCache` in `crates/rraw-app/src/preview/decoder.rs` (`generate_fast_thumbnail`) using SHA-256 content-hash indexing (`path`, file `len`, `mtime`, sidecar `xmp`) with bounds-checked binary serialization.
  - Added `RrawApp::reset_layout()` restoring default filters (`ToolFamily::All`, `active_tools_only = false`) and deactivating canvas tools (`ActiveCanvasTool::None`), wired to the `DarkroomResetLayout` header button in `ui/inspector.rs`.
  - Wired `activate_canvas_tool(ActiveCanvasTool::None)` when canvas tools (crop, horizon, perspective) complete in `ui/viewport/mod.rs`.
  - Dedicated TDD integration suite `crates/rraw-app/tests/p1_audit6_task24_2_cache_chrome_layout_tests.rs` (3 tests). 387 workspace tests passing across 87 suites, 0 clippy warnings.
- [x] **Task 24.3 (`rraw-app`, `rraw-optics`, `rraw-export`)**: Final French String Localization & Domain Crate English Purity
  - Registered 5 tool deactivation keys (`EscapeCropDeactivated`, `EscapeHorizonDeactivated`, `EscapePerspectiveDeactivated`, `EscapeBrushDeactivated`, `EscapeSelectionReset`) in `Key` enum, updated `Key::COUNT = 458`, `ALL_KEYS`, `as_str()`, `from_str()`, and populated all 3 JSON translation catalogs (`en.json`, `fr.json`, `es.json`).
  - Added localized helper functions in `crates/rraw-app/src/i18n/mod.rs` (`batch_selection_count_message`, `batch_deselection_message`, `batch_toggle_photo_message`) handling dynamic pluralization across French, Spanish, and English.
  - Converted all runtime error and completion messages in `crates/rraw-app/src/ui/export_runner.rs` to 100% English, achieving zero non-English strings in domain logic.
  - Dedicated TDD integration suite `crates/rraw-app/tests/p2_audit6_task24_3_i18n_purity_tests.rs` (2 new tests). 389 workspace tests passing across 88 suites, 0 clippy warnings.

---

## Phase 25 — Pre-Release Hardening, Performance Benchmarks & Packaging
- [x] **Task 25.1 (`workspace`)**: End-to-End Performance Benchmarking & Memory Profile
  - Full performance benchmarks on RAW decoding (52.8 MPx/s on real 24MP DNG, synthetic DNG opcode parsing in ~250 µs), Richardson-Lucy Rayon deconvolution scaling, ISO 21496-1 Gain Map calculation (28 MPx/s) and sidecar serialization (40-byte ISOBMFF .tmap, Apple HDRGainMap XMP packet).
  - Memory footprint bounds verification: 800 MB photo cache cap, LRU eviction across 10 photos, and sub-millisecond `DiskThumbnailCache` read/write latency.
  - Multi-format matrix evaluation: extension matching across all 9 camera RAW formats (ARW, CR2, CR3, DNG, NEF, RAF, RW2, ORF, PEF), TIFF Little-Endian & Big-Endian EXIF orientation tags (1..=8), JPEG orientation, and graceful rejection of corrupt files without panics.
  - Interactive UI darkroom responsiveness: full grading pipeline verified within interactive budget.
- [x] **Task 25.2 (`rraw-app`, `scripts`)**: Production macOS Packaging, Bundle Validation & Release Readiness
  - Added idempotent i18n translation catalog bundling (`assets/i18n` -> `Contents/Resources/assets/i18n`) in `scripts/package_macos.sh`.
  - Executed production release build producing signed `Rraw.app` bundle and compressed UDZO `Rraw-v0.1.0.dmg` (12.7 MB, SHA-256 `fd9f3f1b51062b2e8cc29286269795f8fefdc86e0c5bbfa7f0f27d7073f66e12`).
  - Verified codesign validity and hardened runtime entitlements with `codesign --verify --deep --strict`.
  - Dedicated TDD integration suite `crates/rraw-app/tests/phase25_benchmarks_and_formats_tests.rs` (7 new tests). 396 workspace tests passing across 89 suites, 0 clippy warnings.
- [x] **Task 25.3 (`docs`, `assets`)**: Darkroom UI Documentation & README Showcase
  - Added high-resolution Darkroom UI screenshot to `assets/screenshots/darkroom_ui.jpg`.
  - Updated `README.md` showcasing the 120 Hz Metal EDR interface with real-time histogram, tone curves, and filmstrip.

---

## Phase 26 — Maturity Audit #7 Remediation (Phases 1–6)
- [x] **Phase 1 (`cd3c770`, `rraw-app`)**: i18n Integrity & Multilingual Scalability — eliminated all 57 hardcoded language branches; unified 620 dynamic keys across `assets/i18n/{en,fr,es}.json` via `tc(lang, key)`.
- [x] **Phase 2 (`a7800ac`, `rraw-app`, `rraw-decode`)**: Split Monolithic Functions — modularized `render_filmstrip_panel`, `render_central_viewport`, `PureRustRawDecoder::decode_file`, and `render_export_dialog` (+7 unit tests).
- [x] **Phase 3 (`0ce4084`, `rraw-gpu`, `rraw-app`)**: Parameter Objects — introduced `RawSensorFrame`, `PipelineDispatchContext`, `SemanticMaskPass`, `ViewportToolContext`, and `MaskUiContext`; removed `#[allow(clippy::too_many_arguments)]`.
- [x] **Bug Fix (`9a19398`, `rraw-app`)**: Crop Overlay Drag Responsiveness — deferred preview rebuild during drag (`notify_grading_edited_deferred_preview`) and extracted `hit_test_crop_handle` using `press_origin()`.
- [x] **Phase 4 (`ad0b57c`, `rraw-app`)**: De-bloat God Structs — decomposed `PreparedGradingFrame`, `ExportBatchController`, and `DarkroomGradingState` while preserving flat `SidecarRecipe` JSON schema.
- [x] **Bug Fix (`4a7ca08`, `rraw-app`)**: Right Inspector Masks Panel Overflow — fixed horizontal clipping in fixed 360px right inspector (`ui/sections/masks.rs`).
- [x] **Phase 5 (`a582385`, `rraw-app`)**: Performance & Filmstrip Virtualization — migrated filmstrip to `ScrollArea::show_viewport` with exact spacers and `filtered_indices: Vec<usize>` (10,000+ entries) and added `FULL_RES_DEMOSAIC_INVOCATIONS` telemetry (+4 tests).
- [x] **Phase 6 (`d69beed`, `rraw-color`, `rraw-gpu`, CI, `README.md`)**: `cargo-llvm-cov` CI gate (`>= 85%` lines on 5 core crates, verified **86.33%**) and headless WGPU 24-patch ColorChecker visual regression test suite ($\Delta E_{00} < 0.5$, **417 workspace tests passing across 92 suites**).











