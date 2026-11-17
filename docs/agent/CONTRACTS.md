# CONTRACTS.md — Frozen Architectural Interfaces & Mathematical Invariants

This document defines the immutable cross-crate contracts and physical invariants of **Rraw**. Breaking any invariant below requires an approved architectural revision (Gate 1).

---

## 1. Fundamental Domain Types (`rraw-core`)

### 1.1 `CfaPattern`
- Supports `Bayer { origin: BayerOrigin }` ($2 \times 2$ periodicity: `RGGB`, `BGGR`, `GRBG`, `GBRG`) and `XTrans { matrix: [[CfaColor; 6]; 6] }` ($6 \times 6$ Fujifilm periodicity).
- Must expose `period_x(&self) -> usize`, `period_y(&self) -> usize`, and `color_at(&self, x: usize, y: usize) -> CfaColor` (`Red | Green | Blue`).

### 1.2 `NoiseProfile`
- Models per-channel sensor noise in normalized linear space:
  $$\text{Var}(x_{\text{norm}}) = \alpha_c \cdot \max(x_{\text{norm}}, 0.0) + \sigma_{\text{read}, c}^2$$
- Encapsulated via `NoiseProfile::new(...) -> Result<Self, CoreError>` (rejecting negative or non-finite values) with `const fn` getters (`shot_gain_alpha() -> [f32; 3]`, `read_variance_sigma2() -> [f32; 3]`, `source() -> NoiseProfileSource`):
  - `shot_gain_alpha` ($\alpha_c \ge 0.0$, Poisson photon shot-noise slope)
  - `read_variance_sigma2` ($\sigma_{\text{read}, c}^2 \ge 0.0$, electronic read-noise variance)
  - `source` (`DngTag0xc761`, `CameraTable`, or `WaveletMadFallback`)
- Drives BOTH Stage 1 CFA denoising (GAT Anscombe transform) and Stage 3 Capture Sharpening edge-protection threshold $T_{\text{noise}}$.

### 1.3 `PhotoMetadata`
- Canonical immutable metadata record shared by decoding, colorimetry, optics, and DAM storage (`rraw-storage`):
  - Camera make/model, lens model, focal length (mm), aperture ($f/N$), shutter speed, ISO, capture timestamp.
  - Sensor dimensions, active area, per-channel black/white levels, `CfaPattern`, `NoiseProfile`.
  - Color calibration tags (`AsShotNeutral`, `CalibrationIlluminant1/2`, `ColorMatrix1/2`, `ForwardMatrix1/2`, `CameraCalibration1/2`, `AnalogBalance`).
  - Optical flag: **`vignette_handled_by_gainmap: bool`** (true when DNG `OpcodeList2` `GainMap` already corrects lens shading in CFA domain).
  - User rating/culling fields (`rating: u8` in `0..=5`, `pick: PickStatus`, `label: ColorLabel`, `keywords: Vec<String>`).

### 1.4 Concurrency & Cache Epoch Tokens: `(PhotoId, Stage0Epoch)`
- `PhotoId(u64)`: Stable unique identifier for a photo instance in memory/cache.
- `Stage0Epoch(u64)`: Strictly monotonic counter incremented on every Stage 0/1 mutation (black/white point edit, denoise mode switch, hot-pixel threshold, photo reload).
- `Stage0Token { photo_id: PhotoId, epoch: Stage0Epoch }`:
  - `rraw-ai` workers MUST check a cancellation predicate / `CancellationToken` tied to `(PhotoId, Stage0Epoch)` **before every `504×504` tile** and before publishing results.
  - Stale results whose `(PhotoId, Stage0Epoch)` no longer matches the active token MUST be discarded without mutating `Cache L0`.
  - Switching `Cache L0` from `QualityTier::InteractiveWavelet` to `QualityTier::NeuralCoreMl` happens atomically via `ArcSwap<CachedStage0>` only after full-frame RAM assembly and `Queue::write_texture` upload succeed.

---

## 2. Core Trait Boundaries

### 2.1 `RawDecoder` (`rraw-decode`)
```rust
pub trait RawDecoder: Send + Sync {
    fn decode_file(&self, path: &Path) -> Result<DecodedRaw, DecodeError>;
}
```
- Opens RAW files strictly read-only.
- Extracts `PhotoMetadata`, raw CFA mosaic samples, embedded DNG `OpcodeList1..3`, `NoiseProfile` (`0xc761` or wavelet MAD fallback), and `vignette_handled_by_gainmap`.

