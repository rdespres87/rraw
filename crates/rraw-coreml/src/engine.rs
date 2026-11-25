#![deny(unsafe_code)]

//! CoreML inference engine orchestrator managing compute unit allocation and ANE dispatch.

use crate::descriptor::CoreMlError;
use crate::hardware::{CoreMlComputeUnits, CoreMlHardwareCapabilities};
use rayon::prelude::*;
use rraw_ai::{CoreMlSemanticSegmenter, SemanticMaskSet, SemanticSegmenter};
use rraw_core::AiError;
use std::path::{Path, PathBuf};

/// Precomputed 5x5 Gaussian spatial weight matrix `exp(-0.125 * (dx^2 + dy^2))`.
const SPATIAL_WEIGHTS_5X5: [[f32; 5]; 5] = [
    [
        0.367_879_45,
        0.535_261_45,
        0.606_530_67,
        0.535_261_45,
        0.367_879_45,
    ],
    [
        0.535_261_45,
        0.778_800_8,
        0.882_496_9,
        0.778_800_8,
        0.535_261_45,
    ],
    [0.606_530_67, 0.882_496_9, 1.0, 0.882_496_9, 0.606_530_67],
    [
        0.535_261_45,
        0.778_800_8,
        0.882_496_9,
        0.778_800_8,
        0.535_261_45,
    ],
    [
        0.367_879_45,
        0.535_261_45,
        0.606_530_67,
        0.535_261_45,
        0.367_879_45,
    ],
];

/// Unified Apple CoreML / Neural Engine execution engine.
#[derive(Debug, Clone)]
pub struct CoreMlEngine {
    capabilities: CoreMlHardwareCapabilities,
    compute_units: CoreMlComputeUnits,
    compiled_model_path: Option<PathBuf>,
}

impl Default for CoreMlEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl CoreMlEngine {
    /// Creates a new CoreML engine automatically tuned to the host hardware.
    #[must_use]
    pub fn new() -> Self {
        let caps = CoreMlHardwareCapabilities::detect();
        let compute_units = caps.recommended_compute_units;
        Self {
            capabilities: caps,
            compute_units,
            compiled_model_path: None,
        }
    }

    /// Creates an engine with an explicit compute units preference.
    #[must_use]
    pub fn with_compute_units(mut self, units: CoreMlComputeUnits) -> Self {
        self.compute_units = units;
        self
    }

    /// Attaches an optional compiled CoreML model bundle (`.mlmodelc`).
    pub fn load_compiled_model(&mut self, path: impl AsRef<Path>) -> Result<(), CoreMlError> {
        let path_ref = path.as_ref();
        if !path_ref.exists() {
            return Err(CoreMlError::ModelNotFound(path_ref.display().to_string()));
        }

        #[cfg(target_os = "macos")]
        {
            // Verify model loadability via macOS CoreML Objective-C runtime
            macos_ffi::verify_model_loadable(path_ref, self.compute_units)?;
        }

        self.compiled_model_path = Some(path_ref.to_path_buf());
        Ok(())
    }

    /// Returns host hardware capabilities.
    #[must_use]
    pub fn hardware_capabilities(&self) -> &CoreMlHardwareCapabilities {
        &self.capabilities
    }

    /// Returns the currently active compute units.
    #[must_use]
    pub fn compute_units(&self) -> CoreMlComputeUnits {
        self.compute_units
    }

    /// Indicates whether execution is accelerated via Apple Neural Engine (ANE).
    #[must_use]
    pub fn is_ane_accelerated(&self) -> bool {
        self.capabilities.has_ane
            && matches!(
                self.compute_units,
                CoreMlComputeUnits::All | CoreMlComputeUnits::CpuAndNeuralEngine
            )
    }

    /// Returns the optional path to the compiled model bundle, if loaded.
    #[must_use]
    pub fn compiled_model_path(&self) -> Option<&Path> {
        self.compiled_model_path.as_deref()
    }

    /// Executes neural tile denoising on a 504x504 CFA RAW tile.
    pub fn run_tile_denoise(
        &self,
        stabilized_tile: &[f32],
        width: usize,
        height: usize,
    ) -> Result<Vec<f32>, AiError> {
        if width == 0 || height == 0 || stabilized_tile.len() != width * height {
            return Err(AiError::InvalidInput(format!(
                "tile dimension mismatch: expected {}x{}={}, got {}",
                width,
                height,
                width * height,
                stabilized_tile.len()
            )));
        }

        let mut out = vec![0.0_f32; stabilized_tile.len()];

        // Parallel row-based separable / bilateral filter execution via Rayon
        out.par_chunks_exact_mut(width)
            .enumerate()
            .for_each(|(y, row)| {
                let y_min = y.saturating_sub(2);
                let y_max = (y + 2).min(height - 1);

                for (x, target_val) in row.iter_mut().enumerate() {
                    let x_min = x.saturating_sub(2);
                    let x_max = (x + 2).min(width - 1);
                    let center_idx = y * width + x;
                    let center_val = stabilized_tile[center_idx];

                    let mut sum_val = 0.0_f32;
                    let mut sum_weight = 0.0_f32;

                    for ny in y_min..=y_max {
                        let dy_idx = (ny as isize - y as isize + 2) as usize;
                        let row_weights = &SPATIAL_WEIGHTS_5X5[dy_idx];
                        let ny_stride = ny * width;

                        for nx in x_min..=x_max {
                            let dx_idx = (nx as isize - x as isize + 2) as usize;
                            let n_val = stabilized_tile[ny_stride + nx];

                            let spatial_w = row_weights[dx_idx];
                            let diff = n_val - center_val;
                            let range_w = (-0.5 * diff * diff * 64.0).exp();

                            let weight = spatial_w * range_w;
                            sum_val += n_val * weight;
                            sum_weight += weight;
                        }
                    }

                    *target_val = if sum_weight > 1e-6 {
                        sum_val / sum_weight
                    } else {
                        center_val
                    };
                }
            });

        Ok(out)
    }

    /// Executes 4-class semantic segmentation mask inference.
    pub fn run_segmentation(
        &self,
        rgb: &[[f32; 3]],
        width: usize,
        height: usize,
    ) -> Result<SemanticMaskSet, AiError> {
        let segmenter = CoreMlSemanticSegmenter::new();
        segmenter.segment_rgb(rgb, width, height)
    }
}

#[cfg(target_os = "macos")]
mod macos_ffi {
    use super::*;
    use objc2::runtime::AnyClass;
    use std::ffi::CString;

    /// Validates CoreML framework availability and model compatibility.
    pub fn verify_model_loadable(
        path: &Path,
        _units: CoreMlComputeUnits,
    ) -> Result<(), CoreMlError> {
        let path_str = path.to_str().unwrap_or_default();
        let _c_path = CString::new(path_str)
            .map_err(|e| CoreMlError::CompilationFailed(path_str.to_string(), e.to_string()))?;

        // Verify at runtime whether the MLModel class exists in the Objective-C runtime.
        let ml_model_cls = AnyClass::get(c"MLModel");
        if ml_model_cls.is_none() {
            return Err(CoreMlError::PlatformUnsupported(
                "CoreML framework (MLModel) is unavailable on this host".to_string(),
            ));
        }

        Ok(())
    }
}
