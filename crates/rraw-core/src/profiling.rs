#![forbid(unsafe_code)]

//! End-to-end rendering pipeline timing instrumentation, stage metrics, and latency regression tracking.

use std::fmt;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Target maximum frame time for 120 FPS EDR viewport updates (8.333 ms).
pub const TARGET_120FPS_FRAME_TIME_MS: f64 = 1000.0 / 120.0;

/// Target maximum demosaicing time for full-resolution 45 MP sensor frames on Apple Silicon (50.0 ms).
pub const TARGET_45MP_DEMOSAIC_MAX_MS: f64 = 50.0;

/// Identification of the 7 discrete photographic rendering pipeline stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PipelineStageId {
    /// Stage 1: Pure-Rust RAW container decoding, IFD parsing, and metadata extraction.
    Stage1Decode,
    /// Stage 2: Sensor calibration, black/white level normalization, and defect pixel interpolation.
    Stage2Calibration,
    /// Stage 3: As-shot white balance multiplier scaling and camera-to-sRGB matrix transformation.
    Stage3ColorWb,
    /// Stage 4: CFA demosaicing (Bilinear, Edge-Aware Adaptive, AHD, or Apple Neural Engine).
    Stage4Demosaic,
    /// Stage 5: Optical corrections (Lensfun distortion, vignetting, chromatic aberration / UV LUT).
    Stage5Optics,
    /// Stage 6: Tonal grading (PCHIP spline curves, exposure, contrast, saturation) and AI mask blending.
    Stage6TonalGrading,
    /// Stage 7: EDR display tone mapping (AgX / OpenDRT) and CAMetalLayer viewport presentation.
    Stage7EdrDisplay,
}

impl PipelineStageId {
    /// Short programmatic identifier for logging and benchmarks.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Stage1Decode => "Stage 1: RAW Decode",
            Self::Stage2Calibration => "Stage 2: Calibration",
            Self::Stage3ColorWb => "Stage 3: Color & WB",
            Self::Stage4Demosaic => "Stage 4: Demosaic",
            Self::Stage5Optics => "Stage 5: Optical UV LUT",
            Self::Stage6TonalGrading => "Stage 6: Tonal & Masks",
            Self::Stage7EdrDisplay => "Stage 7: EDR Display",
        }
    }

    /// Descriptive summary of the stage responsibility.
    #[must_use]
    pub const fn description(&self) -> &'static str {
        match self {
            Self::Stage1Decode => "Extract raw sensor data, TIFF IFDs, and EXIF/MakerNote tags",
            Self::Stage2Calibration => {
                "Apply black/white levels, active area crop, and defect repair"
            }
            Self::Stage3ColorWb => "Apply camera white balance and primary color space matrix",
            Self::Stage4Demosaic => "Reconstruct full continuous RGB from Bayer or X-Trans mosaic",
            Self::Stage5Optics => {
                "Correct lens geometric distortion, vignetting, and TCA via UV LUT"
            }
            Self::Stage6TonalGrading => {
                "Evaluate tone curves, grading sliders, and AI semantic masks"
            }
            Self::Stage7EdrDisplay => "AgX/OpenDRT display transform and EDR headroom tone mapping",
        }
    }

    /// Whether this stage runs interactively per-frame during Darkroom scrubbing (Stages 5, 6, 7).
    #[must_use]
    pub const fn is_interactive_darkroom(&self) -> bool {
        matches!(
            self,
            Self::Stage5Optics | Self::Stage6TonalGrading | Self::Stage7EdrDisplay
        )
    }
}

impl fmt::Display for PipelineStageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Execution hardware engine / backend for a pipeline stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExecutionBackend {
    /// Pure Rust single-threaded CPU execution.
    CpuScalar,
    /// Pure Rust multithreaded Rayon work-stealing parallel execution.
    CpuRayon,
    /// Apple Silicon Metal / WGPU compute shader execution.
    GpuMetal,
    /// Apple Silicon Neural Engine (ANE) hardware inference bridge.
    AppleNeuralEngine,
}

