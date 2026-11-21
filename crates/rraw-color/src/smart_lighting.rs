#![forbid(unsafe_code)]

//! Scene Dynamic Range Analyzer & Local Tone Mapping Core.
//!
//! Provides scene dynamic range analysis (luminance statistics, scene classification,
//! backlighting detection) and edge-preserving base/detail decomposition using
//! a fast separable Guided Filter for halo-free shadow lift and highlight compression
//! in linear ACEScg (AP1).

use rraw_core::{CoreError, SmartLightingMode, SmartLightingParams};

use crate::ColorError;

/// AP1 ACEScg Rec.2020/AP1 luminance coefficients.
pub const AP1_LUMA_R: f32 = 0.2722287;
pub const AP1_LUMA_G: f32 = 0.6740818;
pub const AP1_LUMA_B: f32 = 0.0536895;

/// Photometric middle gray reference in linear scene reflectance.
pub const MIDDLE_GRAY_LINEAR: f32 = 0.18;

/// Computes the linear ACEScg (AP1) luminance from RGB components.
#[inline]
#[must_use]
pub fn aces_cg_luminance(rgb: [f32; 3]) -> f32 {
    AP1_LUMA_R * rgb[0] + AP1_LUMA_G * rgb[1] + AP1_LUMA_B * rgb[2]
}

/// Normalizes raw intensity into standard `[0.0, 1.0]` fraction.
///
/// Accepts both fraction `0.0..=1.0` and UI percentage `0.0..=100.0`.
#[inline]
#[must_use]
pub fn normalize_intensity(raw: f32) -> f32 {
    if raw > 1.0 {
        (raw / 100.0).clamp(0.0, 1.0)
    } else {
        raw.clamp(0.0, 1.0)
    }
}

/// Dynamic range percentiles extracted from scene luminance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DynamicRangePercentiles {
    pub p0_1: f32,
    pub p1: f32,
    pub p10: f32,
    pub p50: f32,
    pub p90: f32,
    pub p99: f32,
    pub p99_9: f32,
}

/// Scene lighting condition classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SceneClassification {
    /// Dominant dark tones with low key value.
    LowKey,
    /// Balanced dynamic range and standard contrast.
    Normal,
    /// Dominant bright tones with high key value.
    HighKey,
    /// Bright periphery/background with underexposed central subject.
    Backlit,
    /// High dynamic range exceeding standard sensor/display latitude (> 10.5 EV).
    HighDynamicRange,
}

/// Comprehensive scene dynamic range analysis report.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneDynamicRangeAnalysis {
    pub min_luminance: f32,
    pub max_luminance: f32,
    pub mean_luminance: f32,
    pub geometric_mean_luminance: f32,
    pub percentiles: DynamicRangePercentiles,
    pub dynamic_range_ev: f32,
    pub scene_classification: SceneClassification,
    pub under_exposure_ev: f32,
    pub over_exposure_ev: f32,
    pub shadow_clipping_ratio: f32,
    pub highlight_clipping_ratio: f32,
    pub recommended_shadow_lift: f32,
    pub recommended_highlight_compression: f32,
}

/// Fast separable 2D box filter with boundary clamping.
///
/// Runs 1D horizontal sliding window then 1D vertical sliding window in $O(W \times H)$ time.
pub fn box_filter_2d(input: &[f32], width: usize, height: usize, radius: usize) -> Vec<f32> {
    if width == 0 || height == 0 || input.is_empty() {
        return Vec::new();
    }
    if radius == 0 {
        return input.to_vec();
    }

    let r = radius;
    let window_size_h = (2 * r + 1) as f32;
    let mut temp = vec![0.0_f32; width * height];

    // 1. Horizontal pass
    for y in 0..height {
        let row_offset = y * width;
        let row = &input[row_offset..row_offset + width];
        let out_row = &mut temp[row_offset..row_offset + width];

        // Initial window sum at x = 0 with clamped border
        let mut sum = (r as f32) * row[0];
        for k in 0..=r {
            let cx = k.min(width - 1);
            sum += row[cx];
        }
        out_row[0] = sum / window_size_h;

        for (x, out_val) in out_row.iter_mut().enumerate().skip(1) {
            let leaving_idx = if x > r { x - 1 - r } else { 0 };
            let entering_idx = (x + r).min(width - 1);
            sum += row[entering_idx] - row[leaving_idx];
            *out_val = sum / window_size_h;
        }
    }

    // 2. Vertical pass
    let window_size_v = (2 * r + 1) as f32;
    let mut output = vec![0.0_f32; width * height];

    for x in 0..width {
        // Initial window sum at y = 0
        let mut sum = (r as f32) * temp[x];
        for k in 0..=r {
            let cy = k.min(height - 1);
            sum += temp[cy * width + x];
        }
        output[x] = sum / window_size_v;

        for y in 1..height {
            let leaving_y = if y > r { y - 1 - r } else { 0 };
            let entering_y = (y + r).min(height - 1);
            sum += temp[entering_y * width + x] - temp[leaving_y * width + x];
            output[y * width + x] = sum / window_size_v;
        }
    }

    output
}

