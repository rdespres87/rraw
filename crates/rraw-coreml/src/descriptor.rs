#![deny(unsafe_code)]

//! Model descriptors, configuration, and typed CoreML error definitions.

use crate::hardware::CoreMlComputeUnits;
use rraw_core::AiError;
use thiserror::Error;

/// Errors arising during CoreML compilation, initialization, or tensor prediction.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CoreMlError {
    #[error("CoreML model bundle not found: {0}")]
    ModelNotFound(String),

    #[error("Failed to compile CoreML model at {0}: {1}")]
    CompilationFailed(String, String),

    #[error("Inference prediction failed: {0}")]
    PredictionFailed(String),

    #[error("Invalid tensor shape: expected {expected:?}, got {actual:?}")]
    InvalidTensorShape {
        expected: Vec<usize>,
        actual: Vec<usize>,
    },

    #[error("Unsupported platform for Apple Neural Engine hardware: {0}")]
    PlatformUnsupported(String),

    #[error("Invalid model input: {0}")]
    InvalidInput(String),
}

impl From<CoreMlError> for AiError {
    fn from(err: CoreMlError) -> Self {
        match err {
            CoreMlError::InvalidTensorShape { expected, actual } => Self::InvalidInput(format!(
                "tensor shape mismatch: expected {expected:?}, got {actual:?}"
            )),
            CoreMlError::InvalidInput(msg) => Self::InvalidInput(msg),
            CoreMlError::ModelNotFound(path) => Self::Backend(format!("model not found: {path}")),
            CoreMlError::CompilationFailed(path, reason) => {
                Self::Backend(format!("failed to compile model at {path}: {reason}"))
            }
            CoreMlError::PredictionFailed(reason) => Self::Backend(reason),
            CoreMlError::PlatformUnsupported(reason) => Self::Backend(reason),
        }
    }
}

/// Metadata and tensor dimensionality specification for a CoreML model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreMlModelDescriptor {
    model_name: String,
    compute_units: CoreMlComputeUnits,
    input_shape: Vec<usize>,
    output_shape: Vec<usize>,
}

impl CoreMlModelDescriptor {
    /// Constructs a new CoreML model descriptor.
    #[must_use]
    pub fn new(
        model_name: impl Into<String>,
        compute_units: CoreMlComputeUnits,
        input_shape: Vec<usize>,
        output_shape: Vec<usize>,
    ) -> Self {
        Self {
            model_name: model_name.into(),
            compute_units,
            input_shape,
            output_shape,
        }
    }

    /// Returns the descriptive name of the model.
    #[must_use]
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// Returns the assigned execution compute units.
    #[must_use]
    pub fn compute_units(&self) -> CoreMlComputeUnits {
        self.compute_units
    }

    /// Returns the expected input tensor shape.
    #[must_use]
    pub fn input_shape(&self) -> &[usize] {
        &self.input_shape
    }

    /// Returns the expected output tensor shape.
    #[must_use]
    pub fn output_shape(&self) -> &[usize] {
        &self.output_shape
    }

    /// Validates that descriptor properties satisfy non-empty and non-zero constraints.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        !self.model_name.trim().is_empty()
            && !self.input_shape.is_empty()
            && !self.output_shape.is_empty()
            && self.input_shape.iter().all(|&dim| dim > 0)
            && self.output_shape.iter().all(|&dim| dim > 0)
    }
}
