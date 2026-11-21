# HANDOFF.md — Next Action & Session Continuity

## Last Completed Milestone

- **Phase 13.1 — Semantic Versioning & Build Metadata (`rraw-core`, `rraw-app`, `rraw-cli`)**:
  - Implemented `rraw-core::build_info` module and `build.rs` embedding compile-time semantic version (`VERSION`, `VERSION_TAG = "v..."`), Git commit hash (`GIT_COMMIT_HASH`, 7 short hex chars), committer build date (`BUILD_DATE` ISO 8601 UTC), and full CLI string (`LONG_VERSION`).
  - Streamlined darkroom top toolbar: removed redundant `RRAW` heading and relocated compact `FR | EN` toggle to the bottom status bar right-to-left layout (`[FR | EN] · [EDR 120 Hz] · [v0.1.0]`), allowing the top toolbar to start directly with primary photo workflow actions (`Open Image`, `Open Folder`, `Export`).
  - Added "ℹ️ À propos" modal dialog (`render_about_dialog`) in `crates/rraw-app/src/ui/about.rs` with application branding, metadata grid (Version, Commit, Build Date) and Escape key hierarchical dismissal.
  - Added typed i18n keys for French and English in `crates/rraw-app/src/i18n/mod.rs` (`TopBarAbout`, `AboutTitle`, `AboutDescription`, `AboutVersion`, `AboutCommit`, `AboutBuildDate`, `AboutClose`).
  - Added CLI `--version` and `-V` support for `rraw-cli` (via clap `version` & `long_version`) and `rraw-app` (via early argument interception).
  - Added integration test suite `crates/rraw-app/tests/version_tests.rs` (8 tests) asserting semver validity, ISO 8601 UTC date, hex commit hash, `VERSION_TAG`, `LONG_VERSION`, and headless About modal state/Escape dismissal.
  - Commits: `b29cad9` and `68ffca5`.

- **Phase 11 — Internationalization (i18n) (`rraw-app`)**:
  - Implemented typed, zero-allocation string tables (`Language { Fr, En }` enum + `Key` enum with 294 typed UI keys) in `crates/rraw-app/src/i18n/mod.rs`.
  - Compile-time exhaustiveness enforced via `match (self, lang)` mapping every key to both French and English `&'static str` without wildcards.
  - Added backward-compatible `Language` persistence in `AppLayoutConfig` (`layout.json`) with `#[serde(default)]` defaulting to French for existing installations.
  - Updated `ToolFamily::label_lang`, `DarkroomSection::title_lang`, and `ExportScaleMode::label_lang` with dynamic language support while preserving backward compatibility.
  - Added compact interactive `FR | EN` language toggle in the top bar with instant reactive switching without restart.
  - Localized full UI across `app.rs`, `inspector.rs`, `filmstrip.rs`, `explorer.rs`, `exif.rs`, `histogram.rs`, `canvas_hud.rs`, `shortcuts.rs`, `presets.rs`, `viewport.rs`, and `export.rs` (including Timelapse controls and modal dialog).
  - Stabilized egui UI state across language switches using explicit `id_salt` and `Id` hashes on all collapsing headers and modal dialogs.
  - Fixed darkroom inspector ergonomics on fixed 360px panel: line break for 0° rotation button, vertical stacking of local mask types, and concise section titles preventing collision with action buttons.
  - Added comprehensive test suite `crates/rraw-app/tests/i18n_tests.rs` (5 tests) asserting all 294 keys resolve to non-empty strings in all languages, language properties, backward-compatible serde, darkroom & tool family translations, and live language toggle on headless app.
  - Verified with 174 unit and integration tests passing across 47 suites, 0 clippy warnings, and Qwen 3.6 35B local audit APPROVED.

- **Escape Key Modal & Interactive Tool Dismissal Hierarchy (`rraw-app`)**:
  - Implemented hierarchical `handle_escape_key(&mut self) -> bool` on `RrawApp`:
    1. Dismisses active modal dialogs first (`show_shortcuts_dialog`, `show_export_dialog`).
    2. Cancels active viewport interactive tools (`is_crop_editing`, `parallel_tool_phase`, `active_brush_mask_idx`).
    3. Clears filmstrip multi-selection (`selected_photos.clear()`).
  - Added Escape key listeners directly inside `render_shortcuts_dialog` and `render_export_dialog`.
  - Added text-input focus guard (`ui.ctx().egui_wants_keyboard_input()`): prevents global navigation (arrows, PageUp/Down) and shortcuts (Space, Cmd+A, Cmd+D, digits 0..=5) from hijacking active text editing while allowing Escape to dismiss dialogs or release text focus.
  - Added headless testing constructor `RrawApp::new_headless() -> Self`.
  - Added comprehensive test suite `crates/rraw-app/tests/escape_key_tests.rs` (4 tests).
  - Verified with 169 unit and integration tests passing across 46 suites, 0 clippy warnings, and Qwen 3.6 35B local audit APPROVED.

