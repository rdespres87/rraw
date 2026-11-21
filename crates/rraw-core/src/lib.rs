#![forbid(unsafe_code)]

//! Core domain types, mathematical invariants, and cross-crate traits for Rraw.

use std::fmt;

use thiserror::Error;

pub mod build_info;
pub mod curves;
pub mod demosaic;
pub mod denoise;
pub mod recipe;

pub use build_info::*;
pub use curves::*;
pub use demosaic::*;
pub use denoise::*;
pub use recipe::*;

/// Typed error returned by `rraw-core` invariant constructors and validators.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum CoreError {
    /// Image or buffer dimensions were zero or overflowed element count.
    #[error("invalid buffer dimensions: {width}x{height}")]
    InvalidDimensions { width: usize, height: usize },
    /// Sample slice length did not match `width * height`.
    #[error("buffer length mismatch: expected {expected} samples, got {actual}")]
    BufferLengthMismatch { expected: usize, actual: usize },
    /// A physical parameter violated finiteness or positivity constraints.
    #[error("invalid physical value for `{field}`: {value}")]
    InvalidPhysicalValue { field: &'static str, value: f32 },
    /// Recipe serialization failed.
    #[error("recipe serialization error: {0}")]
    SerializationError(String),
    /// Recipe deserialization failed.
    #[error("recipe deserialization error: {0}")]
    DeserializationError(String),
    /// Recipe schema version is not supported.
    #[error("unsupported recipe schema version: expected {expected}, got {actual}")]
    UnsupportedSchemaVersion { expected: u32, actual: u32 },
    /// Invalid hex string for stage hash.
    #[error("invalid stage hash hex string: {0}")]
    InvalidStageHashHex(String),
}

/// Color filter channel at a sensor photosite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CfaColor {
    Red,
    Green,
    Blue,
}

impl CfaColor {
    /// Canonical channel index (`Red = 0`, `Green = 1`, `Blue = 2`).
    #[must_use]
    pub const fn channel_index(self) -> usize {
        match self {
            Self::Red => 0,
            Self::Green => 1,
            Self::Blue => 2,
        }
    }
}

/// Canonical 2x2 Bayer mosaic origin arrangements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BayerOrigin {
    Rggb,
    Bggr,
    Grbg,
    Gbrg,
}

/// Color Filter Array (CFA) layout supported across sensor pipelines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CfaPattern {
    Bayer(BayerOrigin),
    XTrans([[CfaColor; 6]; 6]),
}

impl CfaPattern {
    /// Standard Fujifilm 6x6 X-Trans CFA matrix.
    #[must_use]
    pub const fn fujifilm_xtrans() -> Self {
        use CfaColor::{Blue as B, Green as G, Red as R};
        Self::XTrans([
            [G, B, G, G, R, G],
            [R, G, R, B, G, B],
            [G, B, G, G, R, G],
            [G, R, G, G, B, G],
            [B, G, B, R, G, R],
            [G, R, G, G, B, G],
        ])
    }

    /// Horizontal repeating period of the CFA pattern.
    #[must_use]
    pub const fn period_x(&self) -> usize {
        match self {
            Self::Bayer(_) => 2,
            Self::XTrans(_) => 6,
        }
    }

    /// Vertical repeating period of the CFA pattern.
    #[must_use]
    pub const fn period_y(&self) -> usize {
        match self {
            Self::Bayer(_) => 2,
            Self::XTrans(_) => 6,
        }
    }

