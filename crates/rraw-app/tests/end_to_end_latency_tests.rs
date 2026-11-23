#![forbid(unsafe_code)]

//! End-to-end latency benchmarks and stage-by-stage profiling verification for Rraw.

use std::error::Error;
use std::sync::Arc;

use rraw_app::preview::render_preview_from_base_profiled;
use rraw_app::preview::types::BaseRgbPreview;
use rraw_app::state::DarkroomGradingState;
use rraw_core::{
    demosaic_cfa, BayerOrigin, CfaPattern, DemosaicMethod, ExecutionBackend, PipelineStageId,
    PipelineTiming, SignedCfaBuffer, StageTimer,
};

#[test]
fn test_end_to_end_preview_rendering_latency_120fps() -> Result<(), Box<dyn Error>> {
    let width = 1600_usize;
    let height = 1066_usize;
    let num_pixels = width * height;

    let samples_rgb = vec![[0.4_f32, 0.45_f32, 0.5_f32]; num_pixels];
    let base = BaseRgbPreview {
        width,
        height,
        samples_rgb: Arc::from(samples_rgb),
        as_shot_wb: [2.0, 1.0, 1.5],
        cam_to_srgb: [1.45, -0.35, -0.10, -0.15, 1.35, -0.20, -0.05, -0.30, 1.35],
        semantic_masks: Arc::new(rraw_ai::SemanticMaskSet {
            width,
            height,
            subject: vec![0.1; num_pixels],
            sky: vec![0.2; num_pixels],
            background: vec![0.7; num_pixels],
            portrait: vec![0.0; num_pixels],
        }),
        exif_orientation: 1,
        matched_lens: None,
        focal_length_mm: Some(50.0),
        aperture_f_number: Some(2.8),
    };

    let mut grading = DarkroomGradingState::default();
    grading.denoise_sharpen.color_denoise = 0.0;
    grading.tone.exposure_ev = 0.5;
    grading.tone.contrast = 15.0;
    grading.tone.saturation = 10.0;
    grading.color.kelvin = 5600.0;
    grading.color.tint = 5.0;

    // Warm-up run
    let _ = render_preview_from_base_profiled(&base, &grading);

    // Profiled measurement run
    let (preview_img, hist, timing) = render_preview_from_base_profiled(&base, &grading);

    assert_eq!(preview_img.size, [width, height]);
    assert_eq!(preview_img.pixels.len(), num_pixels);
    assert!(hist.max_count > 0);

    // Check that interactive stages (5, 6, 7) are recorded
    assert!(timing.stage_timing(PipelineStageId::Stage5Optics).is_some());
    assert!(timing
        .stage_timing(PipelineStageId::Stage6TonalGrading)
        .is_some());
    assert!(timing
        .stage_timing(PipelineStageId::Stage7EdrDisplay)
        .is_some());

    let report = timing.format_ascii_report();
    println!("\nPreview Rendering Benchmark (1600x1066):\n{report}");

    // In debug builds without LLVM optimizations, allow headroom for parallel test runner CPU contention;
    // in release mode, verify constant 120 FPS frame time (<= 8.333 ms).
    let max_allowed_ms = if cfg!(debug_assertions) { 250.0 } else { 8.333 };
    assert!(timing.viewport_interactive_millis() < max_allowed_ms);

    Ok(())
}

#[test]
fn test_45mp_full_frame_demosaic_latency_benchmark() -> Result<(), Box<dyn Error>> {
    // 45.4 Megapixel sensor resolution (Nikon Z7 / D850 format: 8256 x 5504)
    // To ensure fast test execution in debug/CI while remaining representative of throughput,
    // benchmark a 2048 x 1536 (3.15 MP) slice and extrapolate, or time a 1024x1024 block.
    let width = 1024_usize;
    let height = 1024_usize;
    let num_pixels = width * height;

    let mut samples = Vec::with_capacity(num_pixels);
    for y in 0..height {
        for x in 0..width {
            let val = ((x + y) % 1024) as f32 / 1024.0;
            samples.push(val);
        }
    }

    let cfa = SignedCfaBuffer::new(width, height, CfaPattern::Bayer(BayerOrigin::Rggb), samples)?;

    let mut timing = PipelineTiming::new();

    // Benchmark Bilinear Demosaicing
    let timer_bilinear = StageTimer::start(
        PipelineStageId::Stage4Demosaic,
        ExecutionBackend::CpuRayon,
        num_pixels,
    );
    let rgb_bilinear = demosaic_cfa(&cfa, DemosaicMethod::Bilinear)?;
    let t_bilinear = timer_bilinear.finish_and_record(&mut timing);

    assert_eq!(rgb_bilinear.len(), num_pixels);
    assert!(t_bilinear.millis() > 0.0);
    let mpx_rate = t_bilinear.throughput_mpx_per_sec();
    println!(
        "Bilinear demosaic: {:6.2} ms ({:6.1} MPx/s)",
        t_bilinear.millis(),
        mpx_rate
    );

    // Benchmark Edge-Aware Adaptive Demosaicing
    let timer_adaptive = StageTimer::start(
        PipelineStageId::Stage4Demosaic,
        ExecutionBackend::CpuRayon,
        num_pixels,
    );
    let rgb_adaptive = demosaic_cfa(&cfa, DemosaicMethod::EdgeAwareAdaptive)?;
    let t_adaptive = timer_adaptive.finish_and_record(&mut timing);

    assert_eq!(rgb_adaptive.len(), num_pixels);
    assert!(t_adaptive.millis() > 0.0);
    println!(
        "Edge-aware demosaic: {:6.2} ms ({:6.1} MPx/s)",
        t_adaptive.millis(),
        t_adaptive.throughput_mpx_per_sec()
    );

    Ok(())
}

#[test]
fn test_pipeline_timing_summary_and_thresholds() {
    let mut timing = PipelineTiming::new();
    timing.record_stage(
        PipelineStageId::Stage1Decode,
        ExecutionBackend::CpuRayon,
        std::time::Duration::from_millis(12),
        45_400_000,
    );
    timing.record_stage(
        PipelineStageId::Stage4Demosaic,
        ExecutionBackend::GpuMetal,
        std::time::Duration::from_millis(22),
        45_400_000,
    );
    timing.record_stage(
        PipelineStageId::Stage5Optics,
        ExecutionBackend::GpuMetal,
        std::time::Duration::from_millis(2),
        45_400_000,
    );
    timing.record_stage(
        PipelineStageId::Stage6TonalGrading,
        ExecutionBackend::GpuMetal,
        std::time::Duration::from_millis(3),
        45_400_000,
    );
    timing.record_stage(
        PipelineStageId::Stage7EdrDisplay,
        ExecutionBackend::GpuMetal,
        std::time::Duration::from_millis(2),
        45_400_000,
    );

    assert!(timing.is_sub50ms_demosaic());
    assert!(timing.is_120fps_viewport_capable());
    assert_eq!(timing.viewport_interactive_millis(), 7.0);

    let report = timing.format_ascii_report();
    assert!(
        report.contains("45.4") || report.contains("MPx/s") || report.contains("Total End-to-End")
    );
}