- **Phase 12 — Batch Recipe Clipboard & Multi-Image Development (`rraw-core` & `rraw-app`)**:
  - Implemented `ColorToneRecipeClipboard` in `rraw-core::recipe` for headless/core Stage 5 colorimetric & tonal grading transfer with `from_stage5` and `apply_to_stage5`.
  - Implemented `GradingRecipeClipboard` in `rraw-app::state` capturing creative color & tone parameters (exposure, contrast, highlights, shadows, blacks, saturation, vibrance, white balance kelvin/tint, display transform AgX/OpenDrt, monochrome toggle, PCHIP tone curves, 3D custom CLUT, creative vignetting).
  - Strictly isolated creative settings: photo-dependent parameters (crop, straighten, geometry, local brush/gradient masks, CoreML semantic masks, lens optics distortion/CA/defringe/vignette correction, and ISO wavelet/chroma/luma denoise) are completely excluded from the clipboard and strictly preserved on target photos.
  - Implemented `RrawApp::copy_active_recipe`, `copy_recipe_for_path`, `paste_recipe_to_selection`, and `paste_recipe_to_path` in `crates/rraw-app/src/app.rs`.
  - Wired native keyboard shortcuts: `Cmd+Shift+C` (or `Ctrl+Shift+C`) to copy active recipe, `Cmd+Shift+V` (or `Ctrl+Shift+V`) to paste onto multi-selected photos (or active photo).
  - Added contextual right-click menu on filmstrip thumbnail cards (`response.context_menu`): "📋 Copier le rendu (Cmd+Shift+C)", "📥 Coller le rendu sur la sélection / cette photo (Cmd+Shift+V)", clipboard summary, and export lot toggling.
  - Added toolbar quick actions `📋 Copier rendu` and `📥 Coller sélection` in the filmstrip top bar.
  - Updated keyboard shortcuts modal cheat sheet (`crates/rraw-app/src/ui/shortcuts.rs`) with dedicated clipboard section.
  - Added full test coverage in `crates/rraw-core/tests/recipe_dag_tests.rs` and `crates/rraw-app/tests/clipboard_tests.rs` verifying selective parameter transfer, photo-dependent preservation, summary format, and sidecar persistence roundtrip.
  - Verified with 165 unit and integration tests passing across 45 test suites, 0 clippy warnings, and Qwen 3.6 35B local audit APPROVED.

- **UI Overhaul Step 1: Darkroom Palette Categorization, Search & Clean Sidebar Architecture (`rraw-app`)**:
  - Implemented `ToolFamily` categorization (`All`, `Light`, `Color`, `Detail`, `Geometry`, `LocalMasks`, `Timelapse`) with label and short-label mappings.
  - Extended `DarkroomSection::ALL` to 11 modules, repatriating parametric `ToneCurves` into the right darkroom panel under the `Light` family alongside Exposure and Contrast.
  - Implemented `section_matches_search` text filter with keyword aliasing for instant tool lookup.
  - Added `is_section_active` and `reset_section` enabling visual modified status indicators (`●`) and per-module `↺` defaults reset.
  - Redesigned `render_darkroom_panel` with category tabs, search input, clear button `✕`, active-only toggle (`✓ Modifiés`), and soft rounded card containers (`#14161b`).
  - Redesigned `render_left_panel` with the reference RGB/Luma Waveform Histogram pinned prominently at the top, followed by scrollable folder explorer, styles, and EXIF inspectors.
  - Sanitized entire git commit history across all 68 ancestor commits to eliminate third-party trademarked names at origin.
  - Local commit: `c4487d9` (`feat(rraw-app): implement darkroom palette categories, active filter, and search`).
  - Verified with 151 unit and integration tests passing across 44 test suites, 0 clippy warnings, and Qwen 3.6 35B local audit APPROVED.

