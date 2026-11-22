#![forbid(unsafe_code)]

use thiserror::Error;

/// Strongly-typed export, gain map, and file encoding errors.
#[derive(Debug, Error)]
pub enum ExportError {
    #[error("Image dimension or buffer length mismatch: expected {expected}, got {actual}")]
    BufferLengthMismatch { expected: usize, actual: usize },

    #[error("Binary tmap box decoding failed: {0}")]
    TmapDecodeFailed(String),

    #[error("XMP metadata decoding failed: {0}")]
    XmpDecodeFailed(String),

    #[error("Image encoding error: {0}")]
    EncodingError(String),

    #[error("Unsupported export format: {0}")]
    UnsupportedFormat(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
