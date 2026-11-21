# SESSION_STATE.md — Live Repository State

> Keep this file strictly synchronized with verified workspace state, test counts, and commit history. Never claim a check passed unless executed after the last edit.

## Current Status

- **Active Phase**: Phase 13 — Application Versioning, Packaging & macOS DMG Installer
- **Active Tasks**:
  - Toolbar UX Refinement (`rraw-app`): **Complete** (Suppression de l'en-tête `RRAW`, migration du sélecteur `FR | EN` dans la barre d'état inférieure droite, ruban supérieur commençant directement sur les actions photo, commit `68ffca5`)
  - Task 13.1 (`rraw-app` & Workspace): **Complete** (Semver `VERSION`, `VERSION_TAG`, `LONG_VERSION`, build date et git commit hash exposés à la compilation, affichage discret en barre d'état, modale "À propos", CLI `--version`, commits `b29cad9`, `68ffca5`)
  - Task 13.2 (Packaging & Distribution): **Complete** (macOS App Bundle `Rraw.app`, `assets/macos/Info.plist`, HiDPI `AppIcon.icns`, script `scripts/package_macos.sh`, codesigning ad-hoc/runtime, installeur DMG drag-and-drop avec lien `/Applications`, test suite `packaging_tests.rs`, Inquisitor APPROVED, commit `6729fa6`, CI green 192 tests across 39 suites)
  - Next: Phase 14 — Smart Lighting & Adaptive Dynamic Range (Task 14.1 Scene Dynamic Range Analyzer & Local Tone Mapping Core)
  - Phase 11 Internationalization (`rraw-app`): **Complete** (Local audit APPROVED via Qwen 3.6 35B, CI green 174 tests across 47 suites, 0 clippy warnings)
  - Task 11.1 (`rraw-app`): **Complete** (Typed zero-allocation string tables, `Language { Fr, En }`, `Key` 103 variants, compile-time exhaustive matching, module `crates/rraw-app/src/i18n/`)
  - Task 11.2 (`rraw-app`): **Complete** (Migrated UI labels across top bar, inspector, filmstrip, EXIF, explorer, histogram, HUD; comprehensive test suite asserting all keys resolve to non-empty strings)
  - Task 11.3 (`rraw-app`): **Complete** (Compact `FR | EN` toggle in top bar, persisted in `layout.json` via `#[serde(default)]`, instant live switch without restart)
  - Task 0.1 & Task 0.2: **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized)
  - Task 1.1 (`rraw-core`): **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized, commit `acedebb`)
  - Task 1.2 (`rraw-decode`): **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized, commit `8fa0a80`)
  - Task 1.3 (`rraw-color`): **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized, commit `bed1a7f`)
  - Task 1.4 (`rraw-cli`): **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized, commit `486aff9`)
  - Task 2.1 (`rraw-gpu`): **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized, commit `4eb20a4`)
  - Task 2.2 (`rraw-gpu`): **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized, commit `8e922ad`)
  - Task 2.3 (`rraw-macos-edr` & `rraw-app`): **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized, commit `7446b08`)
  - Task 3.1 (`rraw-optics` & `rraw-gpu`): **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized, commit `71635cc`)
  - Task 3.2 (`rraw-optics` & `rraw-gpu`): **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized, commit `cd5474f`)
  - Task 4.1 (`rraw-ai`): **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized, commit `28597cd`)
  - Task 4.2 (`rraw-gpu` & `rraw-app`): **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized, commit `e836632`)
  - Task 4.3 (`rraw-storage` & `rraw-app`): **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized, commit `c4d4abb`)
  - Task 4.4 (`rraw-export`): **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized, commit `a3ee723`)
  - Task 5.1 (`rraw-app`): **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized, commit `015d27c`)
  - Modular UI & App Refactoring (`rraw-app`): **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized, commit `ac0a8ea` — 5,772 lines modularized into `state.rs`, `preview.rs`, `shell.rs`, `ui/*`)
  - Phase 6.1 (Interactive PCHIP Tone Curves): **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized, commit `64921a7`)
  - App Tone Curves UI, Rayon 60 FPS Preview, Raster & UX: **Complete** (CI green, Gate 3 authorized, commit `d75eafc`)
  - GitHub Actions CI Workflow: **Complete** (`macos-13` runner, commit `551db83`)
  - Tone Curves Mathematical Hardening & Zero-Allocation LUT: **Complete** (Inquisitor APPROVED, commit `00ab991`)
  - App In-Place Preview Slicing & Continuous Drag UX: **Complete** (Inquisitor APPROVED, commit `99bd8a7`)
  - Phase 6.1b.1 Tone Curves Ergonomics, Sqrt Histogram & Readout: **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized, commit `4b0c724`)
  - Phase 6.1b.2 WGSL Stage 5 GPU Tone Curves Integration: **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized, commit `e084390`)
  - Phase 6.2 Extended Film Emulation & Creative Presets: **Complete** (Inquisitor APPROVED, CI green, Gate 3 authorized, commit `a607876`)
  - Phase 6.2b GPU WGSL 3D CLUT & User `.cube` Import: **Complete** (Inquisitor APPROVED, CI green 106 tests, Gate 3 authorized, commit `bd55977`)
  - Phase 6.3 CoreML AI Semantic Segmentation Masks: **Complete** (Inquisitor APPROVED, CI green 117 tests across 37 suites, commits `8dc9095`, `245aa56`, `af92b80`)
  - Phase 6.4 Color Management & Soft Proofing (ICC Profiles & Out-of-Gamut Alerts): **Complete** (Inquisitor APPROVED, CI green 122 tests across 38 suites, commit `d46b2b8`)
  - Phase 6.5 Timelapse, Keyframe Ramping & Temporal Deflicker Engine (`rraw-export` & `rraw-app`): **Complete** (Inquisitor APPROVED, CI green 127 tests across 39 suites, commit `568540a`)
  - Full-Resolution 1:1 CFA Export Demosaicing & Sensor Dimensions Fix: **Complete** (commit `65568a0`)
  - Task 7.1 (`rraw-core` & `rraw-app`): **Complete** (Inquisitor APPROVED, CI green 132 tests across 40 suites, local audit APPROVED via Qwen 3.6 35B)
  - Task 7.2 & 7.3 (`rraw-core` & `rraw-app`): **Complete** (Inquisitor APPROVED, CI green 136 tests across 41 suites, local audit APPROVED via Qwen 3.6 35B, right-click zoom before/after & live denoise engine, commit `7e81685`)
  - Chroma Denoise Color-Bleed & WB Barrier Fix: **Complete** (Local audit APPROVED via Qwen 3.6 35B, CI green 137 tests across 41 suites, commit `3b07b4a`)
  - Optical Defringe, Creative Vignette & Sepia Preset: **Complete** (Local audit APPROVED via Qwen 3.6 35B, CI green 144 tests across 43 suites, commit `165ba90`)
  - Phase 9 Local Retouching & Masking Suite (Linear/Radial Gradients & Freehand Brush): **Complete** (Local audit APPROVED via Qwen 3.6 35B, CI green 150 tests across 44 suites, commits `94ea97d`, `d15a0f1`, `669b748`)
  - UI Overhaul Step 1 (Darkroom Palette Categories, Active Filter, Real-time Search & Clean Sidebar Panels): **Complete** (Local audit APPROVED via Qwen 3.6 35B, CI green 151 tests across 44 suites, zero third-party brands in history, commit `c4487d9`)
  - UI Overhaul Step 1 Refinements (Panel Width Locking 360px/320px, Auto-Unrolling, Multi-line Controls Stacking, Glyph Tofu Fix & Layout Persistence): **Complete** (Local audit APPROVED via Qwen 3.6 35B, CI green 152 tests across 44 suites, commit `80e97a4`)
  - French Terminology Harmonization (Histogram RGB, Complete French Photography Nomenclature & Label Consistency): **Complete** (Local audit APPROVED via Qwen 3.6 35B, CI green 152 tests across 44 suites, commit `b55d6d3`)
  - UI Overhaul Step 2 (Pro Viewport HUD, Split Comparison, Clipping Overlays, Collapsible Filmstrip & Shortcuts Modal): **Complete** (commit `0428e25`, 161 tests across 44 suites, 0 clippy warnings, Qwen 3.6 35B local audit APPROVED)
  - Task 8.1 Optical Defringe, Creative Vignetting & Sepia Vintage (`rraw-optics` & `rraw-app`): **Complete** (Local audit APPROVED via Qwen 3.6 35B, CI green 142 tests across 42 suites, commit `165ba90`)
  - Phase 9.1 Local Retouching & Masking Suite (Linear/Radial Gradients & Overlays): **Complete** (Local audit APPROVED via Qwen 3.6 35B, CI green 146 tests across 43 suites, commit `94ea97d`)
  - Phase 9.2 Freehand Adjustment Brush Mask (Vector Dabs, Zoom Retouching & Viewport Pan): **Complete** (Local audit APPROVED via Qwen 3.6 35B, CI green 148 tests across 43 suites, commit `d15a0f1`)
  - Live Filmstrip Thumbnail Synchronization & Crop Matching: **Complete** (Local audit APPROVED via Qwen 3.6 35B, CI green 150 tests across 44 suites)
  - Phase 12 Batch Recipe Clipboard & Multi-Image Development (`rraw-core` & `rraw-app`): **Complete** (Local audit APPROVED via Qwen 3.6 35B, CI green 165 tests across 45 suites, 0 clippy warnings)
  - Escape Key Modal & Tool Dismissal Hierarchy & Text Input Focus Safety (`rraw-app`): **Complete** (Local audit APPROVED via Qwen 3.6 35B, CI green 169 tests across 46 suites, 0 clippy warnings)