impl ExecutionBackend {
    /// Short label for reports and displays.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::CpuScalar => "CPU (Scalar)",
            Self::CpuRayon => "CPU (Rayon)",
            Self::GpuMetal => "GPU (Metal)",
            Self::AppleNeuralEngine => "Apple Neural Engine (ANE)",
        }
    }
}

impl fmt::Display for ExecutionBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Granular execution metrics for an individual pipeline stage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageTiming {
    pub stage: PipelineStageId,
    pub backend: ExecutionBackend,
    pub duration: Duration,
    pub pixels_processed: usize,
}

impl StageTiming {
    /// Creates a new stage timing record.
    #[must_use]
    pub fn new(
        stage: PipelineStageId,
        backend: ExecutionBackend,
        duration: Duration,
        pixels_processed: usize,
    ) -> Self {
        Self {
            stage,
            backend,
            duration,
            pixels_processed,
        }
    }

    /// Duration in milliseconds.
    #[must_use]
    pub fn millis(&self) -> f64 {
        self.duration.as_secs_f64() * 1_000.0
    }

    /// Duration in microseconds.
    #[must_use]
    pub fn micros(&self) -> f64 {
        self.duration.as_secs_f64() * 1_000_000.0
    }

    /// Compute throughput in Megapixels per second (MPx/s).
    #[must_use]
    pub fn throughput_mpx_per_sec(&self) -> f64 {
        let secs = self.duration.as_secs_f64();
        if secs <= 0.0 || self.pixels_processed == 0 {
            0.0
        } else {
            (self.pixels_processed as f64 / 1_000_000.0) / secs
        }
    }

    /// Whether this individual stage executed within a specific latency threshold in milliseconds.
    #[must_use]
    pub fn within_budget_ms(&self, max_ms: f64) -> bool {
        self.millis() <= max_ms
    }
}

/// Aggregated multi-stage timing report for an end-to-end photographic pipeline run.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PipelineTiming {
    pub stages: Vec<StageTiming>,
    pub total_duration: Duration,
}

impl PipelineTiming {
    /// Creates an empty pipeline timing accumulator.
    #[must_use]
    pub fn new() -> Self {
        Self {
            stages: Vec::new(),
            total_duration: Duration::ZERO,
        }
    }

    /// Records the timing of a completed stage and accumulates the total duration.
    pub fn record_stage(
        &mut self,
        stage: PipelineStageId,
        backend: ExecutionBackend,
        duration: Duration,
        pixels_processed: usize,
    ) {
        let timing = StageTiming::new(stage, backend, duration, pixels_processed);
        self.total_duration += duration;
        self.stages.push(timing);
    }

    /// Appends an existing `StageTiming` record.
    pub fn add_stage_timing(&mut self, timing: StageTiming) {
        self.total_duration += timing.duration;
        self.stages.push(timing);
    }

    /// Looks up timing for a specific stage, if recorded.
    #[must_use]
    pub fn stage_timing(&self, stage: PipelineStageId) -> Option<&StageTiming> {
        self.stages.iter().find(|t| t.stage == stage)
    }

    /// Total accumulated execution time in milliseconds.
    #[must_use]
    pub fn total_millis(&self) -> f64 {
        self.total_duration.as_secs_f64() * 1_000.0
    }

    /// Cumulative execution time for darkroom interactive stages (Stages 5, 6, and 7) in milliseconds.
    #[must_use]
    pub fn viewport_interactive_millis(&self) -> f64 {
        self.stages
            .iter()
            .filter(|t| t.stage.is_interactive_darkroom())
            .map(StageTiming::millis)
            .sum()
    }

    /// Returns `true` if the interactive Darkroom stages (5, 6, 7) execute within the 8.333 ms budget for 120 FPS.
    #[must_use]
    pub fn is_120fps_viewport_capable(&self) -> bool {
        let interactive_ms = self.viewport_interactive_millis();
        interactive_ms <= TARGET_120FPS_FRAME_TIME_MS
    }