- **UI Overhaul Step 1 Refinements: Ergonomics, Dimension Locking & Layout Persistence (`rraw-app`)**:
  - Fixed-size panel enforcement: `"darkroom_panel_fixed"` (360px) and `"left_panel_fixed"` (320px) with `.resizable(false)` and `.exact_size(...)` eliminating resize cursors and cached collapsed state.
  - Dynamic tool unrolling: selecting any specific category automatically opens all sections belonging to that family via `CollapsingState`.
  - Multi-line controls stacking:
    - Geometry: `Straighten` 90° CCW/CW stacked above Mirror H/V.
    - Masks: Brush Radius stacked above Flow; AI Mask Feather stacked above Opacity; Gradient coordinates stacked.
    - Color & Proofing: Full-width stacked ComboBoxes with label on line 1, dropdown on line 2, preventing horizontal scroll clipping.
  - Glyph tofu box fix: Replaced unsupported Unicode `☐`/`☑` characters with `✓` and clean text labels.
  - Layout state persistence: `active_tool_family` and `active_tools_only` saved and restored across app restarts via `~/.config/rraw/layout.json`.
  - Local commit: `80e97a4` (`fix(rraw-app): lock panel dimensions, prevent layout truncation and refine inspector ergonomics`).
  - Verified with 152 unit and integration tests passing across 44 test suites, 0 clippy warnings, and Qwen 3.6 35B local audit APPROVED.

- **French UI Terminology Harmonization (`rraw-app`)**:
  - Replaced "Waveform RGB" with "Histogramme RGB" across UI widgets and documentation.
  - Complete French photographic nomenclature across top bar, filmstrip, inspector sliders, and EXIF tables.
  - Local commit: `b55d6d3` (`fix(rraw-app): harmonize user interface terminology and labels in French`).
  - Verified with 152 unit and integration tests passing across 44 test suites, 0 clippy warnings, and Qwen 3.6 35B local audit APPROVED.

