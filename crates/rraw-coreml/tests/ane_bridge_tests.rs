//! Integration tests for the `rraw-coreml` ANE bridge layer.
//!
//! Exercises hardware detection, compute-unit invariants, model descriptor
//! validation, raw denoiser dimension/Anscombe correctness, and semantic
//! segmenter partition-of-unity guarantees.

use rraw_ai::{NeuralTileDenoiser, SemanticSegmenter};
use rraw_core::{AiError, NoiseProfile, NoiseProfileSource, SemanticMaskCategory};
use rraw_coreml::{
    AneRawDenoiser, AneSemanticSegmenter, CoreMlComputeUnits, CoreMlEngine,
    CoreMlHardwareCapabilities, CoreMlModelDescriptor,
};
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

fn sample_noise_profile() -> Result<NoiseProfile, Box<dyn Error>> {
    let profile = NoiseProfile::new(
        [0.001, 0.001, 0.001],
        [0.0001, 0.0001, 0.0001],
        NoiseProfileSource::DngTag0xc761,
    )?;
    Ok(profile)
}

#[test]
fn test_hardware_detection() {
    let caps = CoreMlHardwareCapabilities::detect();
    assert!(
        !caps.device_name.trim().is_empty(),
        "device_name should be non-empty, got {:?}",
        caps.device_name
    );

    #[cfg(target_os = "macos")]
    {
        // On macOS ARM64 (Apple Silicon), ANE should be detected
        #[cfg(target_arch = "aarch64")]
        assert!(caps.has_ane, "Apple Silicon should report has_ane = true");
    }

    let units = caps.recommended_compute_units;
    let display = format!("{units}");
    assert!(!display.is_empty());
}

#[test]
fn test_compute_units_invariants() {
    let variants = [
        CoreMlComputeUnits::All,
        CoreMlComputeUnits::CpuAndNeuralEngine,
        CoreMlComputeUnits::CpuAndGpu,
        CoreMlComputeUnits::CpuOnly,
    ];

    for variant in &variants {
        let display = format!("{variant}");
        assert!(!display.is_empty());
        let debug = format!("{variant:?}");
        assert!(!debug.is_empty());
        assert_eq!(*variant, *variant);
    }

    assert_eq!(
        format!("{}", CoreMlComputeUnits::All),
        "All (ANE + GPU + CPU)"
    );
    assert_eq!(
        format!("{}", CoreMlComputeUnits::CpuAndNeuralEngine),
        "CPU + Neural Engine"
    );
    assert_eq!(format!("{}", CoreMlComputeUnits::CpuAndGpu), "CPU + GPU");
    assert_eq!(format!("{}", CoreMlComputeUnits::CpuOnly), "CPU Only");
}

#[test]
fn test_model_descriptor() {
    let desc = CoreMlModelDescriptor::new(
        "cfa_denoiser_504x504",
        CoreMlComputeUnits::CpuAndNeuralEngine,
        vec![1, 1, 504, 504],
        vec![1, 1, 504, 504],
    );
    assert_eq!(desc.model_name(), "cfa_denoiser_504x504");
    assert_eq!(desc.compute_units(), CoreMlComputeUnits::CpuAndNeuralEngine);
    assert_eq!(desc.input_shape(), &[1, 1, 504, 504]);
    assert_eq!(desc.output_shape(), &[1, 1, 504, 504]);
    assert!(desc.is_valid());

    let invalid_desc = CoreMlModelDescriptor::new("", CoreMlComputeUnits::All, vec![], vec![]);
    assert!(!invalid_desc.is_valid());
}

