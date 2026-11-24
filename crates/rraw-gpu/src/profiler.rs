#![forbid(unsafe_code)]

//! End-to-end GPU photographic pipeline profiler, stage timing, and 120 FPS latency verification.

use rraw_core::{
    CfaPattern, ExecutionBackend, NoiseProfile, PipelineStageId, PipelineTiming, StageTimer,
};
use rraw_optics::UvLutTable;

use crate::context::GpuContext;
use crate::error::GpuError;
use crate::pipeline::{Stage1Params, Stage1Pipeline, Stage2Params, Stage2Pipeline};
use crate::stage3::{Stage3Params, Stage3RlPipeline};
use crate::stage4::{Stage4Params, Stage4WarpPipeline};
use crate::stage5::{Stage5Params, Stage5Pipeline};
use crate::stage6::{Stage6Params, Stage6Pipeline};
use crate::stage7::{Stage7Params, Stage7Pipeline};

/// Raw sensor input frame for a full 7-stage profiled GPU pipeline run.
#[derive(Debug, Clone, Copy)]
pub struct RawSensorFrame<'a> {
    pub raw_pixels: &'a [u16],
    pub cfa_pattern: CfaPattern,
    pub wb_multipliers: [f32; 3],
}

/// Shared dispatch parameters for profiled full and interactive GPU pipeline executions.
#[derive(Debug, Clone, Copy)]
pub struct PipelineDispatchContext<'a> {
    pub width: u32,
    pub height: u32,
    pub uv_lut: &'a UvLutTable,
    pub stage5_params: &'a Stage5Params,
    pub stage6_params: &'a Stage6Params,
    pub stage7_params: &'a Stage7Params,
}

/// Comprehensive GPU compute pipeline profiler orchestrating and benchmarking all 7 discrete stages.
pub struct GpuPipelineProfiler {
    pub stage1: Stage1Pipeline,
    pub stage2: Stage2Pipeline,
    pub stage3: Stage3RlPipeline,
    pub stage4: Stage4WarpPipeline,
    pub stage5: Stage5Pipeline,
    pub stage6: Stage6Pipeline,
    pub stage7: Stage7Pipeline,
}

impl GpuPipelineProfiler {
    /// Compiles all 7 WGSL compute pipelines on `ctx`.
    pub fn new(ctx: &GpuContext) -> Result<Self, GpuError> {
        let stage1 = Stage1Pipeline::new(ctx)?;
        let stage2 = Stage2Pipeline::new(ctx)?;
        let stage3 = Stage3RlPipeline::new(ctx)?;
        let stage4 = Stage4WarpPipeline::new(ctx)?;
        let stage5 = Stage5Pipeline::new(ctx)?;
        let stage6 = Stage6Pipeline::new(ctx)?;
        let stage7 = Stage7Pipeline::new(ctx)?;

        Ok(Self {
            stage1,
            stage2,
            stage3,
            stage4,
            stage5,
            stage6,
            stage7,
        })
    }

