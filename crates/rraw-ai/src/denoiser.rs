#![forbid(unsafe_code)]

//! Neural tile denoiser abstractions and deterministic simulation engines.

use rraw_core::{AiError, NoiseProfile};

/// Trait implemented by neural network CFA denoisers (CoreML, ONNX, or simulation).
pub trait NeuralTileDenoiser: Send + Sync {
    /// Denoises a single fixed-size CFA tile in-place or returning a fresh tensor.
    fn denoise_tile(
        &self,
        tile: &[f32],
        width: usize,
        height: usize,
        noise: &NoiseProfile,
    ) -> Result<Vec<f32>, AiError>;
}

/// Simulation mode for deterministic testing without external neural runtimes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimulationMode {
    /// Passes input tile through unmodified (`x -> x`).
    Identity,
    /// Gentle 3x3 box smoothing.
    BoxFilter3x3,
}

/// Deterministic mock neural denoiser used for unit and contract verification.
#[derive(Debug, Clone, Copy)]
pub struct SimulatedTileDenoiser {
    mode: SimulationMode,
}

impl SimulatedTileDenoiser {
    /// Constructs a simulated denoiser with identity passthrough.
    #[must_use]
    pub const fn identity() -> Self {
        Self {
            mode: SimulationMode::Identity,
        }
    }

    /// Constructs a simulated denoiser with the specified mode.
    #[must_use]
    pub const fn new(mode: SimulationMode) -> Self {
        Self { mode }
    }
}

impl NeuralTileDenoiser for SimulatedTileDenoiser {
    fn denoise_tile(
        &self,
        tile: &[f32],
        width: usize,
        height: usize,
        _noise: &NoiseProfile,
    ) -> Result<Vec<f32>, AiError> {
        let expected = width.saturating_mul(height);
        if tile.len() != expected {
            return Err(AiError::InvalidInput(format!(
                "tile length mismatch: expected {expected}, got {}",
                tile.len()
            )));
        }

        match self.mode {
            SimulationMode::Identity => Ok(tile.to_vec()),
            SimulationMode::BoxFilter3x3 => {
                let mut out = vec![0.0_f32; expected];
                for y in 0..height {
                    for x in 0..width {
                        let mut sum = 0.0_f32;
                        let mut count = 0.0_f32;
                        for dy in -1..=1 {
                            for dx in -1..=1 {
                                let nx = (x as isize + dx).clamp(0, width as isize - 1) as usize;
                                let ny = (y as isize + dy).clamp(0, height as isize - 1) as usize;
                                sum += tile[ny * width + nx];
                                count += 1.0;
                            }
                        }
                        out[y * width + x] = sum / count;
                    }
                }
                Ok(out)
            }
        }
    }
}