/// Edge-preserving self-guided filter (He et al., ECCV 2010 / TPAMI 2013).
///
/// Smooths flat regions while preserving sharp step edges without haloing or gradient reversal.
pub fn self_guided_filter(
    input: &[f32],
    width: usize,
    height: usize,
    radius: usize,
    eps: f32,
) -> Vec<f32> {
    let n = width * height;
    if n == 0 || input.len() != n {
        return Vec::new();
    }
    if radius == 0 {
        return input.to_vec();
    }

    // 1. Mean of input (mean_I)
    let mean_i = box_filter_2d(input, width, height, radius);

    // 2. Mean of input squared (corr_I)
    let mut input_sq = Vec::with_capacity(n);
    for &val in input {
        input_sq.push(val * val);
    }
    let corr_i = box_filter_2d(&input_sq, width, height, radius);

    // 3. Compute linear regression coefficients a and b:
    //    var_I = corr_I - mean_I * mean_I
    //    a = var_I / (var_I + eps)
    //    b = (1.0 - a) * mean_I
    let mut a = Vec::with_capacity(n);
    let mut b = Vec::with_capacity(n);
    for i in 0..n {
        let m = mean_i[i];
        let var_i = (corr_i[i] - m * m).max(0.0);
        let a_val = var_i / (var_i + eps);
        let b_val = (1.0 - a_val) * m;
        a.push(a_val);
        b.push(b_val);
    }

    // 4. Mean of a and b
    let mean_a = box_filter_2d(&a, width, height, radius);
    let mean_b = box_filter_2d(&b, width, height, radius);

    // 5. Output q = mean_a * I + mean_b
    let mut q = Vec::with_capacity(n);
    for i in 0..n {
        q.push(mean_a[i] * input[i] + mean_b[i]);
    }

    q
}

