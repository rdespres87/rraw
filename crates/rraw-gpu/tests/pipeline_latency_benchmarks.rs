#![forbid(unsafe_code)]

//! Integration latency regression test suite for 7-stage GPU compute pipeline on Apple Silicon / Metal.

use std::error::Error;
use std::time::Duration;

use rraw_core::{
    BayerOrigin, CfaPattern, ExecutionBackend, PipelineStageId, PipelineTiming, Stage6MasksParams,
};
use rraw_gpu::{GpuContext, GpuPipelineProfiler, Stage5Params, Stage6Params, Stage7Params};
use rraw_optics::{OpticalCorrectionModel, UvLutGenerator};

#[test]
fn test_gpu_all_7_stages_profiled() -> Result<(), Box<dyn Error>> {
    let ctx = match GpuContext::new_headless() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("NOTICE: Headless GPU adapter unavailable in current CI harness ({e}); skipping GPU execution.");
            return Ok(());
        }
    };

    let profiler = GpuPipelineProfiler::new(&ctx)?;

    let width = 128_u32;
    let height = 128_u32;
    let num_pixels = (width * height) as usize;
    let raw_pixels = vec![2048_u16; num_pixels];
    let uv_lut = UvLutGenerator::generate(&OpticalCorrectionModel::default(), 16);

    let stage5 = Stage5Params::new(width, height, 0, 3, 0.25);
    let stage6 = Stage6Params {
        width,
        height,
        masks: Stage6MasksParams::default(),
    };
    let stage7 = Stage7Params {
        width,
        height,
        headroom: 2.0,
    };

    let (out_rgb, timing) = profiler.execute_full_profiled(
        &ctx,
        &raw_pixels,
        width,
        height,
        CfaPattern::Bayer(BayerOrigin::Rggb),
        [2.1, 1.0, 1.6],
        &uv_lut,
        &stage5,
        &stage6,
        &stage7,
    )?;

    assert_eq!(out_rgb.len(), num_pixels * 3);

    // Verify all discrete pipeline stages were recorded
    assert!(timing
        .stage_timing(PipelineStageId::Stage2Calibration)
        .is_some());
    assert!(timing
        .stage_timing(PipelineStageId::Stage4Demosaic)
        .is_some());
    assert!(timing
        .stage_timing(PipelineStageId::Stage3ColorWb)
        .is_some());
    assert!(timing.stage_timing(PipelineStageId::Stage5Optics).is_some());
    assert!(timing
        .stage_timing(PipelineStageId::Stage6TonalGrading)
        .is_some());
    assert!(timing
        .stage_timing(PipelineStageId::Stage7EdrDisplay)
        .is_some());

    assert!(timing.total_millis() > 0.0);

    let report = timing.format_ascii_report();
    assert!(report.contains("Stage 1: RAW Decode") || report.contains("Stage 2: Calibration"));
    assert!(report.contains("Stage 7: EDR Display"));
    println!("\n{report}");

    Ok(())
}

#[test]
fn test_gpu_interactive_viewport_benchmark() -> Result<(), Box<dyn Error>> {
    let ctx = match GpuContext::new_headless() {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "NOTICE: Headless GPU adapter unavailable ({e}); skipping interactive benchmark."
            );
            return Ok(());
        }
    };

    let profiler = GpuPipelineProfiler::new(&ctx)?;

    let width = 256_u32;
    let height = 256_u32;
    let num_pixels = (width * height) as usize;
    let demosaiced_rgb = vec![0.5_f32; num_pixels * 3];
    let uv_lut = UvLutGenerator::generate(&OpticalCorrectionModel::default(), 16);

    let stage5 = Stage5Params::new(width, height, 0, 3, 0.15);
    let stage6 = Stage6Params {
        width,
        height,
        masks: Stage6MasksParams::default(),
    };
    let stage7 = Stage7Params {
        width,
        height,
        headroom: 1.5,
    };

    let (display_rgb, timing) = profiler.execute_interactive_viewport_profiled(
        &ctx,
        &demosaiced_rgb,
        width,
        height,
        &uv_lut,
        &stage5,
        &stage6,
        &stage7,
    )?;

    assert_eq!(display_rgb.len(), num_pixels * 3);
    assert!(timing.stage_timing(PipelineStageId::Stage5Optics).is_some());
    assert!(timing
        .stage_timing(PipelineStageId::Stage6TonalGrading)
        .is_some());
    assert!(timing
        .stage_timing(PipelineStageId::Stage7EdrDisplay)
        .is_some());

    // In unoptimized debug builds or under CI runner contention / virtualization, allow headroom;
    // in release mode on dedicated silicon, enforce responsive interactive frame budget (< 16.67 ms / 60 FPS).
    let max_allowed_ms = if cfg!(debug_assertions) || std::env::var("CI").is_ok() {
        250.0
    } else {
        16.666
    };
    assert!(
        timing.viewport_interactive_millis() < max_allowed_ms,
        "Interactive viewport latency exceeded budget: {:.2} ms >= {:.2} ms",
        timing.viewport_interactive_millis(),
        max_allowed_ms
    );

    Ok(())
}

#[test]
fn test_pipeline_timing_invariants_and_ascii_table() {
    let mut timing = PipelineTiming::new();
    timing.record_stage(
        PipelineStageId::Stage1Decode,
        ExecutionBackend::CpuRayon,
        Duration::from_millis(10),
        45_000_000,
    );
    timing.record_stage(
        PipelineStageId::Stage2Calibration,
        ExecutionBackend::GpuMetal,
        Duration::from_millis(3),
        45_000_000,
    );
    timing.record_stage(
        PipelineStageId::Stage3ColorWb,
        ExecutionBackend::GpuMetal,
        Duration::from_millis(2),
        45_000_000,
    );
    timing.record_stage(
        PipelineStageId::Stage4Demosaic,
        ExecutionBackend::GpuMetal,
        Duration::from_millis(15),
        45_000_000,
    );
    timing.record_stage(
        PipelineStageId::Stage5Optics,
        ExecutionBackend::GpuMetal,
        Duration::from_millis(2),
        45_000_000,
    );
    timing.record_stage(
        PipelineStageId::Stage6TonalGrading,
        ExecutionBackend::GpuMetal,
        Duration::from_millis(3),
        45_000_000,
    );
    timing.record_stage(
        PipelineStageId::Stage7EdrDisplay,
        ExecutionBackend::GpuMetal,
        Duration::from_millis(2),
        45_000_000,
    );

    assert_eq!(timing.total_millis(), 37.0);
    // Viewport interactive = Stages 5 + 6 + 7 = 2 + 3 + 2 = 7 ms
    assert_eq!(timing.viewport_interactive_millis(), 7.0);
    assert!(timing.is_120fps_viewport_capable());
    assert!(timing.is_sub50ms_demosaic());

    let report = timing.format_ascii_report();
    assert!(report.contains("Total End-to-End Pipeline Latency"));
    assert!(report.contains("Interactive Darkroom Viewport (Stages 5-7)"));
    assert!(report.contains("PASS"));
}
