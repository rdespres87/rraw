#![forbid(unsafe_code)]

//! TDD integration tests for the live WGPU interactive preview backend
//! (`GpuPreviewEngine`, `can_use_gpu_fast_path`, automatic CPU fallback, and colorimetric parity).

use std::sync::Arc;

use glam::Vec3;
use rraw_ai::SemanticMaskSet;
use rraw_app::preview::{
    can_use_gpu_fast_path, render_preview_from_base_profiled, BaseRgbPreview, GpuPreviewEngine,
    PreviewBackendKind,
};
use rraw_app::state::{
    AppLayoutConfig, DarkroomGradingState, LocalMaskKind, LocalMaskLayer, OpticalCorrectionTab,
};
use rraw_color::colorchecker::delta_e_00_acescg;
use rraw_color::DisplayTransform;
use rraw_core::curves::CurvePoint;

fn synthetic_color_ramp_preview(width: usize, height: usize) -> BaseRgbPreview {
    let n = width * height;
    let mut samples = Vec::with_capacity(n);
    for y in 0..height {
        let fy = (y as f32 + 0.5) / height as f32;
        for x in 0..width {
            let fx = (x as f32 + 0.5) / width as f32;
            let r = (0.04 + 0.72 * fx).clamp(0.0, 1.0);
            let g = (0.05 + 0.65 * fy).clamp(0.0, 1.0);
            let b = (0.06 + 0.55 * (1.0 - 0.5 * (fx + fy))).clamp(0.0, 1.0);
            samples.push([r, g, b]);
        }
    }
    BaseRgbPreview {
        width,
        height,
        samples_rgb: Arc::from(samples),
        as_shot_wb: [1.15, 1.0, 1.25],
        cam_to_srgb: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        semantic_masks: Arc::new(SemanticMaskSet {
            width: 2,
            height: 2,
            subject: vec![0.9, 0.1, 0.2, 0.8],
            sky: vec![0.1, 0.9, 0.1, 0.1],
            background: vec![0.0, 0.0, 0.7, 0.1],
            portrait: vec![0.0, 0.0, 0.0, 0.0],
        }),
        exif_orientation: 1,
        matched_lens: None,
        focal_length_mm: Some(50.0),
        aperture_f_number: Some(2.8),
    }
}

fn srgb_u8_to_linear_vec3(px: egui::Color32) -> Vec3 {
    let to_lin = |c: u8| -> f32 { (c as f32 / 255.0).powf(2.2) };
    Vec3::new(to_lin(px.r()), to_lin(px.g()), to_lin(px.b()))
}