/// Analyzes the dynamic range and photometric distribution of an ACEScg RGB image.
pub fn analyze_scene_dynamic_range(
    rgb_acescg: &[[f32; 3]],
    width: usize,
    height: usize,
) -> Result<SceneDynamicRangeAnalysis, ColorError> {
    if width == 0 || height == 0 {
        return Err(ColorError::Core(CoreError::InvalidDimensions {
            width,
            height,
        }));
    }
    let expected =
        width
            .checked_mul(height)
            .ok_or(ColorError::Core(CoreError::InvalidDimensions {
                width,
                height,
            }))?;
    if rgb_acescg.len() != expected {
        return Err(ColorError::Core(CoreError::BufferLengthMismatch {
            expected,
            actual: rgb_acescg.len(),
        }));
    }

    let mut min_luma = f32::INFINITY;
    let mut max_luma = f32::NEG_INFINITY;
    let mut sum_luma = 0.0_f64;
    let mut log_sum_luma = 0.0_f64;
    let mut shadow_clip_count = 0_usize;
    let mut highlight_clip_count = 0_usize;

    // Subsampling for fast percentile estimation on large images
    let sample_stride = (expected / 65536).max(1);
    let mut samples = Vec::with_capacity(expected / sample_stride + 1);

    // Central region bounds (25%..75% width & height)
    let center_x_start = width / 4;
    let center_x_end = (3 * width) / 4;
    let center_y_start = height / 4;
    let center_y_end = (3 * height) / 4;

    let mut center_luma_sum = 0.0_f64;
    let mut center_pixel_count = 0_usize;
    let mut periph_luma_sum = 0.0_f64;
    let mut periph_pixel_count = 0_usize;

    for (idx, rgb) in rgb_acescg.iter().enumerate() {
        let y = aces_cg_luminance(*rgb).max(0.0);

        if y < min_luma {
            min_luma = y;
        }
        if y > max_luma {
            max_luma = y;
        }
        sum_luma += y as f64;
        log_sum_luma += (y.max(1e-6) as f64).ln();

        if y <= 1e-4 {
            shadow_clip_count += 1;
        }
        if y >= 0.99 {
            highlight_clip_count += 1;
        }

        let px = idx % width;
        let py = idx / width;
        if px >= center_x_start && px < center_x_end && py >= center_y_start && py < center_y_end {
            center_luma_sum += y as f64;
            center_pixel_count += 1;
        } else {
            periph_luma_sum += y as f64;
            periph_pixel_count += 1;
        }

        if idx % sample_stride == 0 {
            samples.push(y);
        }
    }

    let n = expected as f64;
    let mean_luma = (sum_luma / n) as f32;
    let geometric_mean_luma = (log_sum_luma / n).exp() as f32;

    // Sort samples for percentiles
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let sample_len = samples.len();

    let get_percentile = |pct: f64| -> f32 {
        if sample_len == 0 {
            return 0.0;
        }
        let pos = (pct * (sample_len - 1) as f64).clamp(0.0, (sample_len - 1) as f64);
        let idx = pos as usize;
        let frac = (pos - idx as f64) as f32;
        if idx + 1 < sample_len {
            samples[idx] * (1.0 - frac) + samples[idx + 1] * frac
        } else {
            samples[idx]
        }
    };

    let percentiles = DynamicRangePercentiles {
        p0_1: get_percentile(0.001),
        p1: get_percentile(0.01),
        p10: get_percentile(0.10),
        p50: get_percentile(0.50),
        p90: get_percentile(0.90),
        p99: get_percentile(0.99),
        p99_9: get_percentile(0.999),
    };

    let dynamic_range_ev = (percentiles.p99.max(1e-5) / percentiles.p1.max(1e-5)).log2();

    // Scene classification
    let center_avg = if center_pixel_count > 0 {
        (center_luma_sum / center_pixel_count as f64) as f32
    } else {
        mean_luma
    };
    let periph_avg = if periph_pixel_count > 0 {
        (periph_luma_sum / periph_pixel_count as f64) as f32
    } else {
        mean_luma
    };

    let scene_classification = if dynamic_range_ev > 10.5 {
        SceneClassification::HighDynamicRange
    } else if periph_avg > 0.15 && center_avg < 0.45 * periph_avg {
        SceneClassification::Backlit
    } else if geometric_mean_luma < 0.05 {
        SceneClassification::LowKey
    } else if geometric_mean_luma > 0.35 {
        SceneClassification::HighKey
    } else {
        SceneClassification::Normal
    };

    let under_exposure_ev = (MIDDLE_GRAY_LINEAR / geometric_mean_luma.max(1e-5)).log2();
    let over_exposure_ev = (percentiles.p99_9.max(1.0) / 1.0).log2();

    let shadow_clipping_ratio = (shadow_clip_count as f32) / (expected as f32);
    let highlight_clipping_ratio = (highlight_clip_count as f32) / (expected as f32);

    // Recommended parameters tailored to scene classification
    let (recommended_shadow_lift, recommended_highlight_compression) = match scene_classification {
        SceneClassification::Backlit => (0.85, 0.45),
        SceneClassification::HighDynamicRange => (0.65, 0.60),
        SceneClassification::LowKey => (0.45, 0.20),
        SceneClassification::HighKey => (0.15, 0.55),
        SceneClassification::Normal => (0.35, 0.25),
    };

    Ok(SceneDynamicRangeAnalysis {
        min_luminance: min_luma,
        max_luminance: max_luma,
        mean_luminance: mean_luma,
        geometric_mean_luminance: geometric_mean_luma,
        percentiles,
        dynamic_range_ev,
        scene_classification,
        under_exposure_ev,
        over_exposure_ev,
        shadow_clipping_ratio,
        highlight_clipping_ratio,
        recommended_shadow_lift,
        recommended_highlight_compression,
    })
}