    /// Returns the CFA color at sensor coordinates `(x, y)`.
    #[must_use]
    pub const fn color_at(&self, x: usize, y: usize) -> CfaColor {
        match self {
            Self::Bayer(origin) => {
                let cx = x & 1;
                let cy = y & 1;
                match (origin, cy, cx) {
                    (BayerOrigin::Rggb, 0, 0) => CfaColor::Red,
                    (BayerOrigin::Rggb, 0, 1) | (BayerOrigin::Rggb, 1, 0) => CfaColor::Green,
                    (BayerOrigin::Rggb, 1, 1) => CfaColor::Blue,
                    (BayerOrigin::Bggr, 0, 0) => CfaColor::Blue,
                    (BayerOrigin::Bggr, 0, 1) | (BayerOrigin::Bggr, 1, 0) => CfaColor::Green,
                    (BayerOrigin::Bggr, 1, 1) => CfaColor::Red,
                    (BayerOrigin::Grbg, 0, 0) | (BayerOrigin::Grbg, 1, 1) => CfaColor::Green,
                    (BayerOrigin::Grbg, 0, 1) => CfaColor::Red,
                    (BayerOrigin::Grbg, 1, 0) => CfaColor::Blue,
                    (BayerOrigin::Gbrg, 0, 0) | (BayerOrigin::Gbrg, 1, 1) => CfaColor::Green,
                    (BayerOrigin::Gbrg, 0, 1) => CfaColor::Blue,
                    (BayerOrigin::Gbrg, 1, 0) => CfaColor::Red,
                    (_, _, _) => CfaColor::Green,
                }
            }
            Self::XTrans(matrix) => matrix[y % 6][x % 6],
        }
    }
}

/// Origin of a sensor `NoiseProfile`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NoiseProfileSource {
    /// Extracted from DNG `NoiseProfile` tag (`0xc761`).
    DngTag0xc761,
    /// Loaded from calibrated camera/ISO model tables.
    CameraTable,
    /// Estimated automatically via high-frequency wavelet MAD fallback.
    WaveletMadFallback,
}

/// Calibrated Poisson-Gaussian sensor noise parameters per RGB channel.
///
/// Models `Var(x_norm) = alpha_c * max(x_norm, 0.0) + sigma_read_c^2`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoiseProfile {
    shot_gain_alpha: [f32; 3],
    read_variance_sigma2: [f32; 3],
    source: NoiseProfileSource,
}

impl NoiseProfile {
    /// Constructs a validated `NoiseProfile`, rejecting negative or non-finite values.
    pub fn new(
        shot_gain_alpha: [f32; 3],
        read_variance_sigma2: [f32; 3],
        source: NoiseProfileSource,
    ) -> Result<Self, CoreError> {
        for value in shot_gain_alpha {
            if !value.is_finite() || value < 0.0 {
                return Err(CoreError::InvalidPhysicalValue {
                    field: "shot_gain_alpha",
                    value,
                });
            }
        }
        for value in read_variance_sigma2 {
            if !value.is_finite() || value < 0.0 {
                return Err(CoreError::InvalidPhysicalValue {
                    field: "read_variance_sigma2",
                    value,
                });
            }
        }
        Ok(Self {
            shot_gain_alpha,
            read_variance_sigma2,
            source,
        })
    }

    /// Per-channel photon shot-noise gain (`alpha_R`, `alpha_G`, `alpha_B`).
    #[must_use]
    pub const fn shot_gain_alpha(&self) -> [f32; 3] {
        self.shot_gain_alpha
    }

    /// Per-channel electronic read-noise variance (`sigma_R^2`, `sigma_G^2`, `sigma_B^2`).
    #[must_use]
    pub const fn read_variance_sigma2(&self) -> [f32; 3] {
        self.read_variance_sigma2
    }

    /// Calibration provenance.
    #[must_use]
    pub const fn source(&self) -> NoiseProfileSource {
        self.source
    }

    /// Evaluates signal-dependent variance at a normalized intensity `x_norm`.
    #[must_use]
    pub fn variance_at(&self, color: CfaColor, normalized_signal: f32) -> f32 {
        let idx = color.channel_index();
        let positive_signal = if normalized_signal.is_finite() {
            normalized_signal.max(0.0)
        } else {
            0.0
        };
        self.shot_gain_alpha[idx] * positive_signal + self.read_variance_sigma2[idx]
    }
}