#[test]
fn can_use_gpu_fast_path_accepts_standard_grading_and_rejects_cpu_only_modes() {
    let base = synthetic_color_ramp_preview(16, 16);
    let mut grading = DarkroomGradingState::default();
    assert!(can_use_gpu_fast_path(&base, &grading));

    // Standard exposure, WB, contrast, saturation, shadows/highlights, and tone curves stay on GPU.
    grading.tone.exposure_ev = 0.75;
    grading.tone.contrast = 20.0;
    grading.tone.shadows = 25.0;
    grading.tone.highlights = -15.0;
    grading.tone.saturation = 15.0;
    grading.color.kelvin = 6200.0;
    assert!(can_use_gpu_fast_path(&base, &grading));

    // OpenDRT falls back to CPU.
    let mut opendrt = grading.clone();
    opendrt.color.display_transform = DisplayTransform::OpenDrt;
    assert!(!can_use_gpu_fast_path(&base, &opendrt));

    // Soft proofing falls back to CPU.
    let mut sp = grading.clone();
    sp.soft_proof.enabled = true;
    assert!(!can_use_gpu_fast_path(&base, &sp));

    // Rotation and optical distortion/vignette stay on GPU fast path (pre-Stage-5 warp).
    let mut rotated = grading.clone();
    rotated.geometry.rotation_degrees = 2.5;
    assert!(can_use_gpu_fast_path(&base, &rotated));

    let mut distorted = grading.clone();
    distorted.optics.optical_tab = OpticalCorrectionTab::Manual;
    distorted.optics.distortion_correction = true;
    distorted.optics.distortion_amount = 18.0;
    assert!(can_use_gpu_fast_path(&base, &distorted));

    // Optical defringe falls back to CPU.
    let mut defringed = grading.clone();
    defringed.optics.defringe_purple_amount = 25.0;
    assert!(!can_use_gpu_fast_path(&base, &defringed));

    // ANE neural denoise and RL capture sharpening fall back to CPU.
    let mut ane_denoised = grading.clone();
    ane_denoised.denoise_sharpen.ane_neural_denoise = true;
    ane_denoised.denoise_sharpen.ane_denoise_strength = 50.0;
    assert!(!can_use_gpu_fast_path(&base, &ane_denoised));

    let mut sharpened = grading.clone();
    sharpened.denoise_sharpen.capture_sharpening_amount = 1.2;
    assert!(!can_use_gpu_fast_path(&base, &sharpened));

    // Local gradient masks (including invert, contrast, saturation, temperature, overlay) stay on GPU;
    // freehand brush masks fall back to CPU.
    let mut adv_mask = grading.clone();
    adv_mask.masks.local_masks.push(LocalMaskLayer {
        id: "lin1".to_string(),
        name: "Grad".to_string(),
        kind: LocalMaskKind::LinearGradient {
            start: [0.0, 0.0],
            end: [0.0, 1.0],
        },
        enabled: true,
        invert: true,
        feather: 0.35,
        opacity: 0.9,
        exposure_ev: -0.5,
        contrast: 25.0,
        saturation: 18.0,
        temperature_shift: 15.0,
    });
    adv_mask.masks.show_local_mask_overlay = Some(0);
    assert!(can_use_gpu_fast_path(&base, &adv_mask));

    let mut brush_mask = grading.clone();
    brush_mask.masks.local_masks.push(LocalMaskLayer {
        id: "brush1".to_string(),
        name: "Brush".to_string(),
        kind: LocalMaskKind::FreehandBrush {
            dabs: Vec::new(),
            brush_radius: 0.05,
            brush_flow: 1.0,
            erase_mode: false,
        },
        enabled: true,
        invert: false,
        feather: 0.5,
        opacity: 1.0,
        exposure_ev: 0.5,
        contrast: 0.0,
        saturation: 0.0,
        temperature_shift: 0.0,
    });
    assert!(!can_use_gpu_fast_path(&base, &brush_mask));
}

#[test]
fn gpu_preview_engine_respects_disable_toggle_and_matches_cpu_bit_for_bit() {
    let base = synthetic_color_ramp_preview(16, 16);
    let mut grading = DarkroomGradingState::default();
    grading.tone.exposure_ev = 0.4;

    let mut engine = GpuPreviewEngine::new();
    let (img_disabled, hist_disabled, _timing, backend) =
        engine.render_preview_profiled(&base, &grading, false);
    assert_eq!(backend, PreviewBackendKind::CpuRayon);

    let (img_cpu, hist_cpu, _) = render_preview_from_base_profiled(&base, &grading);
    assert_eq!(img_disabled.size, img_cpu.size);
    assert_eq!(img_disabled.pixels, img_cpu.pixels);
    assert_eq!(hist_disabled.lum, hist_cpu.lum);
}

#[test]
fn gpu_preview_engine_auto_falls_back_for_opendrt_and_soft_proof() {
    let base = synthetic_color_ramp_preview(12, 12);
    let mut engine = GpuPreviewEngine::new();

    let mut opendrt = DarkroomGradingState::default();
    opendrt.color.display_transform = DisplayTransform::OpenDrt;
    let (_, _, _, backend_drt) = engine.render_preview_profiled(&base, &opendrt, true);
    assert_eq!(backend_drt, PreviewBackendKind::CpuRayon);

    let mut soft_proof = DarkroomGradingState::default();
    soft_proof.soft_proof.enabled = true;
    let (_, _, _, backend_sp) = engine.render_preview_profiled(&base, &soft_proof, true);
    assert_eq!(backend_sp, PreviewBackendKind::CpuRayon);
}