/// Applies scene-aware Smart Lighting adaptive local tone mapping in linear ACEScg.
///
/// Modulates base luminance using an edge-preserving Guided Filter while preserving
/// high-frequency detail and exact chromaticity ratios $(R/Y, G/Y, B/Y)$.
pub fn apply_smart_lighting_acescg(
    rgb_pixels: &mut [[f32; 3]],
    width: usize,
    height: usize,
    params: &SmartLightingParams,
) -> Result<Option<SceneDynamicRangeAnalysis>, ColorError> {
    if !params.is_active() {
        return Ok(None);
    }

    let analysis = analyze_scene_dynamic_range(rgb_pixels, width, height)?;

    let eff_intensity = normalize_intensity(params.intensity);

    // Resolve effective shadow lift and highlight compression amounts
    let (shadow_lift, highlight_compression) = match params.mode {
        SmartLightingMode::Off => (0.0, 0.0),
        SmartLightingMode::Slight => (
            (analysis.recommended_shadow_lift * 0.9).max(0.60) * eff_intensity,
            (analysis.recommended_highlight_compression * 0.9).max(0.35) * eff_intensity,
        ),
        SmartLightingMode::Medium => (
            (analysis.recommended_shadow_lift * 1.5).max(1.05) * eff_intensity,
            (analysis.recommended_highlight_compression * 1.4).max(0.65) * eff_intensity,
        ),
        SmartLightingMode::Strong => (
            (analysis.recommended_shadow_lift * 2.2).max(1.55) * eff_intensity,
            (analysis.recommended_highlight_compression * 2.0).max(0.95) * eff_intensity,
        ),
        SmartLightingMode::Custom => (
            params.custom_shadow_lift * 1.8 * eff_intensity,
            params.custom_highlight_compression * 1.6 * eff_intensity,
        ),
    };

    if shadow_lift <= 0.0 && highlight_compression <= 0.0 {
        return Ok(Some(analysis));
    }

    let n = width * height;
    let mut log_luma = Vec::with_capacity(n);
    let mut original_luma = Vec::with_capacity(n);

    for rgb in rgb_pixels.iter() {
        let y = aces_cg_luminance(*rgb).max(0.0);
        original_luma.push(y);
        log_luma.push((y.max(1e-6)).log2());
    }

    // Edge-preserving decomposition
    let filter_radius = (width.min(height) / 24).clamp(8, 96);
    let eps = 0.08; // ~0.28 EV variance in log2 domain for smooth natural edge transitions
    let base_log = self_guided_filter(&log_luma, width, height, filter_radius, eps);

    let mid_log = MIDDLE_GRAY_LINEAR.log2(); // ~ -2.4739
    let floor_log = 1e-4_f32.log2(); // ~ -13.2877
    let ceil_log = analysis.percentiles.p99_9.max(2.0).log2(); // Highlight anchor

    for i in 0..n {
        let b = base_log[i];
        let d = log_luma[i] - b;
        let mut b_mapped = b;

        // Shadow lift (smooth toe lift below midtones with black floor preservation)
        if b < mid_log && shadow_lift > 0.0 {
            let stops_below = (mid_log - b).max(0.0);
            let t_shadow = (stops_below / 6.0).clamp(0.0, 1.0);
            let black_taper = ((b - floor_log) / 3.0).clamp(0.0, 1.0);
            let lift_delta =
                shadow_lift * 3.0 * t_shadow.powf(0.65) * (1.0 - 0.32 * t_shadow) * black_taper;
            b_mapped += lift_delta;
        }

        // Highlight compression (smooth shoulder rolloff above midtones)
        if b > mid_log && highlight_compression > 0.0 {
            let stops_above = (b - mid_log).max(0.0);
            let t_high = (stops_above / (ceil_log - mid_log).max(1.0)).clamp(0.0, 1.0);
            let comp_delta = highlight_compression * 1.6 * t_high * (1.0 - 0.3 * t_high);
            b_mapped -= comp_delta;
        }

        let luma_out_log = b_mapped + d;
        let luma_out = 2.0_f32.powf(luma_out_log);
        let orig_y = original_luma[i];

        let scale = if orig_y > 1e-7 {
            luma_out / orig_y
        } else {
            1.0
        };

        // Rescale RGB proportionally to preserve chromaticity
        rgb_pixels[i][0] = (rgb_pixels[i][0] * scale).max(0.0);
        rgb_pixels[i][1] = (rgb_pixels[i][1] * scale).max(0.0);
        rgb_pixels[i][2] = (rgb_pixels[i][2] * scale).max(0.0);
    }

    Ok(Some(analysis))
}