### 2.2 `TileDenoiser` (`rraw-core` trait, implemented in `rraw-ai`)
```rust
pub struct TileDenoiseRequest<'a> {
    pub cfa_signed: &'a [f32],
    pub width: usize,
    pub height: usize,
    pub pattern: CfaPattern,
    pub noise: &'a NoiseProfile,
    pub token: Stage0Token,
}

pub trait TileDenoiser: Send + Sync {
    fn denoise_cfa(
        &self,
        request: &TileDenoiseRequest<'_>,
        is_cancelled: &(dyn Fn() -> bool + Sync),
    ) -> Result<Vec<f32>, AiError>;
}
```
- **Tile Geometry & `TileRegion` Invariant**:
  - Tile size MUST be **`504 × 504` pixels** with **`48` pixel** cosine-window ($\cos^2$) overlap (`stride = 456`). Both `504`, `48`, and `456` are exact multiples of `6` (common period dividing Bayer `2` and X-Trans `6`), preserving exact CFA phase at tile origin `(0, 0)`.
  - Every `TileRegion { origin_x, origin_y, valid_width, valid_height, pad_right, pad_bottom }` MUST satisfy:
    - `origin_x % 6 == 0 && origin_y % 6 == 0` (never step backward onto unaligned right/bottom sensor edges)
    - `valid_width + pad_right == 504` and `valid_height + pad_bottom == 504`
    - Right/bottom padding (`pad_right`, `pad_bottom`) is filled via phase-preserving mirror/border reflection on `6×6` boundaries when assembling the fixed `504×504` CoreML tensor.
    - Zero dimensions (`width == 0 || height == 0`) return `Err(AiError::InvalidDimensions { width, height })`.
- **FP16 ANE Conditioning**: Normalized CFA values and variance maps MUST be dynamically compressed (Anscombe / $\log_2$ in `[0, 1]`) before FP16 CoreML inference and inverted back to signed `f32` afterward.

### 2.3 `DisplayHeadroomProvider` (`rraw-core` trait, implemented in `rraw-macos-edr`)
```rust
pub trait DisplayHeadroomProvider: Send + Sync {
    fn current_headroom(&self) -> HeadroomSample;
}
```
- `HeadroomSample` (encapsulated via `HeadroomSample::new(...)` with `const fn` getters `active_headroom()` / `potential_headroom()`):
  - `active_headroom()` (`f32`) — Clamped to `>= 1.0` (e.g., `NSScreen.maximumExtendedDynamicRangeColorComponentValue`, `1.0` on SDR monitors, up to `16.0` on Liquid Retina XDR; non-finite inputs fall back to `1.0`).
  - `potential_headroom()` (`f32`) — Panel hardware maximum (`maximumPotentialExtendedDynamicRangeColorComponentValue`, clamped `>= active_headroom()`).
- **Fixed EDR Surface Format Invariant**:
  - `SurfaceSpec::EDR_SURFACE_FORMAT` is **permanently `Rgba16Float`** (`TextureFormat::Rgba16Float`) with `extendedLinearDisplayP3` color space (`wantsExtendedDynamicRangeContent = YES`).
  - Moving a window between an XDR display (`H_active > 1.0`) and an external SDR display (`H_active == 1.0`) MUST NOT recreate or switch the surface texture format away from `Rgba16Float`; only `active_headroom` changes.

---

## 3. Physical Pipeline & Mathematical Invariants

### 3.1 Signed `Cache L0` (`R32Float` / `SignedCfaBuffer`)
- Normalized CFA buffer:
  $$x_{\text{norm}}(p) = \frac{\text{RAW}(p) - B_c}{W_c - B_c} \in \mathbb{R}$$
- `SignedCfaBuffer` encapsulates dimensions, `CfaPattern`, and sample storage behind `SignedCfaBuffer::new(...) -> Result<Self, CoreError>` (rejecting zero/overflow dimensions and length mismatches) and getters `width()`, `height()`, `pattern()`, `samples() -> &[f32]`.
- `normalize_cfa_signed` validates buffer-vs-metadata dimension equality and validates all active CFA channel spans (`0..4` Bayer, `0..3` X-Trans) upfront before normalizing.
- **Strict Non-Clamping Rule**: `Cache L0` MUST store signed `f32` values preserving negative samples ($x_{\text{norm}} < 0.0$) produced by electronic read noise around black. Clamping at `0.0` in Stage 1 is forbidden.
- Carries pre-demosaic `clip_mask` ($\mathbb{I}[x_{\text{norm}}(p) \ge 0.999]$) and `NoiseProfile`.