/// Stable identifier for an active photo instance across cache and AI workers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PhotoId(pub u64);

/// Monotonic epoch counter incremented on every Stage 0 / Stage 1 mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Stage0Epoch(pub u64);

impl Stage0Epoch {
    /// Returns the next monotonic epoch token.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

/// Composite concurrency token ensuring AI workers never commit stale tiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Stage0Token {
    pub photo_id: PhotoId,
    pub epoch: Stage0Epoch,
}

/// Active denoising tier backing `Cache L0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QualityTier {
    /// 120 Hz GPU À-Trous wavelet denoiser under Generalized Anscombe Transform.
    InteractiveWavelet,
    /// Asynchronous CoreML / Apple Neural Engine 504x504 tiled neural denoiser.
    NeuralCoreMl,
}

/// Culling pick status stored in DAM index and sidecars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PickStatus {
    Unflagged,
    Pick,
    Reject,
}

/// Color label tag compatible with XMP `xmp:Label`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorLabel {
    None,
    Red,
    Yellow,
    Green,
    Blue,
    Purple,
}

/// Canonical immutable photo metadata extracted during RAW decode.
#[derive(Debug, Clone, PartialEq)]
pub struct PhotoMetadata {
    pub camera_make: String,
    pub camera_model: String,
    pub lens_model: Option<String>,
    pub focal_length_mm: Option<f32>,
    pub aperture_f_number: Option<f32>,
    pub iso: Option<u32>,
    pub width: usize,
    pub height: usize,
    pub black_levels: [f32; 4],
    pub white_levels: [f32; 4],
    pub cfa_pattern: CfaPattern,
    pub noise_profile: NoiseProfile,
    /// Raised when Stage 1 applies a DNG `OpcodeList2` `GainMap`, preventing
    /// redundant radial vignetting correction in Stage 4.
    pub vignette_handled_by_gainmap: bool,
    pub rating: u8,
    pub pick: PickStatus,
    pub label: ColorLabel,
    pub keywords: Vec<String>,
}

/// Signed Stage 1 (`Cache L0`) CFA buffer preserving negative read-noise samples (`< 0.0`).
#[derive(Debug, Clone, PartialEq)]
pub struct SignedCfaBuffer {
    width: usize,
    height: usize,
    pattern: CfaPattern,
    samples: Vec<f32>,
}

impl SignedCfaBuffer {
    /// Creates a signed CFA buffer without clamping values below `0.0`.
    pub fn new(
        width: usize,
        height: usize,
        pattern: CfaPattern,
        samples: Vec<f32>,
    ) -> Result<Self, CoreError> {
        let Some(expected) = width.checked_mul(height) else {
            return Err(CoreError::InvalidDimensions { width, height });
        };
        if expected == 0 {
            return Err(CoreError::InvalidDimensions { width, height });
        }
        if samples.len() != expected {
            return Err(CoreError::BufferLengthMismatch {
                expected,
                actual: samples.len(),
            });
        }
        Ok(Self {
            width,
            height,
            pattern,
            samples,
        })
    }
}

impl Default for SignedCfaBuffer {
    fn default() -> Self {
        Self {
            width: 1,
            height: 1,
            pattern: CfaPattern::Bayer(BayerOrigin::Rggb),
            samples: vec![0.0],
        }
    }
}

impl SignedCfaBuffer {
    /// Sensor buffer width in pixels.
    #[must_use]
    pub const fn width(&self) -> usize {
        self.width
    }

    /// Sensor buffer height in pixels.
    #[must_use]
    pub const fn height(&self) -> usize {
        self.height
    }

    /// CFA mosaic layout of the sensor buffer.
    #[must_use]
    pub const fn pattern(&self) -> CfaPattern {
        self.pattern
    }

    /// Signed normalized CFA sample slice (`< 0.0` preserved).
    #[must_use]
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }
}