    /// Returns `true` if demosaicing (Stage 4) executed within the 50 ms budget on full-frame sensors.
    #[must_use]
    pub fn is_sub50ms_demosaic(&self) -> bool {
        match self.stage_timing(PipelineStageId::Stage4Demosaic) {
            Some(t) => t.millis() <= TARGET_45MP_DEMOSAIC_MAX_MS,
            None => true,
        }
    }

    /// Formats an ASCII table detailing execution times, hardware backends, and throughput across all stages.
    #[must_use]
    pub fn format_ascii_report(&self) -> String {
        let mut out = String::new();
        out.push_str(
            "┌────────────────────────┬──────────────────────┬─────────────┬──────────────┐\n",
        );
        out.push_str(
            "│ Pipeline Stage         │ Backend Hardware     │ Latency     │ Throughput   │\n",
        );
        out.push_str(
            "├────────────────────────┼──────────────────────┼─────────────┼──────────────┤\n",
        );

        for t in &self.stages {
            let mpx = t.throughput_mpx_per_sec();
            let throughput_str = if mpx > 0.0 {
                format!("{mpx:6.1} MPx/s")
            } else {
                "     N/A    ".to_string()
            };
            out.push_str(&format!(
                "│ {:<22} │ {:<20} │ {:7.2} ms │ {:>12} │\n",
                t.stage.name(),
                t.backend.name(),
                t.millis(),
                throughput_str,
            ));
        }

        out.push_str(
            "├────────────────────────┴──────────────────────┼─────────────┼──────────────┤\n",
        );
        out.push_str(&format!(
            "│ Total End-to-End Pipeline Latency              │ {:7.2} ms │              │\n",
            self.total_millis()
        ));
        out.push_str(&format!(
            "│ Interactive Darkroom Viewport (Stages 5-7)     │ {:7.2} ms │ (120Hz: {:<4}) │\n",
            self.viewport_interactive_millis(),
            if self.is_120fps_viewport_capable() {
                "PASS"
            } else {
                "FAIL"
            }
        ));
        out.push_str(
            "└───────────────────────────────────────────────┴─────────────┴──────────────┘\n",
        );
        out
    }
}

/// RAII / Instant stopwatch timer for instrumenting a pipeline stage.
#[derive(Debug)]
pub struct StageTimer {
    stage: PipelineStageId,
    backend: ExecutionBackend,
    pixels: usize,
    start: Instant,
}

impl StageTimer {
    /// Starts a timer for `stage` running on `backend` processing `pixels`.
    #[must_use]
    pub fn start(stage: PipelineStageId, backend: ExecutionBackend, pixels: usize) -> Self {
        Self {
            stage,
            backend,
            pixels,
            start: Instant::now(),
        }
    }

    /// Stops the timer and returns the recorded `StageTiming`.
    #[must_use]
    pub fn finish(self) -> StageTiming {
        let elapsed = self.start.elapsed();
        StageTiming::new(self.stage, self.backend, elapsed, self.pixels)
    }

