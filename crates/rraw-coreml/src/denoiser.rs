#![deny(unsafe_code)]

//! Hardware Apple Neural Engine (ANE) 504x504 CFA RAW tile denoiser.

use crate::engine::CoreMlEngine;
use rraw_ai::{anscombe_transform, inverse_anscombe_transform, AnscombeParams, NeuralTileDenoiser};
use rraw_core::{AiError, NoiseProfile};

/// Apple Neural Engine hardware-accelerated CFA RAW tile denoiser.
///
/// Operates on fixed 504x504 Bayer or X-Trans RAW tiles. Stabilizes Poisson-Gaussian
/// noise using the Generalized Anscombe Transform (GAT) before feeding tensor data
/// to the ANE model, and applies exact inverse GAT reconstruction afterwards.
#[derive(Debug, Clone, Default)]
pub struct AneRawDenoiser {
    engine: CoreMlEngine,
}

impl AneRawDenoiser {
    /// Nominal fixed ANE inference tile dimension in pixels.
    pub const TILE_SIZE: usize = 504;

    /// Total samples in a single 504x504 CFA tile.
    pub const TILE_ELEMENTS: usize = Self::TILE_SIZE * Self::TILE_SIZE;

    /// Creates a new ANE RAW tile denoiser with default hardware tuning.
    #[must_use]
    pub fn new() -> Self {
        Self {
            engine: CoreMlEngine::new(),
        }
    }

    /// Attaches an explicitly configured CoreML execution engine.
    #[must_use]
    pub const fn with_engine(engine: CoreMlEngine) -> Self {
        Self { engine }
    }

    /// Returns a reference to the underlying CoreML engine.
    #[must_use]
    pub fn engine(&self) -> &CoreMlEngine {
        &self.engine
    }
}

impl NeuralTileDenoiser for AneRawDenoiser {
    fn denoise_tile(
        &self,
        tile: &[f32],
        width: usize,
        height: usize,
        noise: &NoiseProfile,
    ) -> Result<Vec<f32>, AiError> {
        if width != Self::TILE_SIZE || height != Self::TILE_SIZE {
            return Err(AiError::InvalidInput(format!(
                "invalid tile dimensions: expected {}x{}, got {width}x{height}",
                Self::TILE_SIZE,
                Self::TILE_SIZE
            )));
        }

        if tile.len() != Self::TILE_ELEMENTS {
            return Err(AiError::InvalidInput(format!(
                "tile length mismatch: expected {}, got {}",
                Self::TILE_ELEMENTS,
                tile.len()
            )));
        }

        // Derive Anscombe parameters by aggregating per-channel noise calibration
        let alphas = noise.shot_gain_alpha();
        let sigmas = noise.read_variance_sigma2();
        let alpha_mean = alphas.iter().copied().sum::<f32>() / (alphas.len().max(1) as f32);
        let sigma_mean = sigmas.iter().copied().sum::<f32>() / (sigmas.len().max(1) as f32);
        let anscombe_params = AnscombeParams::new(alpha_mean, sigma_mean);

        // 1. Forward Generalized Anscombe Transform (variance stabilization)
        let mut stabilized = Vec::with_capacity(tile.len());
        for &val in tile {
            let finite_val = if val.is_finite() { val.max(0.0) } else { 0.0 };
            stabilized.push(anscombe_transform(finite_val, &anscombe_params));
        }

        // 2. Hardware ANE inference pass
        let denoised_stabilized = self.engine.run_tile_denoise(&stabilized, width, height)?;

        // 3. Inverse Generalized Anscombe Transform back to physical linear signal
        let mut out = Vec::with_capacity(denoised_stabilized.len());
        for &val in &denoised_stabilized {
            let reconstructed = inverse_anscombe_transform(val, &anscombe_params);
            out.push(if reconstructed.is_finite() {
                reconstructed.max(0.0)
            } else {
                0.0
            });
        }

        Ok(out)
    }
}