#[test]
fn gpu_preview_engine_matches_cpu_within_delta_e_1_0() {
    let base = synthetic_color_ramp_preview(24, 24);
    let mut engine = GpuPreviewEngine::new();
    if !engine.ensure_initialized() {
        // Headless CI without a GPU adapter must fall back gracefully without failing.
        let grading = DarkroomGradingState::default();
        let (_, _, _, backend) = engine.render_preview_profiled(&base, &grading, true);
        assert_eq!(backend, PreviewBackendKind::CpuRayon);
        return;
    }

    let mut curved_grading = DarkroomGradingState::default();
    curved_grading.tone.exposure_ev = 0.35;
    curved_grading.tone.contrast = 12.0;
    curved_grading.tone.shadows = 15.0;
    curved_grading.tone.highlights = -10.0;
    curved_grading.tone.vibrance = 10.0;
    curved_grading.color.kelvin = 5800.0;
    curved_grading.tone_curves.combined_rgb.points = vec![
        CurvePoint { x: 0.0, y: 0.0 },
        CurvePoint { x: 0.25, y: 0.22 },
        CurvePoint { x: 0.75, y: 0.79 },
        CurvePoint { x: 1.0, y: 1.0 },
    ];

    let mut advanced_masks = DarkroomGradingState::default();
    advanced_masks.masks.local_masks.push(LocalMaskLayer {
        id: "rad_inv".to_string(),
        name: "Radial".to_string(),
        kind: LocalMaskKind::RadialGradient {
            center: [0.5, 0.5],
            radius: [0.35, 0.3],
            rotation_deg: 15.0,
        },
        enabled: true,
        invert: true,
        feather: 0.4,
        opacity: 0.85,
        exposure_ev: -0.4,
        contrast: 20.0,
        saturation: 15.0,
        temperature_shift: 12.0,
    });
    advanced_masks.masks.local_masks.push(LocalMaskLayer {
        id: "lin_warm".to_string(),
        name: "Linear".to_string(),
        kind: LocalMaskKind::LinearGradient {
            start: [0.0, 0.1],
            end: [0.0, 0.9],
        },
        enabled: true,
        invert: false,
        feather: 0.5,
        opacity: 0.7,
        exposure_ev: 0.25,
        contrast: -10.0,
        saturation: -12.0,
        temperature_shift: -15.0,
    });

    let mut overlay_masks = advanced_masks.clone();
    overlay_masks.masks.show_local_mask_overlay = Some(0);

    let scenarios = [
        DarkroomGradingState::default(),
        curved_grading,
        advanced_masks,
        overlay_masks,
    ];
    for grading in &scenarios {
        let (gpu_img, gpu_hist, _gpu_timing, backend) =
            engine.render_preview_profiled(&base, grading, true);
        assert_eq!(backend, PreviewBackendKind::GpuWgpu);

        let (cpu_img, cpu_hist, _cpu_timing) = render_preview_from_base_profiled(&base, grading);
        assert_eq!(gpu_img.size, cpu_img.size);
        assert_eq!(gpu_img.pixels.len(), cpu_img.pixels.len());

        let mut max_de = 0.0_f32;
        for (&gp, &cp) in gpu_img.pixels.iter().zip(cpu_img.pixels.iter()) {
            let de = delta_e_00_acescg(srgb_u8_to_linear_vec3(gp), srgb_u8_to_linear_vec3(cp));
            if de > max_de {
                max_de = de;
            }
        }
        assert!(
            max_de < 1.0,
            "GPU vs CPU preview colorimetric parity exceeded DeltaE00 < 1.0 (max_de = {max_de:.4})"
        );

        let gpu_total: u32 = gpu_hist.lum.iter().sum();
        let cpu_total: u32 = cpu_hist.lum.iter().sum();
        assert_eq!(gpu_total, cpu_total);
    }
}

#[test]
fn layout_config_defaults_to_gpu_preview_enabled_and_deserializes_legacy_json() {
    let default_cfg = AppLayoutConfig::default();
    assert!(default_cfg.gpu_preview_enabled);

    // Legacy layout.json without `gpu_preview_enabled` must default to `true`.
    let legacy_json = r#"{"section_order":[]}"#;
    let parsed: Result<AppLayoutConfig, _> = serde_json::from_str(legacy_json);
    assert!(parsed.is_ok());
    if let Ok(cfg) = parsed {
        assert!(cfg.gpu_preview_enabled);
    }
}
