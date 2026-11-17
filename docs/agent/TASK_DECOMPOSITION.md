# TASK_DECOMPOSITION.md — Phase & Subtask Breakdown

This document decomposes **Rraw** development into atomic, TDD-gated subtasks across Phases 0 through 4. Every subtask requires a failing synthetic test first (`Red`), minimal clean implementation (`Green`), refactoring (`Refactor`), Inquisitor audit, and Gate 3 approval.

---

## Phase 0 — Governance, Documentation & 11-Crate Workspace Initialization

### Task 0.1 — Repository Governance & Specification
- Initialize Git (`main`) and install executable `.git/hooks/pre-commit` (`cargo fmt --all -- --check` + `cargo clippy --workspace --all-targets -- -D warnings`).
- Author `.gitignore`, `rust-toolchain.toml`, `LICENSE` (`GPL-3.0-or-later`), `README.md`, `AGENTS.md`.
- Author `docs/studies/ARCHITECTURE.md` and all 10 governance documents in `docs/agent/`.

### Task 0.2 — Root `Cargo.toml` & 11-Crate Workspace Skeleton
- Configure `[workspace]` (`resolver = "2"`, 11 members under `crates/`), `[workspace.package]` (`edition = "2021"`, `license = "GPL-3.0-or-later"`, `version = "0.1.0"`), and `[workspace.lints.clippy]` (`unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented` set to `"deny"`).
- Create all 11 crates (`9` libs + `rraw-cli` + `rraw-app`) with `[lints] workspace = true`.
- Enforce `#![forbid(unsafe_code)]` on 10 crates and `#![deny(unsafe_code)]` on `rraw-macos-edr`.
- Implement foundational contract types/traits (`CfaPattern`, `NoiseProfile`, `PhotoMetadata`, `PhotoId`, `Stage0Epoch`, `WhiteBalanceMultipliers`, `ChannelSaturation`, `SurfaceSpec`, `DisplayHeadroomProvider`, `TileDenoiser`, `RawDecoder`) and contract/smoke tests in all 11 crates.
- Verify `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`.

---

## Phase 1 — Mathematical Foundations, Pure-Rust Decoding, Colorimetry & CLI

### Task 1.1 — Core Recipe DAG & Deterministic Stage Hashing (`rraw-core`)
- Versioned `.rraw.json` recipe schema (`schema_version`), DAG parameter nodes, deterministic canonical JSON serialization, and isolated stage cache hashes (`hash_l0`, `hash_l1`, `hash_l2`, `hash_display`).
- **TDD Gate**: Changing Stage 5 exposure invalidates `hash_display` while leaving `hash_l0`, `hash_l1`, and `hash_l2` bitwise identical.

### Task 1.2 — Pure-Rust RAW Decoding, `NoiseProfile` & DNG Opcodes (`rraw-decode`)
- Implement `RawDecoder` over `rawler` plus deterministic synthetic RAW generator.
- Extract DNG `NoiseProfile` (`0xc761`) and wavelet MAD fallback estimator.
- Parse binary DNG `OpcodeList1..3` (`WarpRectilinear`, `FixVignetteRadial`, `GainMap` raising `vignette_handled_by_gainmap`).
- **TDD Gate**: Bit-exact decoding of synthetic DNG 1.7 `OpcodeList3` + `GainMap`; MAD recovers synthetic noise $\sigma = 0.02$ within $\pm 2\%$; signed sub-black samples remain $< 0.0$ with zero mean ($|\mu| < 10^{-4}$).

### Task 1.3 — Spectral Colorimetry, Gamut Guard & DRT (`rraw-color`)
- Implement iterative CCT solver and 1D linear mired interpolation.
- Implement separated `ForwardMatrix1/2` and `ColorMatrix1/2` -> Bradford -> `ACEScg` (`AP1` D60) matrix pipelines.
- Implement identity-core `Gamut Guard` (85% `AP1` core identity) and `AgX` / `OpenDRT` parameterized by $H_{\text{active}}$.
- **TDD Gate**: $\Delta E_{00} < 0.01$ on all 24 analytical ColorChecker patches on both matrix paths; `AgX(x, H)` strictly monotonically increasing ($df/dx > 0$) and bounded by $H$ on $x \in [0, 100]$.

### Task 1.4 — Reference CPU Pipeline & Golden CLI (`rraw-color` + `rraw-cli`)
- Wire shared CPU reference pipeline: signed black/white normalization -> pre-demosaic WB ($\text{sat}_c = w_c$, no `1.0` clamp) -> RCD demosaic -> Highlight Reconstruction -> colorimetry -> 16-bit PNG export.
- **TDD Gate**: Synthetic Bayer linear luminance ramp up to physical saturation with $w_R = 2.1, w_G = 1.0, w_B = 1.6$ runs through `rraw-cli` with zero false color or magenta halo up to $\text{sat}_c$ ($R = G = B$ after matrix within $10^{-4}$).

---

## Phase 2 — Headless `wgpu` GPU Pipeline, LRU DAG Cache & 120 Hz EDR Viewport

### Task 2.1 — Headless `wgpu` Context & Stages 1–2 WGSL Shaders (`rraw-gpu`)
- Implement `GpuContext::new_headless()`.
- WGSL Stage 1: signed `R32Float` normalization, hot-pixel/PDAF repair, pre-demosaic `clip_mask`, GAT Anscombe À-Trous wavelets.
- WGSL Stage 2: unclamped WB (`sat_c = w_c`), RCD (Bayer) & Markesteijn (X-Trans) demosaicing, chromatic-ratio Highlight Reconstruction.
- **TDD Gate (Headless GPU)**: Synthetic Siemens star PSNR $> 42\text{ dB}$; clipped solar disk on $R$ & $G$ recovers radial gradient above `1.0` up to $\max(w_c)$ without magenta ringing.