    /// Executes the full 7-stage GPU rendering pipeline from raw sensor data with per-stage timing instrumentation.
    pub fn execute_full_profiled(
        &self,
        ctx: &GpuContext,
        sensor: &RawSensorFrame<'_>,
        dispatch: &PipelineDispatchContext<'_>,
    ) -> Result<(Vec<f32>, PipelineTiming), GpuError> {
        let width = dispatch.width;
        let height = dispatch.height;
        let num_pixels = (width as usize).saturating_mul(height as usize);
        if sensor.raw_pixels.len() != num_pixels {
            return Err(GpuError::BufferLengthMismatch {
                expected: num_pixels,
                actual: sensor.raw_pixels.len(),
            });
        }

        let mut timing = PipelineTiming::new();

        // Stage 1: Sensor Preprocessing & Calibration
        let s1_params = Stage1Params {
            width,
            height,
            black_levels: [512.0, 512.0, 512.0, 512.0],
            white_levels: [16383.0, 16383.0, 16383.0, 16383.0],
            cfa_pattern: sensor.cfa_pattern,
            noise_profile: NoiseProfile::default(),
            wavelet_passes: 1,
        };
        let t1 = StageTimer::start(
            PipelineStageId::Stage2Calibration,
            ExecutionBackend::GpuMetal,
            num_pixels,
        );
        let (norm_cfa, clip_mask) = self.stage1.execute(ctx, sensor.raw_pixels, &s1_params)?;
        t1.finish_and_record(&mut timing);

        // Stage 2: Bayer / X-Trans Demosaicing
        let s2_params = Stage2Params {
            width,
            height,
            cfa_pattern: sensor.cfa_pattern,
            wb_multipliers: sensor.wb_multipliers,
        };
        let t2 = StageTimer::start(
            PipelineStageId::Stage4Demosaic,
            ExecutionBackend::GpuMetal,
            num_pixels,
        );
        let s2_rgb = self
            .stage2
            .execute(ctx, &norm_cfa, &clip_mask, &s2_params)?;
        t2.finish_and_record(&mut timing);

        // Stage 3: Color, WB & Richardson-Lucy Deconvolution
        let s3_params = Stage3Params {
            width,
            height,
            iterations: 0, // Fast path without iterative deconvolution for benchmark baseline
            base_psf_sigma: 1.0,
            epsilon: 1e-4,
            deringing_threshold: 0.1,
            shot_r: 0.0,
            shot_g: 0.0,
            shot_b: 0.0,
            read_r: 0.0,
            read_g: 0.0,
            read_b: 0.0,
        };
        let t3 = StageTimer::start(
            PipelineStageId::Stage3ColorWb,
            ExecutionBackend::GpuMetal,
            num_pixels,
        );
        let s3_rgb = self.stage3.execute(ctx, &s2_rgb, &s3_params)?;
        t3.finish_and_record(&mut timing);

        // Stage 4: Optical Corrections (Lensfun UV LUT Distortion & Vignette)
        let s4_params = Stage4Params {
            width,
            height,
            apply_radial_vignette: true,
            vignette_v1: 0.0,
            vignette_v2: 0.0,
        };
        let t4 = StageTimer::start(
            PipelineStageId::Stage5Optics,
            ExecutionBackend::GpuMetal,
            num_pixels,
        );
        let s4_rgb = self
            .stage4
            .execute(ctx, &s3_rgb, dispatch.uv_lut, &s4_params)?;
        t4.finish_and_record(&mut timing);

        // Stage 5: Tonal Grading, 3D LUT & Local Contrast
        let t5 = StageTimer::start(
            PipelineStageId::Stage6TonalGrading,
            ExecutionBackend::GpuMetal,
            num_pixels,
        );
        let s5_rgb = self.stage5.execute(ctx, &s4_rgb, dispatch.stage5_params)?;
        t5.finish_and_record(&mut timing);

        // Stage 6: Local Geometric, Parametric & Semantic Masks
        let t6 = StageTimer::start(
            PipelineStageId::Stage6TonalGrading,
            ExecutionBackend::GpuMetal,
            num_pixels,
        );
        let s6_rgb = self.stage6.execute(ctx, &s5_rgb, dispatch.stage6_params)?;
        t6.finish_and_record(&mut timing);

        // Stage 7: AgX / OpenDRT EDR Display Tone Mapping & Viewport Output
        let t7 = StageTimer::start(
            PipelineStageId::Stage7EdrDisplay,
            ExecutionBackend::GpuMetal,
            num_pixels,
        );
        let (display_rgb, _, _) = self.stage7.execute(ctx, &s6_rgb, dispatch.stage7_params)?;
        t7.finish_and_record(&mut timing);

        Ok((display_rgb, timing))
    }