- **Git Branch**: `main`
- **Remote**: `git@github.com:rdespres87/rraw.git` (Tracking)
- **Last Approved Gate**: Gate 3 (Authorized on 2026-11-19)

## Workspace Structure Verified

- 11 Cargo workspace members under `crates/`:
  - `rraw-core` (`lib`, `#![forbid(unsafe_code)]`)
  - `rraw-decode` (`lib`, `#![forbid(unsafe_code)]`)
  - `rraw-optics` (`lib`, `#![forbid(unsafe_code)]`)
  - `rraw-color` (`lib`, `#![forbid(unsafe_code)]`)
  - `rraw-gpu` (`lib`, `#![forbid(unsafe_code)]`)
  - `rraw-ai` (`lib`, `#![forbid(unsafe_code)]`)
  - `rraw-storage` (`lib`, `#![forbid(unsafe_code)]`)
  - `rraw-export` (`lib`, `#![forbid(unsafe_code)]`)
  - `rraw-macos-edr` (`lib`, `#![deny(unsafe_code)]`)
  - `rraw-cli` (`bin`, `#![forbid(unsafe_code)]`)
  - `rraw-app` (`bin`, `#![forbid(unsafe_code)]`)

## Known Open Items

- Known Open Items:
  - Phase 5 GUI Enhancements:
    - Left panel layout: strictly ordered (1. `📁 Explorateur de dossiers` at the top, 2. `🎨 Styles & Préréglages` with dropdown combobox + reset, 3. `📋 Données EXIF` clear striped summary table, comprehensive decoded tag inspection with live search filter, and JSON copy/export, 4. `📊 Histogramme Waveform RGB` at the bottom).
    - Explorer usability & default directory: defaults to user's Desktop (`~/Desktop`), interactive breadcrumb navigation bar, and clean `📁 .. (Dossier parent)` entry at the top of the directory listing.
    - Filmstrip scrollbar clearance: card height resized (104px) and 14px buffer space added so the horizontal scrollbar sits completely below the cards and never covers the rating stars.
    - Classical multi-selection: click (single select/open), Cmd/Ctrl+Click (toggle), Shift+Click (range selection), Cmd+A (select all), Cmd+D / Escape (clear), Space (toggle active photo), Shift+Arrow (range extension).
    - Photo switching performance (instant visual feedback): immediate display of thumbnail texture in frame 0 while background decode loads, bounding downsampling width to 1600 (3x faster), prefetch 2-ahead and 2-behind, and expanded MRU cache to 30 photos.
    - Embellished Histogram: OLED rounded card, sqrt scaling to reveal midtones, glowing RGB curves, quartile zone guides (Noirs, Ombres, Exposition, Hautes Lum.), clipping badges.
    - Crop & Geometry: 120 FPS vector overlay, aspect ratio presets, Enter/double-click commit, independent enable/disable toggles.
    - Batch Export: configurable subfolder (`Export`), suffix (`_rraw`), formats (JPEG quality, PNG 8/16, TIFF 8/16), auto-closing dialog.
- Total tests: 174 unit/integration tests passing cleanly across 47 test suites.

## Last Observed Verification Run

- `./.git/hooks/pre-commit` (`cargo fmt --all -- --check` + `cargo clippy --workspace --all-targets -- -D warnings`): passed (exit `0`).
- `cargo test --workspace`: passed (`174` unit/integration tests across all `11` crates; `0` failed, `0` ignored).
