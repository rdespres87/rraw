use thiserror::Error;

/// Strongly-typed GPU pipeline and context errors.
#[derive(Debug, Error)]
pub enum GpuError {
    #[error("No compatible GPU adapter found for headless execution")]
    AdapterNotFound,

    #[error("Failed to request GPU device: {0}")]
    DeviceRequestFailed(String),

    #[error("Failed to map GPU staging buffer: {0}")]
    BufferMapFailed(String),

    #[error("Shader compilation or creation failed: {0}")]
    ShaderError(String),

    #[error("Invalid image dimensions: {width}x{height}")]
    InvalidDimensions { width: u32, height: u32 },

    #[error("Buffer length mismatch: expected {expected}, got {actual}")]
    BufferLengthMismatch { expected: usize, actual: usize },
}