/// Pre-demosaic white-balance channel multipliers `(w_R, w_G, w_B)`.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WhiteBalanceMultipliers {
    r: f32,
    g: f32,
    b: f32,
}

impl WhiteBalanceMultipliers {
    /// Neutral 1.0 white-balance multipliers.
    pub const UNITY: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 1.0,
    };

    /// Validates strictly positive finite white-balance multipliers.
    pub fn new(r: f32, g: f32, b: f32) -> Result<Self, CoreError> {
        for (field, value) in [("wb_r", r), ("wb_g", g), ("wb_b", b)] {
            if !value.is_finite() || value <= 0.0 {
                return Err(CoreError::InvalidPhysicalValue { field, value });
            }
        }
        Ok(Self { r, g, b })
    }

    /// Red channel white-balance multiplier.
    #[must_use]
    pub const fn r(self) -> f32 {
        self.r
    }

    /// Green channel white-balance multiplier.
    #[must_use]
    pub const fn g(self) -> f32 {
        self.g
    }

    /// Blue channel white-balance multiplier.
    #[must_use]
    pub const fn b(self) -> f32 {
        self.b
    }
}

impl Default for WhiteBalanceMultipliers {
    fn default() -> Self {
        Self::UNITY
    }
}

/// Per-channel physical saturation ceiling `sat_c = 1.0 * w_c` passed to HLR.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChannelSaturation {
    r: f32,
    g: f32,
    b: f32,
}

impl ChannelSaturation {
    /// Computes validated `sat_c = 1.0 * w_c` without clamping to `1.0`.
    pub fn from_wb(wb: WhiteBalanceMultipliers) -> Result<Self, CoreError> {
        let validated = WhiteBalanceMultipliers::new(wb.r, wb.g, wb.b)?;
        Ok(Self {
            r: 1.0 * validated.r,
            g: 1.0 * validated.g,
            b: 1.0 * validated.b,
        })
    }

    /// Red channel physical saturation ceiling `sat_R = 1.0 * w_R`.
    #[must_use]
    pub const fn r(self) -> f32 {
        self.r
    }

    /// Green channel physical saturation ceiling `sat_G = 1.0 * w_G`.
    #[must_use]
    pub const fn g(self) -> f32 {
        self.g
    }

    /// Blue channel physical saturation ceiling `sat_B = 1.0 * w_B`.
    #[must_use]
    pub const fn b(self) -> f32 {
        self.b
    }

    /// Minimum channel saturation ceiling (`min_c sat_c`).
    #[must_use]
    pub fn min_channel(self) -> f32 {
        self.r.min(self.g).min(self.b)
    }

    /// Maximum channel saturation ceiling (`max_c sat_c`).
    #[must_use]
    pub fn max_channel(self) -> f32 {
        self.r.max(self.g).max(self.b)
    }
}

/// Fixed swapchain surface pixel format contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SurfaceFormat {
    Rgba16Float,
}

/// Display surface specification shared across GPU and macOS EDR layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SurfaceSpec;

impl SurfaceSpec {
    /// The `wgpu::SurfaceConfiguration` format MUST remain permanently fixed
    /// at `Rgba16Float` regardless of SDR/XDR display transitions.
    pub const EDR_SURFACE_FORMAT: SurfaceFormat = SurfaceFormat::Rgba16Float;
}

/// Live and hardware-maximum EDR display headroom values polled from `NSScreen`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeadroomSample {
    active_headroom: f32,
    potential_headroom: f32,
}

impl HeadroomSample {
    /// Normalizes a headroom reading, ensuring `1.0 <= active_headroom <= potential_headroom`.
    #[must_use]
    pub fn new(active_headroom: f32, potential_headroom: f32) -> Self {
        let safe_active = if active_headroom.is_finite() && active_headroom >= 1.0 {
            active_headroom
        } else {
            1.0
        };
        let safe_potential = if potential_headroom.is_finite() && potential_headroom >= safe_active
        {
            potential_headroom
        } else {
            safe_active
        };
        Self {
            active_headroom: safe_active,
            potential_headroom: safe_potential,
        }
    }