#[test]
fn test_ane_raw_denoiser_dimension_check() -> TestResult {
    let denoiser = AneRawDenoiser::new();
    let noise = sample_noise_profile()?;

    // Wrong buffer size (100 elements instead of 504*504 = 254016)
    let invalid_tile = vec![0.5_f32; 100];
    let result = denoiser.denoise_tile(&invalid_tile, 504, 504, &noise);
    assert!(result.is_err());
    assert!(matches!(result, Err(AiError::InvalidInput(_))));

    // Wrong dimensions passed (width=10, height=10)
    let small_tile = vec![0.5_f32; 100];
    let result = denoiser.denoise_tile(&small_tile, 10, 10, &noise);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn test_ane_raw_denoiser_504x504_anscombe() -> TestResult {
    let denoiser = AneRawDenoiser::new();
    let noise = sample_noise_profile()?;
    let size = 504 * 504;

    // Synthetic tile with a baseline signal and noise
    let mut synthetic_tile = vec![0.25_f32; size];
    for (i, val) in synthetic_tile.iter_mut().enumerate() {
        let noise_term = ((i % 17) as f32 - 8.0) * 0.005;
        *val = (*val + noise_term).clamp(0.0, 1.0);
    }

    let denoised = denoiser.denoise_tile(&synthetic_tile, 504, 504, &noise)?;

    assert_eq!(denoised.len(), size);
    for &val in &denoised {
        assert!(val.is_finite(), "denoised sample must be finite");
        assert!(val >= 0.0, "denoised sample must be >= 0.0");
    }

    // Variance should be reduced by the denoising operation
    let mean_orig: f32 = synthetic_tile.iter().sum::<f32>() / size as f32;
    let var_orig: f32 = synthetic_tile
        .iter()
        .map(|&x| (x - mean_orig).powi(2))
        .sum::<f32>()
        / size as f32;

    let mean_denoised: f32 = denoised.iter().sum::<f32>() / size as f32;
    let var_denoised: f32 = denoised
        .iter()
        .map(|&x| (x - mean_denoised).powi(2))
        .sum::<f32>()
        / size as f32;

    assert!(
        var_denoised <= var_orig,
        "denoising should reduce or maintain variance: orig={var_orig}, denoised={var_denoised}"
    );
    Ok(())
}

#[test]
fn test_ane_semantic_segmenter_partition_of_unity() -> TestResult {
    let segmenter = AneSemanticSegmenter::new();
    let width = 32;
    let height = 32;
    let total = width * height;

    // Create synthetic landscape RGB image
    let mut rgb = Vec::with_capacity(total);
    for y in 0..height {
        let v = y as f32 / height as f32;
        for x in 0..width {
            let u = x as f32 / width as f32;
            let pixel = if v < 0.4 {
                // Sky area: high blue
                [0.2_f32, 0.45_f32, 0.85_f32]
            } else if (u - 0.5).abs() < 0.2 && (v - 0.6).abs() < 0.2 {
                // Subject / portrait area: warm skin tones
                [0.72_f32, 0.52_f32, 0.42_f32]
            } else {
                // Background ground: green/brown
                [0.3_f32, 0.5_f32, 0.2_f32]
            };
            rgb.push(pixel);
        }
    }

    let masks = segmenter.segment_rgb(&rgb, width, height)?;

    assert_eq!(masks.width, width);
    assert_eq!(masks.height, height);
    assert_eq!(masks.subject.len(), total);
    assert_eq!(masks.sky.len(), total);
    assert_eq!(masks.background.len(), total);
    assert_eq!(masks.portrait.len(), total);

    // Invariant: Partition of unity `subject[i] + background[i] == 1.0`
    for i in 0..total {
        let s = masks.subject[i];
        let bg = masks.background[i];
        let sky = masks.sky[i];
        let port = masks.portrait[i];

        assert!(
            (0.0..=1.0).contains(&s),
            "subject out of bounds at {i}: {s}"
        );
        assert!(
            (0.0..=1.0).contains(&bg),
            "background out of bounds at {i}: {bg}"
        );
        assert!(
            (0.0..=1.0).contains(&sky),
            "sky out of bounds at {i}: {sky}"
        );
        assert!(
            (0.0..=1.0).contains(&port),
            "portrait out of bounds at {i}: {port}"
        );

        let sum = s + bg;
        assert!(
            (sum - 1.0).abs() < 1e-4,
            "partition of unity violated at pixel {i}: subject={s}, background={bg}, sum={sum}"
        );
    }

    // Verify interleaved RGBA layout
    let rgba = masks.to_rgba_interleaved();
    assert_eq!(rgba.len(), total * 4);
    for i in 0..total {
        assert_eq!(rgba[i * 4], masks.subject[i]);
        assert_eq!(rgba[i * 4 + 1], masks.sky[i]);
        assert_eq!(rgba[i * 4 + 2], masks.background[i]);
        assert_eq!(rgba[i * 4 + 3], masks.portrait[i]);
    }

    // Verify bilinear sampling on sky area (u=0.5, v=0.2)
    let sky_val = masks.sample_bilinear(SemanticMaskCategory::Sky, 0.5, 0.2, false, 0.2);
    assert!(
        sky_val > 0.3,
        "sky confidence should be prominent in upper frame"
    );
    Ok(())
}

#[test]
fn test_ane_semantic_segmenter_invalid_dimensions() {
    let segmenter = AneSemanticSegmenter::new();
    assert!(segmenter.segment_rgb(&[], 0, 10).is_err());
    assert!(segmenter.segment_rgb(&[], 10, 0).is_err());
    assert!(segmenter.segment_rgb(&[[0.0, 0.0, 0.0]; 5], 4, 4).is_err());
}

#[test]
fn test_coreml_engine_integration() {
    let engine = CoreMlEngine::default();
    assert!(!engine.hardware_capabilities().device_name.is_empty());
    let desc = CoreMlModelDescriptor::new(
        "rraw_cfa_denoiser",
        CoreMlComputeUnits::All,
        vec![1, 1, 504, 504],
        vec![1, 1, 504, 504],
    );
    assert!(desc.is_valid());
}
