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
- [ ] **Task 4.3 (`rraw-storage` & `rraw-app`)**: Async `rusqlite` DAM index, namespace-preserving `.xmp` sidecars, disk thumbnail mipmap cache, browser/filmstrip UI
- [ ] **Task 4.4 (`rraw-export`)**: Multi-threaded batch exporter (16-bit TIFF/PNG, JPEG, AVIF, JXL) + ISO 21496-1 HDR Gain Maps (binary `tmap` + XMP `hdrgm:*`)