    /// Live available headroom (`>= 1.0`, `1.0` on SDR monitors, up to `16.0` on XDR).
    #[must_use]
    pub const fn active_headroom(self) -> f32 {
        self.active_headroom
    }

    /// Panel hardware peak headroom (`>= active_headroom`).
    #[must_use]
    pub const fn potential_headroom(self) -> f32 {
        self.potential_headroom
    }
}

/// Safe abstraction over macOS display headroom queries (`NSScreen` / `CAMetalLayer`).
pub trait DisplayHeadroomProvider: Send + Sync {
    /// Returns the instantaneous SDR/EDR headroom available on the current display.
    fn current_headroom(&self) -> HeadroomSample;
}

/// CoreML / ANE tile geometry preserving both Bayer (2x2) and X-Trans (6x6) CFA phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileGeometry {
    pub tile_size: usize,
    pub overlap: usize,
}

impl TileGeometry {
    /// Default CoreML tile width and height (`504 = 252 * 2 = 84 * 6`).
    pub const TILE_SIZE: usize = 504;
    /// Default cosine overlap margin (`48 = 24 * 2 = 8 * 6`).
    pub const TILE_OVERLAP: usize = 48;
    /// Common phase alignment period dividing both Bayer (`2`) and X-Trans (`6`).
    pub const CFA_PHASE_ALIGNMENT: usize = 6;
    /// Canonical CoreML ANE tile configuration.
    pub const COREML: Self = Self {
        tile_size: Self::TILE_SIZE,
        overlap: Self::TILE_OVERLAP,
    };

    /// Validates a phase-aligned tile geometry where `tile_size > overlap` and both
    /// are positive multiples of [`Self::CFA_PHASE_ALIGNMENT`] (`6`).
    pub fn new(tile_size: usize, overlap: usize) -> Result<Self, AiError> {
        if tile_size == 0
            || overlap >= tile_size
            || !tile_size.is_multiple_of(Self::CFA_PHASE_ALIGNMENT)
            || !overlap.is_multiple_of(Self::CFA_PHASE_ALIGNMENT)
        {
            return Err(AiError::InvalidInput(format!(
                "invalid tile geometry: tile_size={tile_size}, overlap={overlap}"
            )));
        }
        Ok(Self { tile_size, overlap })
    }

    /// Effective tile step (`tile_size - overlap`), guaranteed divisible by `6` when valid.
    #[must_use]
    pub const fn stride(self) -> usize {
        self.tile_size.saturating_sub(self.overlap)
    }

    /// Returns `true` when tile size and overlap preserve the given CFA period.
    #[must_use]
    pub const fn preserves_phase(pattern: &CfaPattern) -> bool {
        Self::COREML.preserves_pattern(pattern)
    }

    /// Returns `true` when this geometry preserves `pattern` periodicity on every tile origin.
    #[must_use]
    pub const fn preserves_pattern(self, pattern: &CfaPattern) -> bool {
        let px = pattern.period_x();
        let py = pattern.period_y();
        self.tile_size > 0
            && self.overlap < self.tile_size
            && self.tile_size.is_multiple_of(px)
            && self.tile_size.is_multiple_of(py)
            && self.overlap.is_multiple_of(px)
            && self.overlap.is_multiple_of(py)
    }
}

impl Default for TileGeometry {
    fn default() -> Self {
        Self::COREML
    }
}

/// Error returned by neural tile denoisers (`rraw-ai`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiError {
    Cancelled(Stage0Token),
    InvalidDimensions { width: usize, height: usize },
    InvalidInput(String),
    Backend(String),
}

