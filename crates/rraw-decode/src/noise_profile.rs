//! Noise profile extraction from DNG tag 0xc761 and robust wavelet MAD fallback estimator.

use rraw_core::{NoiseProfile, NoiseProfileSource, SignedCfaBuffer};

use crate::DecodeError;

/// Parses DNG tag `0xc761` (`NoiseProfile`) array of `(shot_noise_alpha, read_noise_sigma2)` pairs.
pub fn parse_dng_noise_profile_tag(tag_data: &[f64]) -> Result<NoiseProfile, DecodeError> {
    if tag_data.len() < 2 || !tag_data.len().is_multiple_of(2) {
        return Err(DecodeError::MalformedMetadata(format!(
            "DNG tag 0xc761 requires an even number of f64 values (pairs of alpha, sigma2), got {}",
            tag_data.len()
        )));
    }

    let pairs = tag_data.len() / 2;
    let mut alphas = [0.0_f32; 3];
    let mut sigmas = [0.0_f32; 3];

    for ch in 0..3 {
        let pair_idx = ch.min(pairs - 1);
        let alpha = tag_data[pair_idx * 2];
        let sigma2 = tag_data[pair_idx * 2 + 1];

        if !alpha.is_finite() || alpha < 0.0 || !sigma2.is_finite() || sigma2 < 0.0 {
            return Err(DecodeError::MalformedMetadata(format!(
                "invalid NoiseProfile values at channel {ch}: alpha={alpha}, sigma2={sigma2}"
            )));
        }

        alphas[ch] = alpha as f32;
        sigmas[ch] = sigma2 as f32;
    }

    Ok(NoiseProfile::new(
        alphas,
        sigmas,
        NoiseProfileSource::DngTag0xc761,
    )?)
}

/// Estimates camera sensor read noise using the robust Median Absolute Deviation (MAD)
/// on high-frequency Haar wavelet detail coefficients across CFA color planes.
///
/// Formula: `sigma_est = MAD(W_1) / 0.67448975`
pub fn estimate_noise_profile_wavelet_mad(
    cfa: &SignedCfaBuffer,
) -> Result<NoiseProfile, DecodeError> {
    let width = cfa.width();
    let height = cfa.height();
    let pattern = cfa.pattern();
    let samples = cfa.samples();

    let step_x = pattern.period_x();
    let step_y = pattern.period_y();

    // Strided subsampling capping sample count to ~16k per channel to avoid multi-million allocation churn
    let raw_pts_per_phase = (width / (step_x * 2)).max(1) * (height / (step_y * 2)).max(1);
    let stride_mult = (raw_pts_per_phase / 16384).max(1);
    let stride_x = step_x * 2 * stride_mult;
    let stride_y = step_y * 2 * stride_mult;

    let pts_cap = (raw_pts_per_phase / stride_mult).min(32768);
    let mut channel_diffs: [Vec<f32>; 3] = [
        Vec::with_capacity(pts_cap),
        Vec::with_capacity(pts_cap),
        Vec::with_capacity(pts_cap),
    ];

    for oy in 0..step_y {
        for ox in 0..step_x {
            let color = pattern.color_at(ox, oy);
            let mut y = oy;
            while y + step_y < height {
                let mut x = ox;
                while x + step_x < width {
                    let p00 = samples[y * width + x];
                    let p10 = samples[y * width + (x + step_x)];
                    let p01 = samples[(y + step_y) * width + x];
                    let p11 = samples[(y + step_y) * width + (x + step_x)];

                    let d = 0.5 * (p00 - p10 - p01 + p11);
                    channel_diffs[color.channel_index()].push(d);

                    x += stride_x;
                }
                y += stride_y;
            }
        }
    }

    let mut sigmas2 = [0.0_f32; 3];

    for ch in 0..3 {
        let diffs = &mut channel_diffs[ch];
        if diffs.len() < 16 {
            return Err(DecodeError::MalformedMetadata(format!(
                "insufficient pixel samples to compute MAD for CFA channel {ch} (got {})",
                diffs.len()
            )));
        }

        let sigma = compute_mad_sigma(diffs);
        sigmas2[ch] = sigma * sigma;
    }

    Ok(NoiseProfile::new(
        [0.0; 3],
        sigmas2,
        NoiseProfileSource::WaveletMadFallback,
    )?)
}

/// Computes the Median Absolute Deviation (MAD) estimator for a slice of wavelet detail differences.
fn compute_mad_sigma(values: &mut [f32]) -> f32 {
    let med = median(values);

    // Compute absolute deviations from median
    let mut deviations: Vec<f32> = values.iter().map(|&v| (v - med).abs()).collect();
    let mad = median(&mut deviations);

    // Standard normal distribution consistency factor: 1 / Phi^-1(3/4) = 1.482602218 = 1 / 0.67448975
    mad / 0.674_489_75
}

/// Helper to compute the median of a floating-point slice in-place using `select_nth_unstable_by`.
fn median(slice: &mut [f32]) -> f32 {
    if slice.is_empty() {
        return 0.0;
    }
    let mid = slice.len() / 2;
    slice.select_nth_unstable_by(mid, |a, b| {
        a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
    });

    if !slice.len().is_multiple_of(2) {
        slice[mid]
    } else {
        // Find max in first half to average mid-1 and mid
        let prev = slice[..mid]
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);
        (prev + slice[mid]) * 0.5
    }
}