- **UI Overhaul Step 2: Pro Viewport HUD, Split Comparison, Clipping Overlays, Collapsible Filmstrip & Keyboard Shortcuts Guide (`rraw-app`)**:
  - Viewport Canvas HUD overlay: bottom-left card with file name, camera & lens, and shooting summary (`focal · aperture · shutter · ISO`); bottom-right interactive rating pill with 5 clickable stars. Toggled with `I`.
  - Top bar decluttering: technical EXIF parameters and ratings moved out of the top bar into the canvas HUD. Added top-bar display toggles (`Avant | Après`, `Écrêtage`, `Infos`, `Pellicule`, `⌨ Raccourcis`).
  - Before / After Split Comparison: side-by-side split view with interactive draggable divider (`⬌`), edge boundary clamping, and zero-flicker mesh rendering. Fixed click conflict where dragging divider would trigger fit peeking. Peeking untouched original via holding `D`, `P`, or `\`. Toggled with `Y`.
  - Canvas Clipping Overlays (Zébrures): photographic clipping detection (blue for crushed shadows `lum <= 1`, red for blown highlights `lum >= 250` or saturated color channels `>= 253`). Rounded `to_u8_fast` (+0.5) to ensure overexposed areas trigger red warnings accurately. Toggled with `J`.
  - Collapsible Filmstrip: toggled with `F9`, persists in `~/.config/rraw/layout.json` alongside HUD visibility.
  - Keyboard Shortcuts Guide: modal dialog accessible via `F1` or `⌨ Raccourcis` button in the top bar.
  - Selection feedback: `Cmd+A` and `Cmd+D` update the bottom status bar with exact feedback.
  - Local commit: `0428e25` (`feat(rraw-app): add canvas HUD, before/after split view, clipping overlays and keyboard shortcuts guide`).
  - Verified with 161 unit and integration tests passing across 44 test suites, 0 clippy warnings, and Qwen 3.6 35B local audit APPROVED.

- **Live Filmstrip Thumbnail Synchronization & Crop Matching (`rraw-app`)**:
  - Implemented `create_thumbnail_from_preview` in `crates/rraw-app/src/preview.rs` to extract downscaled 140px thumbnails from rendered previews with exact crop bounding box support.
  - Live in-place texture update (`tex.set`) in `crates/rraw-app/src/app.rs` (`update_preview_textures`) so moving any grading slider (exposure, WB, curves, sepia, B&W, vignettes, local masks, crop) updates the filmstrip thumbnail in real time (<0.1ms).
  - Background thumbnail worker race condition prevention: skips overwriting active photo with stale background decodes.
  - Sidecar grading and crop support for raster formats (JPEG/PNG/TIFF) in `generate_fast_thumbnail`.
  - Added unit tests in `crates/rraw-app/tests/thumbnail_tests.rs`.
  - Verified with 150 unit and integration tests passing across 44 test suites, 0 clippy warnings, and Qwen 3.6 35B local audit.

- **Phase 9.2 — Freehand Adjustment Brush Mask, Zoom Support & Viewport Pan Navigation (`rraw-app`)**:
  - Implemented `BrushDab` vector stroke model in `crates/rraw-app/src/state.rs` (`pos [f32; 2]`, `radius`, `flow`, `erase`) and `LocalMaskKind::FreehandBrush` with continuous smoothstep feather falloff, additive/subtractive blending, spatial deduplication (`(radius * 0.25)^2`), and `.rraw.json` sidecar persistence.
  - Interactive Viewport tooling in `crates/rraw-app/src/ui/viewport.rs`:
    - Screen-to-UV mapping normalized across display zoom and pan offsets.
    - Mouse wheel / trackpad scroll zooming under brush cursor while in brush mode.
    - Holding `Space` or middle mouse button temporarily engages the Hand Tool (`Grab` / `Grabbing` cursor) to pan around the zoomed image without painting dabs.
    - Reserved `Space` in darkroom mode to avoid toggling photo selection while brushing.
    - Adaptive radius steps for bracket shortcuts `[` / `]` (`0.001` for ultra-fine, `0.002` for intermediate, `0.01` for coarse).
    - Fixed double-click collision (`!is_tool_active` guard) preventing rapid brush dabbing from resetting zoom to Fit.
    - Live circular cursor ring preview with inner feather ring and center dot.
    - Active brush HUD badge displaying brush name, mode (Peindre/Gomme), zoom percentage, and navigation hints.
  - Inspector UI in `crates/rraw-app/src/ui/inspector.rs` with `+ Pinceau`, `🖌️ Peindre` activation toggle, ultra-fine radius slider (0.001 to 0.35), flow slider, Paint/Erase mode switch, stroke count, and clear strokes button.
  - Verified with 148 unit and integration tests passing across 43 test suites, 0 clippy warnings, and Qwen 3.6 35B local audit.

- **Phase 9.1 — Local Retouching & Masking Suite (Linear/Radial Gradients & Overlays) (`rraw-app`)**:
  - Implemented `LocalMaskKind` (`LinearGradient`, `RadialGradient`) and `LocalMaskLayer` with safe analytical continuous weight evaluation in `crates/rraw-app/src/state.rs`.
  - Multi-layer mask accumulation in preview (`render_preview_from_base`) and export (`render_export_image`) pipelines covering Exposure EV, Contrast, Saturation, and Color Temperature.
  - Interactive UI controls in `DarkroomSection::AiSemanticMasks` allowing unlimited gradient layering (`+ Dégradé linéaire`, `+ Dégradé radial`), coordinates tuning, feathering, inversion, and deletion.
  - Ruby-pink mask overlay HUD with 'O' keyboard shortcut and explicit viewport notification badge (overlay off by default upon mask creation).
  - Sidecar recipe persistence in `.rraw.json` (`SidecarRecipe`).
  - Commit: `94ea97d`.
  - Verified with 146 unit and integration tests passing across 43 test suites, 0 clippy warnings, and Qwen 3.6 35B local audit.

- **Phase 8.1 — Optical Defringe, Creative Post-Crop Vignetting & Sepia Vintage (`rraw-optics`, `rraw-app`)**:
  - Implemented pure safe Rust optical defringing engine in `crates/rraw-optics/src/defringe.rs` (`#![forbid(unsafe_code)]`, zero unwrap/panic) detecting and neutralizing purple ($R > G \land B > G$) and green ($G > R \land G > B$) chromatic aberration fringes along high-contrast luminance edges ($3\times 3$ window) with Rayon multithreading.
  - Implemented continuous **Creative Vignetting (Post-Crop / Artistic)** with configurable amount ($-100 \dots +100$), midpoint ($10 \dots 90$), smoothstep feather ($10 \dots 90$), and shape roundness (continuous ellipse $\leftrightarrow$ circle $\leftrightarrow$ rectangle).
  - Added **`GradingPreset::SepiaVintage`** (*"Sépia Vintage (Virage chaud)"*) emulating antique sulfured prints with warm chocolate shadows, golden cream highlights, 6800K white balance, customized PCHIP curves, and subtle creative vignette.
  - Added UI controls in `DarkroomSection::OpticalCorrections` and full sidecar serialization (`.rraw.json`).
  - Commit: `165ba90`.
  - Verified with 142 unit and integration tests passing across 42 test suites, 0 clippy warnings, and Qwen 3.6 35B local audit.