impl fmt::Display for AiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled(token) => {
                write!(
                    f,
                    "AI inference cancelled for photo {} epoch {}",
                    token.photo_id.0, token.epoch.0
                )
            }
            Self::InvalidDimensions { width, height } => {
                write!(f, "invalid tile denoiser dimensions: {width}x{height}")
            }
            Self::InvalidInput(msg) => write!(f, "invalid AI denoiser input: {msg}"),
            Self::Backend(msg) => write!(f, "AI denoiser backend failure: {msg}"),
        }
    }
}

impl std::error::Error for AiError {}

/// Borrowed Stage 1 input bundle passed to [`TileDenoiser::denoise_cfa`].
#[derive(Debug, Clone, Copy)]
pub struct TileDenoiseRequest<'a> {
    pub cfa_signed: &'a [f32],
    pub width: usize,
    pub height: usize,
    pub pattern: CfaPattern,
    pub noise: &'a NoiseProfile,
    pub token: Stage0Token,
}

impl SignedCfaBuffer {
    /// Builds a borrowed [`TileDenoiseRequest`] referencing this signed CFA buffer.
    #[must_use]
    pub fn as_denoise_request<'a>(
        &'a self,
        noise: &'a NoiseProfile,
        token: Stage0Token,
    ) -> TileDenoiseRequest<'a> {
        TileDenoiseRequest {
            cfa_signed: &self.samples,
            width: self.width,
            height: self.height,
            pattern: self.pattern,
            noise,
            token,
        }
    }
}

/// Safe contract for asynchronous tiled CFA denoising (`rraw-ai`).
pub trait TileDenoiser: Send + Sync {
    /// Denoises a signed normalized CFA buffer, checking `is_cancelled` per tile.
    fn denoise_cfa(
        &self,
        request: &TileDenoiseRequest<'_>,
        is_cancelled: &(dyn Fn() -> bool + Sync),
    ) -> Result<Vec<f32>, AiError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_geometry_preserves_bayer_and_xtrans_cfa_phase() {
        let bayer = CfaPattern::Bayer(BayerOrigin::Rggb);
        let xtrans = CfaPattern::fujifilm_xtrans();
        assert!(TileGeometry::preserves_phase(&bayer));
        assert!(TileGeometry::preserves_phase(&xtrans));
        assert_eq!(TileGeometry::TILE_SIZE % 6, 0);
        assert_eq!(TileGeometry::TILE_OVERLAP % 6, 0);
    }

    #[test]
    fn signed_cfa_buffer_preserves_negative_read_noise_samples() -> Result<(), CoreError> {
        let samples = vec![-0.015, 0.0, 0.25, -0.002];
        let pattern = CfaPattern::Bayer(BayerOrigin::Rggb);
        let buffer = SignedCfaBuffer::new(2, 2, pattern, samples.clone())?;
        assert_eq!(buffer.width(), 2);
        assert_eq!(buffer.height(), 2);
        assert_eq!(buffer.pattern(), pattern);
        assert_eq!(buffer.samples(), samples.as_slice());
        assert!(buffer.samples()[0] < 0.0);
        assert!(buffer.samples()[3] < 0.0);
        Ok(())
    }