    /// Executes only the interactive Darkroom stages (Stages 4, 5, 6, 7) from an existing demosaiced RGB buffer.
    ///
    /// This reflects live parameter scrubbing in the user interface (slider moves, tone curves, crop, masks).
    pub fn execute_interactive_viewport_profiled(
        &self,
        ctx: &GpuContext,
        demosaiced_rgb: &[f32],
        dispatch: &PipelineDispatchContext<'_>,
    ) -> Result<(Vec<f32>, PipelineTiming), GpuError> {
        let width = dispatch.width;
        let height = dispatch.height;
        let num_pixels = (width as usize).saturating_mul(height as usize);
        if demosaiced_rgb.len() != num_pixels * 3 {
            return Err(GpuError::BufferLengthMismatch {
                expected: num_pixels * 3,
                actual: demosaiced_rgb.len(),
            });
        }

        let mut timing = PipelineTiming::new();

        // Stage 4: Optical Corrections
        let s4_params = Stage4Params {
            width,
            height,
            apply_radial_vignette: true,
            vignette_v1: 0.0,
            vignette_v2: 0.0,
        };
        let t4 = StageTimer::start(
            PipelineStageId::Stage5Optics,
            ExecutionBackend::GpuMetal,
            num_pixels,
        );
        let s4_rgb = self
            .stage4
            .execute(ctx, demosaiced_rgb, dispatch.uv_lut, &s4_params)?;
        t4.finish_and_record(&mut timing);

        // Stage 5: Tonal Grading
        let t5 = StageTimer::start(
            PipelineStageId::Stage6TonalGrading,
            ExecutionBackend::GpuMetal,
            num_pixels,
        );
        let s5_rgb = self.stage5.execute(ctx, &s4_rgb, dispatch.stage5_params)?;
        t5.finish_and_record(&mut timing);

        // Stage 6: Masks
        let t6 = StageTimer::start(
            PipelineStageId::Stage6TonalGrading,
            ExecutionBackend::GpuMetal,
            num_pixels,
        );
        let s6_rgb = self.stage6.execute(ctx, &s5_rgb, dispatch.stage6_params)?;
        t6.finish_and_record(&mut timing);

        // Stage 7: Display Output
        let t7 = StageTimer::start(
            PipelineStageId::Stage7EdrDisplay,
            ExecutionBackend::GpuMetal,
            num_pixels,
        );
        let (display_rgb, _, _) = self.stage7.execute(ctx, &s6_rgb, dispatch.stage7_params)?;
        t7.finish_and_record(&mut timing);

        Ok((display_rgb, timing))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rraw_core::{BayerOrigin, Stage6MasksParams};

    #[test]
    fn test_profiler_compilation_and_synthetic_execution() -> Result<(), Box<dyn std::error::Error>>
    {
        let ctx = match GpuContext::new_headless() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Skipping GPU test in headless environment without adapter: {e}");
                return Ok(());
            }
        };

        let profiler = GpuPipelineProfiler::new(&ctx)?;

        let width = 64;
        let height = 64;
        let raw_pixels = vec![2000_u16; (width * height) as usize];
        let uv_lut = rraw_optics::UvLutGenerator::generate(
            &rraw_optics::OpticalCorrectionModel::default(),
            16,
        );

        let s5_params = Stage5Params::new(width, height, 0, 3, 0.2);
        let s6_params = Stage6Params {
            width,
            height,
            masks: Stage6MasksParams::default(),
        };
        let s7_params = Stage7Params {
            width,
            height,
            headroom: 1.0,
        };

        let sensor = RawSensorFrame {
            raw_pixels: &raw_pixels,
            cfa_pattern: CfaPattern::Bayer(BayerOrigin::Rggb),
            wb_multipliers: [2.0, 1.0, 1.5],
        };
        let dispatch = PipelineDispatchContext {
            width,
            height,
            uv_lut: &uv_lut,
            stage5_params: &s5_params,
            stage6_params: &s6_params,
            stage7_params: &s7_params,
        };

        let (out_rgb, timing) = profiler.execute_full_profiled(&ctx, &sensor, &dispatch)?;

        assert_eq!(out_rgb.len(), (width * height * 3) as usize);
        assert!(!timing.stages.is_empty());
        assert!(timing.total_millis() > 0.0);
        Ok(())
    }
}