### 3.2 Pre-Demosaic White Balance Saturation Ceiling (`sat_c = 1.0 * w_c`)
- Given validated strictly positive finite white-balance multipliers $\mathbf{w} = (w_R, w_G, w_B)$ (`WhiteBalanceMultipliers::new` / `ChannelSaturation::from_wb` returning `Result<..., CoreError>`):
  - **Strict Non-Clamping Rule**: Pixels MUST NOT be clamped at `1.0` after multiplication by $\mathbf{w}$.
  - Per-channel physical saturation threshold becomes:
    $$\text{sat}_c = 1.0 \times w_c \quad \forall c \in \{R, G, B\}$$
  - Highlight Reconstruction (`HLR`) uses $\text{sat}_c$ (flagging channel saturation at $I_c \ge 0.99 \cdot \text{sat}_c$ or via `clip_mask`) to reconstruct clipped channels up to $\max_c(\text{sat}_c)$ without magenta color cast.

### 3.3 Positive Half-Space Clamping at Stage 3 Entry & Stage 4 Exit
- Richardson-Lucy deconvolution requires non-negative signal inputs: $I^{(0)}(x) = \max(I_{\text{HLR}}(x), 0.0)$ with $\epsilon = 10^{-6}$ denominator protection.
- Stage 4 Catmull-Rom bicubic warping clamps negative interpolation lobes via $\max(\text{rgb}, 0.0)$ prior to Stage 5 `ACEScg` colorimetry.

### 3.4 Double-Vignette Prevention (`vignette_handled_by_gainmap`)
- Radial vignetting fused inside the Stage 4 `WARP` shader MUST execute **if and only if** `vignette_handled_by_gainmap == false`.
- When `vignette_handled_by_gainmap == true` (DNG `GainMap` already applied in Stage 1), Stage 4 MUST skip radial vignette scaling.

### 3.5 DNG Dual-Illuminant Colorimetry (`ForwardMatrix` vs `ColorMatrix`)
- CCT $T_{\text{K}}$ (`f64`, strictly finite and $> 0.0$, validated by `kelvin_to_mireds -> Result<f64, ColorError>`) and mireds $M = 10^6 / T_{\text{K}}$ are solved iteratively from `CameraNeutral`, then calibration matrices are interpolated linearly in 1D mireds (clamped to $[M_2, M_1]$ regardless of whether $M_1 > M_2$ or $M_1 < M_2$, returning `0.5` when $M_1 \approx M_2$ and rejecting non-finite inputs).
- **Path A (`ForwardMatrix1/2` present)**: $\mathbf{FM}(M)$ takes white-balanced RGB directly to `CIE XYZ D50`, then Bradford adapts `D50 -> D60`, then converts `XYZ D60 -> ACEScg (AP1)`.
- **Path B (`ColorMatrix1/2` fallback)**: Undo $\text{diag}(\mathbf{w})^{-1}$, invert $(\mathbf{AB} \cdot \mathbf{CC}(M) \cdot \mathbf{CM}(M))^{-1}$ to obtain scene `XYZ` under $\text{WP}_{\text{scene}}(M)$, then Bradford adapt $\text{WP}_{\text{scene}}(M) \to \text{D60}$, then convert `XYZ D60 -> ACEScg (AP1)`.
- **Gamut Guard**: Strictly identity ($\mathbf{x} \mapsto \mathbf{x}$) inside the inner 85% core of `AP1` (covering 100% of `sRGB`, `Display P3`, and all 24 ColorChecker patches with $\Delta E_{00} < 0.01$).

### 3.6 Scale Invariance & Coordinate Frames
- **Mipmap Scale Invariance**: For `Cache L1` mipmap level $L = \log_2(\text{downsample\_factor}) \in \{0, 1, 2\}$, Stage 5 Laplacian pyramid executes $N_{\text{effective}} = N_{\text{octaves}} - L$ octaves.
- **Local Mask Coordinates**: All geometric masks (linear/radial gradients) are parameterized in `[0, 1]²` **post-warp, post-crop Stage 4 output coordinates**.

### 3.7 VRAM LRU Budget & 120 Hz RoI Scrubbing
- VRAM cache enforces a hard cap (default `min(2048 MB, 25% unified memory)`), evicting older photos strictly via LRU so only the active photo retains full `L0`/`L1` residency.
- While `is_scrubbing == true` on Stage 1–3 sliders, GPU compute runs only over the visible viewport RoI + `32px` halo (< 2.5 ms target), consolidating full `1:1` + mipmap `Cache L1` asynchronously in `1024×1024` tiles upon release/debounce.
