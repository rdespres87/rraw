#![forbid(unsafe_code)]

//! Strict TDD unit & integration tests for Tasks 7.2 & 7.3: Guided Chroma & Luma Denoising.

use rraw_core::denoise::{denoise_rgb, DenoiseConfig};

#[test]
fn test_denoise_neutral_on_zero_amounts() {
    let width = 8;
    let height = 8;
    let original = vec![[0.3_f32, 0.5_f32, 0.7_f32]; width * height];
    let mut samples = original.clone();

    let config = DenoiseConfig {
        chroma_amount: 0.0,
        luma_amount: 0.0,
    };

    denoise_rgb(&mut samples, width, height, &config);

    for (p_orig, p_denoised) in original.iter().zip(&samples) {
        assert_eq!(p_orig[0], p_denoised[0]);
        assert_eq!(p_orig[1], p_denoised[1]);
        assert_eq!(p_orig[2], p_denoised[2]);
    }
}

#[test]
fn test_chroma_denoise_suppresses_color_speckles_without_blurring_luma_edge() {
    let width = 16;
    let height = 16;
    let mut samples = vec![[0.5_f32, 0.5_f32, 0.5_f32]; width * height];

    // Introduce alternating high-frequency color speckles (chroma noise) on a flat neutral gray patch
    for y in 0..height {
        for x in 0..width {
            let speckle = if (x + y) % 2 == 0 {
                0.15_f32
            } else {
                -0.15_f32
            };
            // Perturb R and B in opposite directions (pure chroma mottle, preserving approximate luminance)
            samples[y * width + x][0] += speckle;
            samples[y * width + x][2] -= speckle;
        }
    }

    // Compute initial chroma variance
    let initial_chroma_var: f32 =
        samples.iter().map(|p| (p[0] - p[2]).powi(2)).sum::<f32>() / (samples.len() as f32);

    assert!(initial_chroma_var > 0.05);

    let config = DenoiseConfig {
        chroma_amount: 0.8,
        luma_amount: 0.0,
    };

    denoise_rgb(&mut samples, width, height, &config);

    // After chroma denoise, the color variance should be substantially suppressed
    let final_chroma_var: f32 =
        samples.iter().map(|p| (p[0] - p[2]).powi(2)).sum::<f32>() / (samples.len() as f32);

    assert!(
        final_chroma_var < initial_chroma_var * 0.25,
        "Chroma variance was not suppressed: initial {initial_chroma_var}, final {final_chroma_var}"
    );
}

#[test]
fn test_luma_denoise_smoothes_noisy_flat_patch() {
    let width = 16;
    let height = 16;
    let mut samples = vec![[0.5_f32, 0.5_f32, 0.5_f32]; width * height];

    // Introduce high-frequency luminance noise
    for (i, p) in samples.iter_mut().enumerate() {
        let n = if i % 2 == 0 { 0.08_f32 } else { -0.08_f32 };
        p[0] += n;
        p[1] += n;
        p[2] += n;
    }

    let initial_luma_var: f32 =
        samples.iter().map(|p| (p[1] - 0.5).powi(2)).sum::<f32>() / (samples.len() as f32);

    let config = DenoiseConfig {
        chroma_amount: 0.0,
        luma_amount: 0.75,
    };

    denoise_rgb(&mut samples, width, height, &config);

    let final_luma_var: f32 =
        samples.iter().map(|p| (p[1] - 0.5).powi(2)).sum::<f32>() / (samples.len() as f32);

    assert!(
        final_luma_var < initial_luma_var * 0.4,
        "Luma variance was not suppressed: initial {initial_luma_var}, final {final_luma_var}"
    );
}