    #[test]
    fn channel_saturation_tracks_white_balance_above_one() -> Result<(), CoreError> {
        let wb = WhiteBalanceMultipliers::new(2.1, 1.0, 1.6)?;
        let sat = ChannelSaturation::from_wb(wb)?;
        assert!((sat.r() - 2.1).abs() < f32::EPSILON);
        assert!((sat.g() - 1.0).abs() < f32::EPSILON);
        assert!((sat.b() - 1.6).abs() < f32::EPSILON);
        assert!((sat.min_channel() - 1.0).abs() < f32::EPSILON);
        assert!((sat.max_channel() - 2.1).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn noise_profile_variance_clamps_negative_signal_but_keeps_read_variance(
    ) -> Result<(), CoreError> {
        let noise = NoiseProfile::new(
            [0.01, 0.02, 0.03],
            [0.0004, 0.0005, 0.0006],
            NoiseProfileSource::DngTag0xc761,
        )?;
        assert_eq!(noise.shot_gain_alpha(), [0.01, 0.02, 0.03]);
        assert_eq!(noise.read_variance_sigma2(), [0.0004, 0.0005, 0.0006]);
        assert_eq!(noise.source(), NoiseProfileSource::DngTag0xc761);
        let var_neg = noise.variance_at(CfaColor::Red, -0.05);
        assert!((var_neg - 0.0004).abs() < 1e-7);
        let var_pos = noise.variance_at(CfaColor::Green, 0.5);
        assert!((var_pos - 0.0105).abs() < 1e-7);
        let var_nan = noise.variance_at(CfaColor::Blue, f32::NAN);
        assert!((var_nan - 0.0006).abs() < 1e-7);
        Ok(())
    }

    #[test]
    fn headroom_sample_enforces_sdr_floor_inverted_bounds_and_fixed_surface_format() {
        let sdr = HeadroomSample::new(0.4, 0.8);
        assert!((sdr.active_headroom() - 1.0).abs() < f32::EPSILON);
        assert!((sdr.potential_headroom() - 1.0).abs() < f32::EPSILON);

        let nan_sample = HeadroomSample::new(f32::NAN, f32::INFINITY);
        assert!((nan_sample.active_headroom() - 1.0).abs() < f32::EPSILON);
        assert!((nan_sample.potential_headroom() - 1.0).abs() < f32::EPSILON);

        let inf_active = HeadroomSample::new(f32::INFINITY, 16.0);
        assert!((inf_active.active_headroom() - 1.0).abs() < f32::EPSILON);
        assert!((inf_active.potential_headroom() - 16.0).abs() < f32::EPSILON);

        let inverted = HeadroomSample::new(8.0, 4.0);
        assert!((inverted.active_headroom() - 8.0).abs() < f32::EPSILON);
        assert!((inverted.potential_headroom() - 8.0).abs() < f32::EPSILON);

        assert_eq!(SurfaceSpec::EDR_SURFACE_FORMAT, SurfaceFormat::Rgba16Float);
    }

    #[test]
    fn stage0_epoch_increments_monotonically_and_saturates_at_u64_max() {
        let e0 = Stage0Epoch(0);
        let e1 = e0.next();
        assert_eq!(e1, Stage0Epoch(1));
        assert!(e1 > e0);

        let max_epoch = Stage0Epoch(u64::MAX);
        assert_eq!(max_epoch.next(), Stage0Epoch(u64::MAX));
    }

    #[test]
    fn cfa_patterns_expose_exact_periods_and_colors() {
        let bayer_cases = [
            (
                BayerOrigin::Rggb,
                [
                    CfaColor::Red,
                    CfaColor::Green,
                    CfaColor::Green,
                    CfaColor::Blue,
                ],
            ),
            (
                BayerOrigin::Bggr,
                [
                    CfaColor::Blue,
                    CfaColor::Green,
                    CfaColor::Green,
                    CfaColor::Red,
                ],
            ),
            (
                BayerOrigin::Grbg,
                [
                    CfaColor::Green,
                    CfaColor::Red,
                    CfaColor::Blue,
                    CfaColor::Green,
                ],
            ),
            (
                BayerOrigin::Gbrg,
                [
                    CfaColor::Green,
                    CfaColor::Blue,
                    CfaColor::Red,
                    CfaColor::Green,
                ],
            ),
        ];

        for (origin, expected) in bayer_cases {
            let pattern = CfaPattern::Bayer(origin);
            assert_eq!(pattern.period_x(), 2);
            assert_eq!(pattern.period_y(), 2);
            assert_eq!(pattern.color_at(0, 0), expected[0]);
            assert_eq!(pattern.color_at(1, 0), expected[1]);
            assert_eq!(pattern.color_at(0, 1), expected[2]);
            assert_eq!(pattern.color_at(1, 1), expected[3]);
        }

        let xtrans = CfaPattern::fujifilm_xtrans();
        assert_eq!(xtrans.period_x(), 6);
        assert_eq!(xtrans.period_y(), 6);
        assert_eq!(xtrans.color_at(0, 0), xtrans.color_at(6, 6));
        assert_eq!(xtrans.color_at(4, 0), CfaColor::Red);
        assert_eq!(xtrans.color_at(1, 0), CfaColor::Blue);
    }

    #[test]
    fn invalid_physical_inputs_and_overflows_return_typed_errors() {
        assert!(NoiseProfile::new(
            [-0.1, 0.0, 0.0],
            [0.0; 3],
            NoiseProfileSource::WaveletMadFallback
        )
        .is_err());
        assert!(NoiseProfile::new(
            [0.0; 3],
            [f32::NAN, 0.0, 0.0],
            NoiseProfileSource::WaveletMadFallback
        )
        .is_err());

        for bad in [0.0, -0.5, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(WhiteBalanceMultipliers::new(bad, 1.0, 1.0).is_err());
            assert!(WhiteBalanceMultipliers::new(1.0, bad, 1.0).is_err());
            assert!(WhiteBalanceMultipliers::new(1.0, 1.0, bad).is_err());
        }

        assert_eq!(
            SignedCfaBuffer::new(0, 10, CfaPattern::Bayer(BayerOrigin::Rggb), Vec::new()),
            Err(CoreError::InvalidDimensions {
                width: 0,
                height: 10
            })
        );
        assert_eq!(
            SignedCfaBuffer::new(10, 0, CfaPattern::Bayer(BayerOrigin::Rggb), Vec::new()),
            Err(CoreError::InvalidDimensions {
                width: 10,
                height: 0
            })
        );
        assert_eq!(
            SignedCfaBuffer::new(
                usize::MAX,
                2,
                CfaPattern::Bayer(BayerOrigin::Rggb),
                Vec::new()
            ),
            Err(CoreError::InvalidDimensions {
                width: usize::MAX,
                height: 2
            })
        );
        assert_eq!(
            SignedCfaBuffer::new(2, 2, CfaPattern::Bayer(BayerOrigin::Rggb), vec![0.0; 3]),
            Err(CoreError::BufferLengthMismatch {
                expected: 4,
                actual: 3
            })
        );
        assert!(TileGeometry::new(502, 48).is_err());
        assert!(TileGeometry::new(504, 504).is_err());
        assert_eq!(TileGeometry::COREML.stride(), 456);
    }

    #[test]
    fn tile_denoiser_request_supports_epoch_cancellation() -> Result<(), CoreError> {
        struct CancellationCheckDenoiser;

        impl TileDenoiser for CancellationCheckDenoiser {
            fn denoise_cfa(
                &self,
                request: &TileDenoiseRequest<'_>,
                is_cancelled: &(dyn Fn() -> bool + Sync),
            ) -> Result<Vec<f32>, AiError> {
                if is_cancelled() {
                    return Err(AiError::Cancelled(request.token));
                }
                Ok(request.cfa_signed.to_vec())
            }
        }

        let buf = SignedCfaBuffer::new(
            2,
            2,
            CfaPattern::Bayer(BayerOrigin::Rggb),
            vec![-0.01, 0.2, 0.3, 0.4],
        )?;
        let noise = NoiseProfile::new(
            [0.01; 3],
            [0.0001; 3],
            NoiseProfileSource::WaveletMadFallback,
        )?;
        let token = Stage0Token {
            photo_id: PhotoId(7),
            epoch: Stage0Epoch(3),
        };
        let req = buf.as_denoise_request(&noise, token);
        let denoiser = CancellationCheckDenoiser;

        assert_eq!(
            denoiser.denoise_cfa(&req, &|| false),
            Ok(buf.samples().to_vec())
        );
        assert_eq!(
            denoiser.denoise_cfa(&req, &|| true),
            Err(AiError::Cancelled(token))
        );
        Ok(())
    }
}