### Task 2.2 — LRU DAG Cache, 120 Hz RoI Scrubbing, Mipmaps & Stages 5–7 (`rraw-gpu`)
- Implement VRAM-budgeted LRU cache (`L0`/`L1`/`L2`), 120 Hz RoI + `32px` halo scrubbing mode, `1024×1024` async tiled consolidation, and mipmap pyramid ($L \in \{0, 1, 2\}$).
- WGSL Stages 5–7: `ACEScg` colorimetry, $N_{\text{octaves}} - L$ Laplacian pyramid local contrast, `AgX`/`OpenDRT`, workgroup-reduced GPU histogram/RGB parade/clipping alerts.
- **TDD Gate (Headless GPU)**: Scale-invariance RMSE $< 0.5\%$ between `1:1` ($L=0$) and `1:4` ($L=2$); 5th photo into a 2-photo LRU cache cleanly evicts oldest VRAM buffers.

### Task 2.3 — macOS EDR Bridge & Dual-Pass Compositor (`rraw-macos-edr` & `rraw-app`)
- Implement safe `DisplayHeadroomProvider` over audited `objc2` (`NSScreen.maximumExtendedDynamicRangeColorComponentValue`, `CAMetalLayer` `extendedLinearDisplayP3`).
- Maintain fixed `Rgba16Float` surface format across display transitions; implement dual-pass SDR UI + EDR viewport compositor in `rraw-app`.
- **TDD Gate**: Mock `DisplayHeadroomProvider` transition `H_active = 4.0` -> `1.0` keeps `Rgba16Float` fixed, clamps viewport output to `1.0`, and preserves `egui` UI invariance.

---

## Phase 3 — 4-Tier Optical Engine & Regularized Capture Sharpening

### Task 3.1 — 4-Tier Optical Resolver, `256×256` UV LUT & Fused `WARP` (`rraw-optics` & `rraw-gpu`)
- Optical MakerNotes (Sony `0x2010`, Nikon `0xc7d5`, Fuji, M4/3; Canon `.CR3` Tier 2 fallback), `Lensfun` XML (`PTLens`, `Poly3`, `Poly5`), local `.lcp` (`Brown-Conrady`), `256×256` per-channel UV LUTs, and Stage 4 Catmull-Rom `WARP` shader with fused radial vignetting (skipped if `vignette_handled_by_gainmap == true`) and `max(rgb, 0.0)`.
- **TDD Gate**: Known pincushion + lateral R/B CA + $\cos^4(\theta)$ vignette rectified to $< 0.05\text{ px}$ error, zero `< 0.0` outputs, and zero over-correction when `vignette_handled_by_gainmap == true`.

### Task 3.2 — Regularized Richardson-Lucy Capture Sharpening (`rraw-optics` & `rraw-gpu`)
- Damped Richardson-Lucy (5–10 iterations, $\epsilon = 10^{-6}$, `I >= 0`), radial $\text{PSF}_r$, `NoiseProfile`-indexed $M_{\text{edge}}$ mask, and local min/max deringing.
- **TDD Gate**: Combined `0.0` floor + Gaussian edge ($\sigma = 1.1\text{ px}$) + Poisson-Gaussian noisy flat region yields zero `NaN`/`Inf`, $\ge +35\%$ MTF50 improvement, **$< +1\%$ noise variance increase in flat region**, and zero ringing.

---

## Phase 4 — CoreML AI (`504×504` Tiling), Local Masks, DAM & HDR Export

### Task 4.1 — `504×504` Phase-Preserving Tiling, FP16 Conditioning & `ort` CoreML (`rraw-ai`)
- `504×504` slicer/blender with `48px` $\cos^2$ overlap, FP16 Anscombe/log dynamic compression, `(PhotoId, Stage0Epoch)` cooperative cancellation, `ArcSwap<CachedStage0>` handoff, and `ort` CoreML EP.
- **TDD Gate**: $2000 \times 1500$ X-Trans $6 \times 6$ pattern processed through a phase-sensitive $(x \bmod 6, y \bmod 6)$ operator reconstructs across all tile seams with `f32` error $< 10^{-6}$.

### Task 4.2 — Post-Warp `[0, 1]²` Local Geometric & Parametric Masks (`rraw-gpu` & `rraw-app`)
- Linear/radial gradients in `[0, 1]²` post-warp/post-crop coordinates and Luminance/Chroma/Hue parametric masks evaluated in `ACEScg` at 120 Hz.

### Task 4.3 — Async SQLite DAM Index, Non-Destructive `.xmp` & Browser (`rraw-storage` & `rraw-app`)
- `rusqlite` async index on `PhotoMetadata`, non-destructive `.xmp` and `.rraw.json` sidecars, disk thumbnail mipmap cache, and filesystem browser/filmstrip.
- **TDD Gate**: Round-trip `.xmp` containing `crs:ToneCurvePV2012` and `darktable:history` after modifying `xmp:Rating` and `dc:subject`, verifying third-party XML nodes remain verbatim intact.

### Task 4.4 — Multi-Threaded Batch Exporter & ISO 21496-1 Gain Maps (`rraw-export`)
- Batch export queue for 16-bit TIFF/PNG, JPEG, AVIF (`rav1e`), JXL (`libjxl`), ICC profiles, and ISO 21496-1 HDR Gain Maps (binary `tmap` ISOBMFF / `APP2` MPF + XMP `hdrgm:*`).
- **TDD Gate**: Export $+3\text{ EV}$ HDR image with ISO 21496-1 Gain Map, parse both binary `tmap` and XMP `hdrgm:*`, and verify reconstructed HDR recovers linear highlights up to `8.0` within $< 1\%$.