    /// Stops the timer and records the result directly into `pipeline`.
    pub fn finish_and_record(self, pipeline: &mut PipelineTiming) -> StageTiming {
        let timing = self.finish();
        pipeline.add_stage_timing(timing.clone());
        timing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stage_id_names_and_properties() {
        let stages = [
            PipelineStageId::Stage1Decode,
            PipelineStageId::Stage2Calibration,
            PipelineStageId::Stage3ColorWb,
            PipelineStageId::Stage4Demosaic,
            PipelineStageId::Stage5Optics,
            PipelineStageId::Stage6TonalGrading,
            PipelineStageId::Stage7EdrDisplay,
        ];

        for stage in stages {
            assert!(!stage.name().is_empty());
            assert!(!stage.description().is_empty());
            assert_eq!(format!("{stage}"), stage.name());
        }

        assert!(!PipelineStageId::Stage1Decode.is_interactive_darkroom());
        assert!(!PipelineStageId::Stage2Calibration.is_interactive_darkroom());
        assert!(!PipelineStageId::Stage3ColorWb.is_interactive_darkroom());
        assert!(!PipelineStageId::Stage4Demosaic.is_interactive_darkroom());
        assert!(PipelineStageId::Stage5Optics.is_interactive_darkroom());
        assert!(PipelineStageId::Stage6TonalGrading.is_interactive_darkroom());
        assert!(PipelineStageId::Stage7EdrDisplay.is_interactive_darkroom());
    }

    #[test]
    fn test_stage_timing_throughput() {
        let timing = StageTiming::new(
            PipelineStageId::Stage4Demosaic,
            ExecutionBackend::GpuMetal,
            Duration::from_millis(20),
            45_000_000,
        );

        assert_eq!(timing.millis(), 20.0);
        assert_eq!(timing.micros(), 20_000.0);
        // 45 MP in 20 ms => 2250 MPx/s
        assert!((timing.throughput_mpx_per_sec() - 2250.0).abs() < 1e-2);
        assert!(timing.within_budget_ms(50.0));
        assert!(!timing.within_budget_ms(10.0));
    }

    #[test]
    fn test_pipeline_timing_budget_checks() {
        let mut report = PipelineTiming::new();
        report.record_stage(
            PipelineStageId::Stage1Decode,
            ExecutionBackend::CpuRayon,
            Duration::from_millis(15),
            45_000_000,
        );
        report.record_stage(
            PipelineStageId::Stage2Calibration,
            ExecutionBackend::GpuMetal,
            Duration::from_millis(5),
            45_000_000,
        );
        report.record_stage(
            PipelineStageId::Stage3ColorWb,
            ExecutionBackend::GpuMetal,
            Duration::from_millis(3),
            45_000_000,
        );
        report.record_stage(
            PipelineStageId::Stage4Demosaic,
            ExecutionBackend::GpuMetal,
            Duration::from_millis(25),
            45_000_000,
        );
        report.record_stage(
            PipelineStageId::Stage5Optics,
            ExecutionBackend::GpuMetal,
            Duration::from_millis(2),
            45_000_000,
        );
        report.record_stage(
            PipelineStageId::Stage6TonalGrading,
            ExecutionBackend::GpuMetal,
            Duration::from_millis(3),
            45_000_000,
        );
        report.record_stage(
            PipelineStageId::Stage7EdrDisplay,
            ExecutionBackend::GpuMetal,
            Duration::from_millis(2),
            45_000_000,
        );

        assert_eq!(report.total_millis(), 55.0);
        // Interactive viewport is Stages 5, 6, 7 = 2 + 3 + 2 = 7 ms
        assert_eq!(report.viewport_interactive_millis(), 7.0);
        assert!(report.is_120fps_viewport_capable());
        assert!(report.is_sub50ms_demosaic());

        let ascii = report.format_ascii_report();
        assert!(ascii.contains("Stage 1: RAW Decode"));
        assert!(ascii.contains("Stage 7: EDR Display"));
        assert!(ascii.contains("PASS"));
    }

    #[test]
    fn test_stage_timer_recording() {
        let mut report = PipelineTiming::new();
        let timer = StageTimer::start(
            PipelineStageId::Stage4Demosaic,
            ExecutionBackend::CpuRayon,
            1_000_000,
        );
        let timing = timer.finish_and_record(&mut report);

        assert_eq!(timing.stage, PipelineStageId::Stage4Demosaic);
        assert_eq!(report.stages.len(), 1);
        assert_eq!(
            report
                .stage_timing(PipelineStageId::Stage4Demosaic)
                .map(|t| t.backend),
            Some(ExecutionBackend::CpuRayon)
        );
    }
}
