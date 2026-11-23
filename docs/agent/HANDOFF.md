# HANDOFF.md — Next Action & Session Continuity

## Next Actionable Objective (Immediate Priority)

- **Phase 22 — Task 22.2 (Audit #4 Priority P1 — State Machine & Enum Hygiene)**:
  - Replace independent boolean flags in `ExportBatchController` (`show_export_dialog`, `show_shortcuts_dialog`, `show_about_dialog`, `export_batch_selection`) and `ViewportController` (`is_crop_editing`, `horizon_tool_active`, `parallel_tool_phase`, `active_brush_mask_idx`) with mutually-exclusive typed enums.
  - Eliminate duplicate state fields between `RrawApp` and `AppLayoutConfig`.
  - Exclude ephemeral UI fields (`lens_search_query`, `defringe_preview_mask`, `optical_tab`) from `<photo>.rraw.json` sidecar serialization in `OpticsGradingParams`.

---

## Last Completed Milestone

- **Phase 22 — Task 22.1 Audit #4 Priority P0 Remediation (`rraw-app`) — COMPLETE**:
  - **P0.1 Parallel Single-Buffer Export & Preview Parity**: Implemented `ExportRasterBuffer` (`U8` | `U16`) and Rayon-parallelized `render_export_image_with_depth`, avoiding dual 8-bit/16-bit buffer allocation and unifying pixel math with `render_preview_from_base_profiled`.
  - **P0.2 Asynchronous O(1)-RAM Streaming Timelapse**: Implemented `prepare_timelapse_gradings_from_lumas` and `spawn_background_timelapse_export`, running timelapse assembly on a background worker thread in 2 streaming passes without retaining all decoded frames in RAM.
  - **P0.3 O(1) AABB-Localized Brush Grid**: Implemented `PrecomputedBrushGrid` (512×512 AABB-localized bilinear lookup) and `eval_all_mask_adjustments_with_brush_grids`.
  - **P0.4 Atomic Sidecar Persistence & `.rraw.json` Precedence**: Added `atomic_write_str` (`write -> flush -> sync_all -> rename`) for sidecar and layout persistence, and enforced `.rraw.json` rating precedence over `.xmp` in `load_photo_sidecars`.
  - **Validation & Tests**: Dedicated integration test suite `crates/rraw-app/tests/p0_audit_remediation_tests.rs` (4 tests). 361 workspace tests passing across 80 suites, 0 clippy errors.

---

## Preceding Completed Milestone

- **Phase 21 — Task 21.3 Production Hardening, Release Polish & macOS App Packaging (`rraw-app`, CI / Packaging) — COMPLETE**:
  - **Keyboard Navigation & View Shortcuts**: Implemented `Cmd+O` / `Cmd+Shift+O` (Open Image / Open Folder), `Cmd+E` (Export Dialog), `Cmd+1/2/3` (instant left panel tab switching between Files, Styles, and EXIF Metadata), and `Enter` to confirm export dialog. Restored right inspector panel 100% to Darkroom development without redundancy.
  - **Star Rating Modifier Guards**: Isolated star ratings `0..=5` from Cmd/Ctrl/Alt modifier keys to eliminate shortcut collisions.
  - **Hardened Runtime Packaging**: Updated `scripts/package_macos.sh` with `assets/macos/Entitlements.plist`, codesigning verification, compressed drag-and-drop DMG generation (UDZO), SHA-256 checksums, and Apple Notarization (`--notarize-profile`) / stapling.
  - **Documentation & Tests**: Updated `README.md` with distribution guide and keyboard shortcuts table; added dedicated integration test suite `crates/rraw-app/tests/keyboard_navigation_tests.rs`. 357 workspace tests passing across 79 suites, 0 clippy warnings under `-D warnings`, Inquisitor APPROVED via Qwen 3.6 35B A3B (0 tokens cloud).

---

## Last Completed Milestone

- **Phase 21 — Task 21.2 End-to-End Latency Benchmarks & Stage-by-Stage Profiling (`rraw-core`, `rraw-gpu`, `rraw-app`, `rraw-cli`) — COMPLETE**:
  - **Unified Pipeline Instrumentation**: Created `rraw-core::profiling` defining `PipelineStageId`, `ExecutionBackend`, `StageTiming`, `PipelineTiming`, and `StageTimer` covering all 7 photographic stages.
  - **7-Stage GPU Pipeline Profiler**: Implemented `GpuPipelineProfiler` in `rraw-gpu` executing and benchmarking all 7 GPU compute shaders sequentially or in interactive darkroom viewport mode (Stages 4-7).
  - **Profiled Preview & Decoder Loops**: Added `render_preview_from_base_profiled` and `decode_photo_file_payload_profiled` in `rraw-app`.
  - **Automated CLI Performance Reports**: Added `--profile` / `-P` mode to `rraw-cli` printing ASCII throughput and latency breakdowns with 120 FPS / sub-50ms checks.
  - **Validation & Tests**: 2 dedicated test suites added (`pipeline_latency_benchmarks.rs`, `end_to_end_latency_tests.rs`). 352 workspace tests passing across 78 suites, 0 clippy warnings under `-D warnings`, Inquisitor APPROVED via Qwen 3.6 35B A3B (0 tokens cloud).

---

## Last Completed Milestone

- **Phase 21 — Task 21.1 Memory Footprint Optimization & Zero-Copy Metal Pipeline (`rraw-core`, `rraw-gpu`, `rraw-app`) — COMPLETE**:
  - **Zero-Copy SignedCfaBuffer**: Converted sensor samples to `Arc<[f32]>` with `from_arc` and `samples_arc`, enabling zero-cost $O(1)$ buffer cloning between background decoders, cache entries, and export queues (~181 MB saved per clone on 45.4 MP images).
  - **Zero-Copy BaseRgbPreview**: Wrapped preview RGB samples in `Arc<[[f32; 3]]>` eliminating 545 MB buffer copies on 1:1 image previews.
  - **Unrestricted WGPU Buffer Allocation**: Queried native `adapter.limits()` in `GpuContext::new_headless()`, bypassing WebGPU's 128 MB storage buffer clamp to enable gigabyte-scale buffer allocations on Apple Silicon Metal UMA.
  - **LRU Memory-Budget Eviction**: Enforced 1.5 GB cache ceiling in `RrawApp::insert_cached_photo` with `memory_footprint_bytes()` and underflow-safe `saturating_sub` arithmetic.
  - **Validation & Tests**: 3 dedicated test suites added (`memory_footprint_tests.rs`, `zero_copy_metal_tests.rs`, `memory_optimization_tests.rs`). 341 workspace tests passing across 76 suites, 0 clippy warnings under `-D warnings`, Inquisitor APPROVED via Qwen 3.6 35B A3B (0 tokens cloud).

---

## Last Completed Milestone

- **Phase 20 — Task 20.2 Interactive AI Masking & Real-Time Neural Denoising UI (`rraw-app`, `rraw-gpu`) — COMPLETE**:
  - **GPU Stage 6 Mask Blending**: Connected real CoreML semantic segmentation output (`SemanticMaskSet.to_rgba_interleaved()`) to GPU Stage 6 compute pipeline (`execute_stage6_gpu`).
  - **Real-Time ANE Denoising**: Added ratio-preserving luminance scaling neural denoising in preview and export rendering loops (`apply_ane_neural_denoise_rgb`), strictly maintaining chromaticity and color saturation invariant.
  - **Interactive Darkroom UI**: Added ANE neural denoise toggle, strength slider (0–100%), dynamic hardware detection badge (`⚡ ANE Accéléré` vs `⚙️ Émulation CPU`), and tile processing status in Detail & Denoise and AI Masking sections.
  - **Multilingual i18n**: Added 7 new typed keys in FR, EN, ES catalogs with 453 total typed keys in `crates/rraw-app/src/i18n/key.rs`.
  - **Validation & Tests**: Dedicated tests in `crates/rraw-app/tests/ane_ui_pipeline_tests.rs` (4 tests). 335 workspace tests passing across 73 suites, 0 clippy warnings under `-D warnings`, Inquisitor APPROVED via Qwen 3.6 35B A3B (0 tokens cloud).

---

## Last Completed Milestone

- **Phase 20 — Task 20.1 Hardware ANE Inference Bridge (`rraw-coreml`, `rraw-ai`, `rraw-app`) — COMPLETE**:
  - **Isolated CoreML / ANE Crate**: Created `crates/rraw-coreml` configured with `#![deny(unsafe_code)]` and audited `// SAFETY:` blocks, safely isolating Objective-C runtime CoreML calls (`MLModel`) and preserving `#![forbid(unsafe_code)]` across 10 of 12 workspace crates.
  - **Hardware Capabilities Detection**: Implemented `CoreMlHardwareCapabilities::detect()` querying Apple Silicon host hardware and ANE availability via `sysctlbyname` with safe cross-platform fallback.
  - **504x504 CFA RAW Denoiser**: Implemented `AneRawDenoiser` with Generalized Anscombe Transform (GAT) forward variance stabilization and exact inverse reconstruction.
  - **4-Class Semantic Segmentation**: Implemented `AneSemanticSegmenter` providing probability maps for `Subject`, `Sky`, `Background`, and `Portrait` with strict partition-of-unity invariant `subject + background == 1.0`.
  - **High-Performance Fallback Engine**: Precomputed 5x5 spatial Gaussian weights matrix and Rayon parallelization yielding a 4.3x speedup.
  - **Preview Integration**: Replaced heuristic segmentation in `rraw-app/src/preview/masks.rs` with `AneSemanticSegmenter`.
  - **Validation & Tests**: Dedicated tests in `crates/rraw-coreml/tests/ane_bridge_tests.rs` (8 tests). 331 workspace tests passing across 72 suites, 0 clippy warnings under `-D warnings`, Inquisitor APPROVED via Qwen 3.6 35B A3B (0 tokens cloud).

---

## Last Completed Milestone

- **Phase 19 — Task 19.3 Tier 1 DNG Embedded Opcode Bridge & UI Manual Profile Picker Override (`rraw-optics`, `rraw-app`) — COMPLETE**:
  - **Pure-Rust DNG Opcode Parser**: Extracted `OpcodeList1`, `OpcodeList2`, and `OpcodeList3` (tags `0xC685`..`0xC687`) from TIFF IFD0 and SubIFDs in `crates/rraw-decode/src/opcodes.rs`.
  - **WarpRectilinear to UV LUT**: Direct mathematical bridge converting DNG 1-plane and 3-plane polynomial coefficients into `OpticalCorrectionModel` and evaluating per-channel Stage 4 UV LUT lookup tables (`UvLutGenerator::generate_from_dng_warp_rectilinear`).
  - **Priority 0 DNG Opcode Resolution**: Integrated into `OpticalResolver::resolve_tier` (Tier 1 `Tier1EmbeddedRawOpcodes`) and `LensfunDatabase::match_metadata` (exact match badge `🏷️ Opcode DNG`).
  - **Manual Profile Search & Override Picker**: Interactive UI in Darkroom optical corrections section (`ui/sections/optics.rs`) with real-time fuzzy search across 1,580 lenses, dynamic confidence badges (🏷️ Opcode DNG, 🔍 Profil manuel, 🎯 Calibré Lensfun, 📐 Modèle générique), and 1-click reset button.
  - **Pipeline Integration**: Preview and export pipeline (`preview/pipeline.rs`) honors `grading.optics.lens_profile_override` with full focal length and aperture interpolation.
  - **Multilingual i18n**: Added 7 new typed keys across FR, EN, ES catalogs with 446 total keys in `crates/rraw-app/src/i18n/key.rs`.
  - **Validation & Tests**: Dedicated tests in `optics_tests.rs` and `profile_ui_tests.rs`. 323 workspace tests passing across 69 suites, 0 clippy warnings under `-D warnings`, Inquisitor APPROVED via Qwen 3.6 35B A3B (0 tokens cloud).

---

## Preceding Completed Milestone

- **Phase 19 — Task 19.2 Universal MakerNotes Lens ID Resolution & Multilingual EXIF Localization (`rraw-decode`, `rraw-app`) — COMPLETE**:
  - **Universal MakerNote Parser**: Implemented in `crates/rraw-decode/src/makernotes.rs` extracting vendor MakerNote tags from native RAW and DNG PrivateData (Pentax `0x003F` `LensRec`, Sony `0x0102`, Canon `0x0016`, Nikon `0x0084`).
  - **Zero Manual Effort Automatic Recognition**: Cross-references `rawler::lens::LensResolver` and 15,000+ lens database with camera make, mount (`k-mount`, `e-mount`, etc.), focal length, and aperture to identify third-party lenses (Sigma, Tamron) automatically.
  - **EXIF & Lensfun Auto-Activation**: Automatically populates `PhotoMetadata.lens_model` and `Lens Model` EXIF tag, automatically activating the calibrated Lensfun profile. Added `sanitize_lens_name` to eliminate duplicate manufacturer prefixes.
  - **Multilingual EXIF Localization**: Full dictionary support in `assets/i18n/{fr,en,es}.json` and `crates/rraw-app/src/i18n/` with 62 new typed keys for tags and enum values in FR/EN/ES (zero heap allocation during render loop).
  - **Clean Monoline UI**: Filtered redundant tags already displayed in the top summary card, enforced uniform single-line height with `.truncate()` and full hover tooltips.
  - **Validation & Tests**: Tests in `decode_tests.rs`, `i18n_tests.rs`, `i18n_catalog_tests.rs`, 319 workspace tests passing across 69 suites, 0 clippy warnings under `-D warnings`, Inquisitor APPROVED via Qwen 3.6 35B A3B (0 tokens cloud).

---

## Preceding Completed Milestone

- **Phase 19 — Task 19.1 Embedded & External Lensfun XML Database Engine (`rraw-optics`) — COMPLETE**:
  - **Embedded Official Catalog**: Compressed database of > 1,500 lenses (511 KB gzip) embedded in `crates/rraw-optics/data/lensfun_database.xml.gz`.
  - **Streaming Pure-Rust XML Parser**: Fast, memory-efficient parsing via `quick-xml` without DOM allocation.
  - **Host System Discovery**: Transparent search and merging of external Lensfun directories (`RRAW_LENSFUN_PATH`, `~/Library/Application Support/rraw/lensfun/`, macOS Homebrew, Linux paths).
  - **Dynamic Optical Interpolation**: Continuous interpolation for polynomial distortion (`ptlens`, `poly3`, `poly5`), lateral TCA (`poly3`, `linear`), and radial vignetting (`pa` converted to inverse gain).
  - **Tier 2 Resolver Integration**: Connected to `OpticalResolver` (`Tier2LensfunXml`) with priority fallback chain.
  - **Validation & Tests**: Dedicated test suite in `lensfun_tests.rs` (4 tests), 316 workspace tests passing across 69 suites, 0 clippy warnings under `-D warnings`, Inquisitor APPROVED via Qwen 3.6 35B A3B (0 tokens cloud).

---

## Preceding Completed Milestone

- **Phase 18 — Task 18.2 Asynchronous Directory Indexing & Sub-Millisecond Filmstrip Filtering (`rraw-app`) — COMPLETE**:
  - **LibraryController DAM Integration**: Wired `SqliteDamIndex` via `session_dam: Option<SqliteDamIndex>`.
  - **Zero-Block Folder Open**: UI thread opens/creates `.rraw/cache.db` in < 0.05 ms and offloads rating loading and file scanning to `spawn_directory_indexer`.
  - **Bounded UI Drain**: `drain_dam_indexing_events(128)` prevents frame drops and maintains rock-solid 120 FPS ProMotion.
  - **Bidirectional Sync & SQLite Retry**: Star ratings written via `set_photo_rating_for_path` persist atomically to `.rraw.json` sidecar and `.rraw/cache.db` with exponential backoff retry on `SQLITE_BUSY`.
  - **Validation & Tests**: Dedicated test suite in `session_dam_app_tests.rs` (3 tests), 312 workspace tests passing across 68 suites, 0 clippy warnings under `-D warnings`, Inquisitor APPROVED via Qwen 3.6 35B A3B (0 tokens cloud).

---

## Preceding Completed Milestone

- **Phase 18 — Task 18.1 Session-Mode SQLite Engine & High-Concurrency WAL Pragmas (`rraw-storage`) — COMPLETE**:
  - **Session-Mode Database**: Automatic creation/discovery of `.rraw/cache.db` in currently opened folder via `SqliteDamIndex::open_session_database`.
  - **High-Concurrency WAL Pragmas**: `journal_mode = WAL`, `synchronous = NORMAL`, `temp_store = MEMORY`, `busy_timeout = 5000`.
  - **Sidecar Single Source of Truth**: `.rraw.json` sidecar takes precedence over RAW camera metadata for ratings, color labels, and pick flags (with `.xmp` fallback).
  - **Incremental Sync**: `sync_photo` skips unchanged records based on `max(raw_mtime, sidecars_mtime)` vs `updated_at`.
  - **Fast Sidecar Sync**: `sync_sidecar` updates database ratings in < 0.1 ms without RAW re-decoding.
  - **Validation & Tests**: Dedicated test suite in `session_dam_tests.rs` (5 tests), 309 workspace tests passing across 67 suites, 0 clippy warnings under `-D warnings`, Inquisitor APPROVED via Qwen 3.6 35B A3B (0 tokens cloud).

---

## Preceding Completed Milestone

- **Phase 17 — Sensor Fidelity, Dynamic CFA & True Multi-Format Export — ALL COMPLETE (Tasks 17.1, 17.2, 17.3)**:
  - **Task 17.1 (`rraw-decode`)** : Résolution dynamique du CFA (`rawler` -> `CfaPattern::Bayer` / `XTrans`) et alignement de phase modulo 6 / modulo 2 lors du recadrage. Commit `57c0c45`.
  - **Task 17.2 (`rraw-core`)** : Dématriçage adaptatif X-Trans 6x6 (`demosaic_xtrans_edge_aware`) avec gradient vert directionnel multithreadé et guidance $(R-G)/(B-G)$ (PSNR >= 38 dB sur mire de Siemens). Commit `68f0575`.
  - **Task 17.3 (`rraw-export`, `rraw-app`)** : Encodage réel AVIF (`ravif`), élimination du fallback silencieux PNG via `ExportError::UnsupportedFormat`, intégration dans le modal d'export de l'UI.
  - **Validation Globale** : 304 tests passants across 66 suites, 0 clippy warnings sous `-D warnings`, Inquisitor APPROVED via Qwen 3.6 35B A3B (0 tokens cloud).
  - **Résolution dynamique du motif CFA** : Détection et mapping automatique depuis `raw_image.camera.cfa` (`rawler`) vers `CfaPattern::Bayer(Rggb | Bggr | Grbg | Gbrg)` et `CfaPattern::fujifilm_xtrans()`.
  - **Suppression du RGGB hardcodé** : Élimination du forçage RGGB aux lignes 102/118 de `rraw-decode/src/lib.rs`.
  - **Alignement de phase lors du recadrage** : `align_crop_to_cfa_phase` préserve l'alignement modulo 6 (X-Trans) et modulo 2 (Bayer) avec clamping de sécurité saturé contre tout débordement.
  - **Validation & Tests** : Suite de tests `cfa_resolution_tests.rs` (4 tests), 298 tests passants across 66 suites, 0 clippy warnings, Inquisitor APPROVED via Qwen 3.6 35B A3B (0 tokens cloud).
  - **Task 16.1 (`rraw-app`)** : Worker séquentiel de décodage avec annulation par époque atomique (`DecodeCoordinator`) et cache de miniatures borné à 256 textures (`LruThumbnailCache`). Commit `e38c4fd`.
  - **Task 16.2 (`rraw-app`)** : Décomposition du God Object `RrawApp` en 5 contrôleurs de domaine étanches (`PhotoSession`, `ViewportController`, `LibraryController`, `ExportBatchController`, `BackgroundWorkers`). Commit `c2aa82a`.
  - **Task 16.3 (`rraw-app`)** : Suite de stress haute contention `stress_concurrency_tests.rs` (250 photos, LRU 300, pool 8 threads, batch 200). Commit `f620ab4`.
  - 294 tests passants across 65 suites, 0 clippy warnings, Inquisitor APPROVED via Qwen 3.6 35B A3B (0 tokens cloud).

---

## Preceding Completed Milestone

- **Phase 16 — Task 16.1 Concurrency Hardening, Thread Cancellation & VRAM Eviction Engine (`rraw-app`)**:
  - **Worker de décodage séquentiel & Annulation par époque atomique** : suppression des `std::thread::spawn` non contrôlés ; introduction de `DecodeCoordinator` (`Arc<AtomicU64>`) et du worker dédié `run_decode_worker` qui annule immédiatement tout calcul rendu obsolète par la navigation rapide au clavier.
  - **Plafond VRAM des miniatures** : structure `LruThumbnailCache` fixant la capacité maximale à 256 textures GPU (environ 64 Mo VRAM max) avec éviction automatique LRU.
  - **Purge au changement de dossier** : purge systématique des textures obsolètes dans `open_directory()` et rafraîchissement des tâches en attente.
  - **Validation & Tests** : suite de tests `concurrency_and_cache_tests.rs` (6 tests unitaires/intégration), 284 tests passants across 63 suites, 0 clippy warnings, Inquisitor APPROVED via Qwen 3.6 35B A3B (0 tokens cloud).

---

## Last Completed Milestone

- **Phase 15 — Architectural Hardening: State De-Obesity, UI Modularization & Scalable i18n — ALL COMPLETE**:
  - **Task 15.1 (`rraw-core` & `rraw-app`)**: State De-Obesity & Domain Hierarchy — **COMPLETE** (commit `5b57ead`).
  - **Task 15.2 (`rraw-app`)**: Monolithic File & Function Decomposition — **COMPLETE** (commits `1aec13c`, `ca37430`, `a61361a`).
  - **Task 15.3 (`rraw-app`)**: Scalable External Resource-Based i18n Engine — **COMPLETE** (commit `d157587` + suivi garde-fous / chaînes en dur commit `0ba31c7`).
    - Découplage complet des traductions du code source Rust via catalogues JSON externes sous `assets/i18n/` (375 clés i18n).
    - Élimination de 100% des chaînes en dur restantes dans l'interface et complétion de l'espagnol à 100%.
    - Garde-fous anti-débordement de panneau (`.truncate()`, dimensions proportionnelles, test automatisé de budget de caractères).
    - 276 tests passants across 62 suites, 0 clippy warnings, Inquisitor APPROVED via Qwen 3.6 35B A3B.

---

## Last Completed Milestone

- **Phase 15 — Task 15.3 Follow-up: Hardcoded Strings Elimination, 100% Spanish Completion & Character Budget Guardrails (`rraw-app`)**:
  - Remplacement de toutes les chaînes d'interface résiduelles codées en dur par des clés i18n (`Key::...`).
  - Ajout du dialogue complet des raccourcis clavier en espagnol et traduction de tous les presets et masques sémantiques.
  - Implémentation du test de budget de caractères `test_i18n_character_budget_guardrails` pour prévenir tout bris d'UI.
  - 276 tests passants across 62 suites, 0 warning clippy, audit local Inquisitor APPROVED.

---

## Last Completed Milestone

- **Phase 15 — Task 15.2 Step 3: Preview Pipeline Modularization (`rraw-app`)**:
  - **Décomposition modulaire de `preview.rs`** : décomposition du monolithe de 2 264 lignes en 5 sous-modules spécialisés sous `crates/rraw-app/src/preview/`:
    - `masks.rs` (109 lignes) : Inférence de masques sémantiques IA et évaluation des ajustements locaux.
    - `orientation.rs` (314 lignes) : Transformations d'orientation EXIF, analyseur de tags TIFF/SubIFD et parcours de répertoires.
    - `types.rs` (414 lignes) : `LoadedPhotoView` et exporteur JSON, calcul matriciel calibration boîtier vers sRGB `compute_cam_to_srgb_matrix`, `BaseRgbPreview`, `CachedPhotoEntry`, `BackgroundDecodePayload`.
    - `decoder.rs` (370 lignes) : Génération de vignettes rapides, décodage en arrière-plan et calculs de timelapse deflicker.
    - `pipeline.rs` (1 112 lignes) : Pipeline de rendu couleur darkroom, rastérisation pour export et helper `generate_preview_image`.
    - `mod.rs` (15 lignes) : Ré-exportation exhaustive assurant une rétrocompatibilité ascendante stricte à 100%.
  - **Validation & Tests** : suite de tests `preview_modularization_tests.rs` (5 tests unitaires), 268 tests passants dans le workspace across 61 suites, 0 avertissement clippy, Inquisitor APPROVED via Qwen 3.6 35B A3B, commit `a61361a`.

---

## Last Completed Milestone

- **Phase 15 — Task 15.2 Step 2: Viewport UI Modularization (`rraw-app`)**:
  - **Décomposition modulaire de `viewport.rs`** : décomposition du monolithe de 1 231 lignes en 6 sous-modules sous `crates/rraw-app/src/ui/viewport/`:
    - `crop_overlay.rs`: Boîte de recadrage interactive, poignées de redimensionnement, grille des tiers, pilule flottante de validation et structure d'état `CropToolState`.
    - `perspective_tools.rs`: Outil de niveau d'horizon interactif et correction trapézoïdale à 2 lignes de référence.
    - `hud.rs`: Badges d'état temps réel (Original Avant/Après, Épreuvage écran, Masques locaux, Pinceau).
    - `brush_overlay.rs`: Peinture manuelle interactive et indicateur circulaire du rayon/adoucissement du pinceau via `BrushViewportContext`.
    - `split_compare.rs`: Ligne de division déplaçable Avant/Après.
    - `mod.rs`: Orchestrateur central (479 lignes) avec gestion du zoom/pan, double-clic, barre d'espace et passe-partout.
  - **Encapsulation & Résolution Clippy** : élimination des fonctions à paramètres à rallonge (`clippy::too_many_arguments`) via `CropToolState` et `BrushViewportContext`.
  - **Validation & Tests** : suite de tests `viewport_modularization_tests.rs` (6 tests unitaires), 263 tests passants dans le workspace across 60 suites, 0 avertissement clippy, Inquisitor APPROVED via Qwen 3.6 35B A3B, commit `ca37430`.

---

## Last Completed Milestone

- **Phase 15 — Task 15.2 Step 1: Inspector UI Modularization (`rraw-app`)**:
  - **Décomposition modulaire de `render_darkroom_section`** : décomposition de la fonction de 1 773 lignes en 11 sous-modules sous `crates/rraw-app/src/ui/sections/` (`exposure_tone`, `white_balance`, `detail_denoise`, `optics`, `soft_proofing`, `crop`, `straighten`, `geometry`, `masks`, `timelapse`, `mod`). `inspector.rs` est allégé de 2 089 à 420 lignes (-1 645 lignes).
  - **Principe de moindre privilège appliqué** : chaque vue reçoit uniquement sa sous-structure de domaine (`ToneGradingParams`, etc.) et retourne un `SectionOutput` typé.
  - **Encapsulation du trapèze** : struct `ParallelLinesToolState` pour regrouper les paramètres du tracé de lignes parallèles et satisfaire `clippy::too_many_arguments`.
  - **Validation & Tests** : suite de tests `section_modularization_tests.rs` (10 tests unitaires), 257 tests passants dans le workspace, 0 avertissement clippy, Inquisitor APPROVED via Qwen 3.6 35B.

---

## Last Completed Milestone

- **Phase 15 — Task 15.1: State De-Obesity & Domain Hierarchy (`rraw-core` & `rraw-app`)**:
  - **Décomposition modulaire de `DarkroomGradingState`** : 57 champs plats éclatés en 5 sous-structures de domaine cohésives sous `crates/rraw-app/src/grading/` (`ToneGradingParams`, `ColorGradingParams`, `DenoiseSharpenParams`, `OpticsGradingParams`, `GeometryGradingParams`).
  - **Rétrocompatibilité Serde intégrale** : `SidecarRecipe` utilise `#[serde(flatten)]` pour aplatir les 5 sous-structures à la sérialisation JSON. Les fichiers de recette existants `.rraw.json` continuent de se charger avec une fidélité numérique parfaite et sans collision de clés.
  - **Migration complète des call sites** : mise à niveau de l'ensemble de l'interface et du moteur (`inspector.rs`, `viewport.rs`, `preview.rs`, `export.rs`, `app.rs`, presse-papier, préréglages).
  - **Tests et validation** : suite de tests TDD `state_de_obesity_tests.rs` ajoutée, 247/247 tests passants dans le workspace, 0 avertissement clippy avec `-D warnings`, audit adversarial impitoyable Qwen 3.6 35B APPROVED (0 token cloud consommés).

---

## Last Completed Milestone

- **Filmstrip RAW Filter Persistence Across Sessions (`rraw-app`)**:
  - **Persistance de la préférence de filtre dans `AppLayoutConfig`** : ajout du champ `raw_only_filter: Option<bool>` avec `#[serde(default)]` dans `AppLayoutConfig` et sauvegarde dans `~/.config/rraw/layout.json`.
  - **Prise en compte au démarrage et au changement de dossier** : utilisation de `layout_config.raw_only_filter.unwrap_or(raw_count > 0)` au lieu de réinitialiser aveuglément à `true` en présence de RAW.
  - **Mise à jour interactive** : synchronisation immédiate de `layout_config.raw_only_filter` et appel de `save()` lors du clic sur le badge `📷 RAW` dans le filmstrip ou lors de l'ouverture explicite d'un raster non-RAW (JPG/PNG).
  - **Tests et validation** : tests unitaires ajoutés dans `filmstrip_filter_tests.rs`, 103 tests passants dans `rraw-app`, 0 avertissement clippy, Inquisitor APPROVED via Qwen 3.6 35B (commit `cd840aa`).

- **Optical Corrections UX, Auto-Crop, DNG Sensor Margins & Before/After Reference (`rraw-optics`, `rraw-decode`, `rraw-app`)**:
  - **Désactivation intégrale des corrections sur l'affichage Avant (`raw_original`)** : implémentation de `DarkroomGradingState::untouched_raw()` forçant l'intégralité des corrections à l'état désactivé neutre (profil d'objectif désactivé, distorsion et vignetage à 0%, aberrations chromatiques et defringe désactivés, géométrie/rotation/trapèze à 0, smart lighting à Off, débruitage et netteté à 0). L'Avant représente rigoureusement le capteur brut pur sans aucune altération.
  - **Isolation stricte des modes Profil vs Manuel** : dans `preview.rs` (`render_preview_from_base` et `render_export_image`), séparation étanche via `match grading.optical_tab`. En mode Manuel, le profil automatique est totalement débrayé (plus de profil fantôme ni d'addition invisible par-dessus les curseurs manuels). En mode Profil, seules les corrections du profil détecté s'appliquent.
  - **Recadrage automatique (Auto-crop) sur la distorsion optique** : calcul d'un facteur de zoom géométrique compensateur $opt\_zoom$ résolu numériquement ($Z^3 - Z^2 - k_{dist} = 0$ pour $k_{dist} > 0$ et $1 / (1 + k_{dist} r_x^2)$ pour $k_{dist} < 0$), éliminant à 100% l'apparition de bordures noires incurvées ou de clampage sur les bords lors de la correction optique.
  - **Rognage natif du capteur selon les métadonnées utiles DNG** : dans `rraw-decode::lib.rs`, extraction et rognage des marges de masquage optique du capteur selon `raw_image.crop_area.or(raw_image.active_area)`, aligné sur les frontières paires 2x2 du quad Bayer (élimination des 60 colonnes noires à gauche et 36 à droite sur Pentax KP et autres boîtiers DNG).
  - **Réactivité interactive** : régénération instantanée de l'aperçu au basculement entre les onglets `[ 🎯 Profil d'objectif | 🛠️ Manuel ]` via `needs_preview_update`.
  - **Tests et validation** : validation de la suite complète avec 235 tests passants across 55 test suites, 0 clippy warnings, et audit local impitoyable Qwen 3.6 35B APPROVED.

- **Filmstrip Thumbnail Fidelity & Orientation/Crop Fixes (`rraw-app`)**:
  - **Élimination de la corruption et inversion de vignettes** : suppression de l'aliasing de texture entre `preview_texture` et `thumbnail_textures`. `preview_texture` ne réutilise plus jamais un handle de vignette et réinitialise son état proprement lors de l'ouverture d'un nouveau fichier pour empêcher tout écrasement des vignettes existantes par l'aperçu de la photo précédente pendant le décodage asynchrone.
  - **Maintien strict du ratio d'aspect des vignettes** : dans `filmstrip.rs`, calcul géométrique du rectangle adapté (`fitted_rect`) centré dans le cadre 110x62 avec fond sombre (letterboxing haut/bas pour les panoramiques, pillarboxing gauche/droite pour les portraits 2:3 et carrés 1:1). Élimine 100% des écrasements et étirements.
  - **Parcours complet de l'orientation EXIF multi-IFD et SubIFD (`0x8769`)** : dans `parse_tiff_ifd0_orientation`, chaînage automatique des IFD (IFD0, IFD1...) et inspection de l'Exif SubIFD avec tampon étendu à 256 Ko (`read_exif_orientation`).
  - **Prise en compte systématique du recadrage** : les vignettes générées en tâche de fond ou lors de l'édition interactive appliquent systématiquement `create_thumbnail_from_preview`, conservant le ratio et les coordonnées de crop.
  - **Synchronisation de la navigation clavier et filtrage RAW** : intégration de `raw_only_filter` dans `navigate_relative_with_shift` et activation par défaut à l'ouverture de dossiers contenant des RAW pour éliminer les doublons visuels des exports compagnons (JPG/PNG).
  - Tests validés : `cargo test --workspace` (232 tests passants, 0 clippy warnings), audit local adversarial Qwen 3.6 35B APPROVED.

- **Phase 14.3 — Smart Lighting UI Inspector & Preset Integration (`rraw-app`)**:
  - **Isolation stricte de la vue Avant / Après** : la texture de référence originale (`raw_original`) est forcée avec `smart_lighting.mode = Off` et `intensity = 0.0`. L'Avant représente fidèlement le capteur brut non retouché.
  - **Résolution de la race condition de décodage asynchrone** : garde `is_still_active_target` empêchant un décodage tardif en tâche de fond d'écraser la photo actuellement sélectionnée lors d'une navigation rapide dans le filmstrip.
  - **Filtrage des doublons dans le filmstrip** : puce `[ 📷 RAW (N) ]` affichée dès que des images non-RAW (JPG, PNG exportés) sont présentes dans le dossier pour les masquer en un clic.
  - Integrated `smart_lighting` into `GradingRecipeClipboard` (`from_grading` / `apply_to_grading`), enabling full batch copy/paste across photos in the darkroom and filmstrip (`Cmd+Shift+C` / `Cmd+Shift+V` via Phase 12 model).
  - Added 10 typed i18n keys in French and English (`InspectorSmartLightingTitle`, `InspectorSmartLightingTooltip`, `InspectorSmartLightingModeOff`, `InspectorSmartLightingModeSlight`, `InspectorSmartLightingModeMedium`, `InspectorSmartLightingModeStrong`, `InspectorSmartLightingModeCustom`, `InspectorSmartLightingIntensity`, `InspectorSmartLightingShadowLift`, `InspectorSmartLightingHighlightComp`).
  - Added interactive Smart Lighting control card inside `DarkroomSection::ExposureTone`:
    - Segmented mode buttons: `[ Désactivé | Léger | Moyen | Fort | Manuel ]`
    - Intensity slider: 0 % à 100 % (avec normalisation bidirectionnelle fraction/pourcentage et auto-initialisation à 100% au basculement depuis `Off`)
    - Mode personnalisé : curseurs fins de débouchage des ombres (0 % à 100 %) et de compression des hautes lumières (0 % à 100 %)
  - Re-calibrated photographic dynamic range toe lift curve:
    - Base metrics in stops-below-midtones $u = (\text{mid\_log} - b) / 6.0$ with $3.0 \cdot u^{0.65} \cdot (1 - 0.32u) \cdot \text{black\_taper}$.
    - Rayon de filtre étendu (`8..=96`) et variance $\epsilon = 0.08$ éliminant les démarcations/halos sur les arêtes contrastées.
    - Étalonnage des modes : `Slight` (+0.8 EV), `Medium` (+1.6 EV), `Strong` (+2.5 EV).
    - Synchronized exact matching curve into GPU WGSL shader `stage5_color_laplacian.wgsl`.
  - Wired section active detection (`is_section_active`) and reset to defaults (`reset_section` and `InspectorResetTone`).
  - Added integration test suite `crates/rraw-app/tests/smart_lighting_ui_tests.rs` (3 tests: clipboard copy/paste, section activation/reset in ExposureTone, and i18n completeness).
  - Verified with 229 unit and integration tests passing across 55 suites, 0 clippy warnings, and Qwen 3.6 35B local adversarial audit APPROVED.

- **Phase 14.2 — Pipeline & GPU/CPU Integration (`rraw-gpu`, `rraw-app`)**:
  - Implemented Smart Lighting local tone mapping in Stage 5 compute WGSL shader (`crates/rraw-gpu/src/shaders/stage5_color_laplacian.wgsl`) with scale-invariant Laplacian pyramid base/detail decomposition ($N_{\text{effective}} = N_{\text{octaves}} - L$) and strict chromaticity preservation via AP1 luminance rescaling ($R/Y, G/Y, B/Y$ invariant).
  - Extended `Stage5Params` with `smart_lighting_shadow_lift`, `smart_lighting_highlight_compression`, and `with_smart_lighting` builder in `crates/rraw-gpu/src/stage5.rs`.
  - Expanded `Stage5UniformsGpu` to 48 bytes with 16-byte WGSL alignment (`enable_smart_lighting: u32`, `smart_lighting_shadow_lift: f32`, `smart_lighting_highlight_compression: f32`, `_pad_sl: f32`) and wired into compute dispatch.
  - Added headless GPU integration test in `crates/rraw-gpu/tests/cache_and_stages5_7_tests.rs` asserting chromaticity ratios $R/G$ and $B/G$ preserved within $2 \times 10^{-3}$ and scale-invariance RMSE $< 1.5\%$ between $L=0$ and $L=2$.
  - Added `smart_lighting: SmartLightingParams` to `DarkroomGradingState` and `SidecarRecipe` in `crates/rraw-app/src/state.rs` with backwards-compatible serde defaults.
  - Implemented CPU pre-pass in `render_preview_from_base` and `render_export_image` in `crates/rraw-app/src/preview.rs` generating a 2D luminance gain map via `apply_smart_lighting_acescg` and applying it with exact chromaticity preservation.
  - Added integration test suite in `crates/rraw-app/tests/smart_lighting_pipeline_tests.rs` (4 tests: preview/export execution, sidecar roundtrip, backward compatibility, and chromaticity preservation).
  - Verified with 226 unit and integration tests passing across 54 suites, 0 clippy warnings, and Qwen 3.6 35B local adversarial audit APPROVED.

- **Phase 14.1 — Scene Dynamic Range Analyzer & Local Tone Mapping Core (`rraw-core`, `rraw-color`)**:
  - Implemented `crates/rraw-color/src/smart_lighting.rs` providing:
    - Full scene dynamic range analyzer (`analyze_scene_dynamic_range`) extracting min/max/mean/geometric mean luma, percentiles $p_{0.1} \dots p_{99.9}$, dynamic range in EV, under/over-exposure EV, shadow/highlight clipping ratios.
    - Automatic scene classification: `Backlit` (spatial peripheral vs central luma analysis), `HighDynamicRange` ($> 10.5$ EV), `LowKey`, `HighKey`, `Normal`.
    - Edge-preserving Guided Filter (`self_guided_filter` and fast separable $O(N)$ sliding window `box_filter_2d`) decomposing log-luminance into Base and Detail layers without gradient reversal or haloing.
    - Adaptive halo-free shadow lift (smooth quadratic lift preserving the black floor) and highlight compression (smooth shoulder rolloff) in linear ACEScg AP1.
    - Strict chromaticity preservation: scales RGB channels by $Y_{\text{mapped}} / Y$, mathematically guaranteeing invariant chromaticity ratios $(R/Y, G/Y, B/Y)$.
  - Added `SmartLightingMode` (`Off`, `Slight`, `Medium`, `Strong`, `Custom`) and `SmartLightingParams` (`mode`, `intensity`, `custom_shadow_lift`, `custom_highlight_compression`) to `Stage5GradingParams` in `crates/rraw-core/src/recipe.rs` with backwards-compatible serde defaults.
  - Re-exported module in `crates/rraw-color/src/lib.rs` and integrated Smart Lighting into `CpuReferencePipeline::execute` in `crates/rraw-color/src/pipeline.rs`.
  - Added unit test suite in `crates/rraw-color/tests/smart_lighting_tests.rs` (11 unit/integration tests).
  - Verified with 221 unit and integration tests passing across 53 suites, 0 clippy warnings, and Qwen 3.6 35B local adversarial audit APPROVED. Commit: `3d8c6b7`.

- **Phase 8.3 — Enhanced Optical Inspector UI (`rraw-optics`, `rraw-app`)**:
  - Implemented `defringe_mask_rgb` in `crates/rraw-optics/src/defringe.rs` providing an interactive diagnostic fringe visualization mask highlighting detected purple fringes in bright magenta, green fringes in neon-green, and background pixels in dimmed monochrome ($Y \times 0.35$).
  - Re-exported `defringe_mask_rgb` in `crates/rraw-optics/src/lib.rs`.
  - Added `OpticalCorrectionTab` (`Profile`, `Manual`) and new fields `defringe_threshold: f32` (default 0.08) and `defringe_preview_mask: bool` (default false) to `DarkroomGradingState` and `SidecarRecipe` in `crates/rraw-app/src/state.rs` with backwards-compatible serde defaults.
  - Updated `render_preview_from_base` in `crates/rraw-app/src/preview.rs` to dynamically evaluate `defringe_threshold` and render the defringe preview mask, while strictly isolating `render_export_image` with `show_defringe_mask = false` so export never leaks diagnostic overlays.
  - Reorganized `DarkroomSection::OpticalCorrections` in `crates/rraw-app/src/ui/inspector.rs` with a sleek segmented tab bar `[ 🎯 Profil d'objectif | 🛠️ Manuel ]`, lens detection card with focal/aperture summary, profile modulation sliders (0–200%), manual distortion/CA/defringe sliders, edge contrast threshold slider (1–30%), interactive defringe mask toggle, and comprehensive reset delegating to `reset_section`.
  - Added 6 typed i18n keys for French and English in `crates/rraw-app/src/i18n/mod.rs` (`InspectorLensTabProfile`, `InspectorLensTabManual`, `InspectorParamDefringeThreshold`, `InspectorDefringePreviewMask`, `InspectorDefringePreviewMaskTooltip`, `InspectorLensProfileDisabledNotice`).
  - Added unit test suites in `crates/rraw-optics/tests/defringe_tests.rs` (7 tests) and `crates/rraw-app/tests/profile_ui_tests.rs` (6 tests).
  - Verified with 210 unit and integration tests passing across 52 suites, 0 clippy warnings, and Qwen 3.6 35B local adversarial audit APPROVED. Commit: `ef158e6`.

- **Phase 8.2 — Lens Profile Matching & Metadata Linking (`rraw-optics`, `rraw-app`)**:
  - Implemented optical lens database module (`crates/rraw-optics/src/db.rs`) with curated static catalog of prime and zoom lenses (Sony, Canon, Nikon, Fujifilm, Sigma, Tamron) with calibrated radial polynomial distortion ($k_1, k_2, k_3$), lateral TCA scale factors ($scale_r, scale_b$), and aperture-dependent radial vignetting ($v_1, v_2$).
  - Implemented continuous focal length and aperture interpolation (`LensProfile::interpolate_model`) clamping at zoom limits, interpolating linearly across calibrated focal brackets, and scaling vignetting across f-stops.
  - Implemented fuzzy/normalized lens lookup (`LensDatabase::find_lens`) and metadata matcher (`LensDatabase::match_metadata`) with smooth focal-dependent generic fallback.
  - Implemented strength-scaled resolution (`OpticalResolver::resolve_with_strength`) modulating distortion, TCA, and vignette by user percentage factors (0% .. 200%).
  - Extended darkroom state (`DarkroomGradingState`) and sidecar serialization (`SidecarRecipe`) with `lens_profile_auto`, `lens_distortion_strength`, `lens_vignette_strength`, and `lens_ca_strength` with backward-compatible serde defaults (`default_true_bool`, `default_100_f32`).
  - Integrated profile corrections into preview and export pipelines (`preview.rs`, `ui/export.rs`) with `matched_lens` attached to `BaseRgbPreview`, gracefully falling back to baseline defaults when `matched_lens` is None.
  - Integrated auto lens profile inspector UI in `DarkroomSection::OpticalCorrections` ([inspector.rs](file:///Users/rdespres/Projets/Rraw/crates/rraw-app/src/ui/inspector.rs)) featuring a detected lens status badge (`🎯` / `📐`), profile intensity sliders, and separation from manual fine-tuning controls.
  - Added 9 typed i18n keys for French and English in `crates/rraw-app/src/i18n/mod.rs` (`InspectorLensProfileTitle`, `InspectorLensProfileAuto`, `InspectorLensProfileAutoTooltip`, `InspectorLensProfileDetected`, `InspectorLensProfileGeneric`, `InspectorLensProfileDistortion`, `InspectorLensProfileVignette`, `InspectorLensProfileCa`, `InspectorLensManualTitle`).
  - Added comprehensive test suites: `crates/rraw-optics/tests/profile_matching_tests.rs` (6 tests) and `crates/rraw-app/tests/profile_ui_tests.rs` (4 tests).
  - Verified with 206 unit/integration tests passing cleanly across 52 suites, 0 clippy warnings, and Qwen 3.6 35B local adversarial audit APPROVED.

- **Phase 13.2 — macOS App Bundle & Drag-and-Drop DMG Installer (`assets/macos`, `scripts`, `crates/rraw-app`)**:
  - Implemented macOS Application Bundle assembly pipeline (`scripts/package_macos.sh`) creating `target/dist/Rraw.app` with `Contents/MacOS/rraw-app`, `Contents/Resources/AppIcon.icns`, `Contents/Info.plist`, and `Contents/PkgInfo`.
  - Authored canonical `assets/macos/Info.plist` template specifying `CFBundleName = "Rraw"`, `CFBundleExecutable = "rraw-app"`, `CFBundleIdentifier = "com.rraw.darkroom"`, `LSMinimumSystemVersion = "13.0"`, `NSHighResolutionCapable = true`, `NSSupportsAutomaticGraphicsSwitching = true`, and standard RAW photo document associations (`.dng`, `.arw`, `.cr2`, `.cr3`, `.nef`, `.orf`, `.rw2`, `.raf`, `.pef`).
  - Implemented high-resolution Apple ICNS generation pipeline (`assets/macos/generate_icon.py`) creating master 1024x1024 darkroom squircle icon and compiling `assets/macos/AppIcon.icns` via `iconutil`.
  - Implemented production drag-and-drop DMG packager with `/Applications` symlink, compressed UDZO image creation via `hdiutil`, ad-hoc/runtime codesigning, and SHA-256 checksum generation (`target/dist/Rraw-v0.1.0.dmg`).
  - Added integration test suite `crates/rraw-app/tests/packaging_tests.rs` (4 tests) asserting plist integrity, Apple ICNS magic bytes & size, packaging script execution permissions, and CLI `--help` invocation.
  - Verified with 192 unit and integration tests passing across 39 suites, 0 clippy warnings, and Qwen 3.6 35B local adversarial audit APPROVED.

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

## Next Logical Action: Task 14.2 (`rraw-gpu` & `rraw-app`)

Proceed directly to:
**Phase 14.2 (`rraw-gpu` & `rraw-app`)**:
- Pipeline & GPU/CPU Integration (Real-time WGSL Stage 5 shader execution at 120 FPS or compute pass, scale-invariant downsample factor handling $N_{\text{effective}} = N_{\text{octaves}} - L$, chromaticity preservation avoiding desaturation or hue shifts in recovered shadows/highlights, preview and export integration).
- Followed by **Task 14.3 (`rraw-app`)**: Smart Lighting inspector section with presets and intensity controls.

## Repository Completion State

All planned deliverables across Phases 0–13 are implemented, and Task 14.1 of Phase 14 is complete. Total 221 tests passing workspace-wide, 0 clippy warnings, and pre-commit checks passing.

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

