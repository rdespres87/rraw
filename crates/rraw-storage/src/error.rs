#![forbid(unsafe_code)]

use thiserror::Error;

/// Strongly-typed storage, database, and sidecar errors.
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("XMP serialization or parsing error: {0}")]
    Xmp(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