- **Phase 7 — Advanced Demosaicing & Noise Reduction Suite (`rraw-core`, `rraw-app`)**:
  - Implemented `DemosaicMethod::EdgeAwareAdaptive` (directional horizontal/vertical green gradient interpolation with second-order curvature correction and green-guided chrominance difference reconstruction eliminating zippering artifacts along contrast edges) in `crates/rraw-core/src/demosaic.rs`.
  - Implemented guided chrominance and luminance noise reduction engine (`DenoiseConfig`, `denoise_rgb`, `denoise_rgb_with_wb`) in `crates/rraw-core/src/denoise.rs` with Rayon parallelism and exact inverse YUV recombination.
  - Implemented anti-color-bleed barriers: pre-normalization with white-balance multipliers before YUV estimation, tightened photometric sigma, and continuous highlight attenuation preventing purple fringing bleeding on backlit high-contrast borders.
  - Added Detail & Denoise section in `rraw-app` with live sliders (`color_denoise` default 25%, `luma_denoise` default 0%) and sidecar serialization.
  - Enabled right-click (`PointerButton::Secondary`) and shortcut keys (`\`, `P`) in 1:1 zoom view to inspect original unedited image portion with HUD badge without breaking left-click pan/drag navigation.
  - Commits: `7e81685`, `3b07b4a`.
  - Verified with 137 unit and integration tests passing across 41 test suites, 0 clippy warnings, and Qwen 3.6 35B local audit.

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

## Next Logical Action: Phase 11 — Internationalization (i18n)

In priority order:
1. **Task 11.1 (`rraw-app`)**: Create `crates/rraw-app/src/i18n/` module with typed `Language { Fr, En }` enum and zero-cost compile-time string tables with exhaustive key matching.
2. **Task 11.2 (`rraw-app`)**: Migrate all hard-coded UI labels across `inspector.rs`, `app.rs`, `filmstrip.rs`, `exif.rs`, `shortcuts.rs`, `canvas_hud.rs`, and `histogram.rs` to i18n references.
3. **Task 11.3 (`rraw-app`)**: Add compact `FR | EN` toggle button in the top bar, persist `language` in `~/.config/rraw/layout.json`, and verify live instant switching without application restart.

## Upcoming Roadmap Horizons: Phase 12 & Phase 13

4. **Phase 12 — Batch Recipe Clipboard & Multi-Image Development (`rraw-core` & `rraw-app`)**:
   - In-memory clipboard holding active recipe snapshot for color & tone grading parameters (Exposure, Contrast, White Balance CCT/Tint, Tone Curves, Saturation, Film Emulations/Styles), strictly excluding photo-dependent parameters (Crop, Straighten/Geometry, Local Brush/Gradient Masks, Lens Optics, ISO Denoise).
   - Multi-selection filmstrip batch pasting via `Cmd+Shift+C` / `Cmd+Shift+V` and right-click context menu ("Copier le rendu / Coller le rendu"), immediate thumbnail re-render, sidecar auto-save, and status feedback.
5. **Phase 13 — Application Versioning, Packaging & macOS DMG Installer (`rraw-app` & Distribution)**:
   - Compile-time semantic versioning (`VERSION`), build timestamp, and git commit hash in top bar, "À propos" dialog, and CLI `--version`.
   - Standalone `Rraw.app` bundle and drag-and-drop `.dmg` installer packaging script with custom icon and `/Applications` symlink.

## Repository Completion State

All planned deliverables across Phases 0–10 are implemented, verified by adversarial TDD (161 tests passing workspace-wide, 0 clippy warnings), and passing all pre-commit formatting and clippy gates. Phase 11 (i18n) is the active development horizon.

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

