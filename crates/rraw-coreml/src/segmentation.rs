#![deny(unsafe_code)]

//! Hardware Apple Neural Engine (ANE) 4-class semantic segmentation bridge.

use crate::engine::CoreMlEngine;
use rraw_ai::{SemanticMaskSet, SemanticSegmenter};
use rraw_core::AiError;

/// CoreML / Apple Neural Engine 4-class semantic segmentation inference engine.
///
/// Infeers probabilistic spatial maps for `Subject`, `Sky`, `Background`, and `Portrait`
/// directly from linear RGB image frames. Enforces exact partition-of-unity
/// (`subject + background == 1.0`) across all pixels.
#[derive(Debug, Clone, Default)]
pub struct AneSemanticSegmenter {
    engine: CoreMlEngine,
}

impl AneSemanticSegmenter {
    /// Creates a new ANE semantic segmenter with default hardware tuning.
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

impl SemanticSegmenter for AneSemanticSegmenter {
    fn segment_rgb(
        &self,
        rgb: &[[f32; 3]],
        width: usize,
        height: usize,
    ) -> Result<SemanticMaskSet, AiError> {
        if width == 0 || height == 0 {
            return Err(AiError::InvalidDimensions { width, height });
        }
        let total = width.saturating_mul(height);
        if rgb.len() != total {
            return Err(AiError::InvalidInput(format!(
                "RGB buffer length mismatch: expected {total}, got {}",
                rgb.len()
            )));
        }

        self.engine.run_segmentation(rgb, width, height)
    }
}
