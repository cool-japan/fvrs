//! Error types shared by all filesystem operations

use thiserror::Error;

/// Error type for filesystem operations
#[derive(Error, Debug)]
pub enum FsError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid path: {0}")]
    InvalidPath(String),
    #[error("File system event error: {0}")]
    FsEvent(String),
    #[error("Operation not supported: {0}")]
    NotSupported(String),
    #[error("Invalid regex pattern: {0}")]
    InvalidRegex(String),
    #[error("Search error: {0}")]
    Search(String),
    #[error("Permission error: {0}")]
    Permission(String),
    #[error("Hash error: {0}")]
    Hash(String),
    #[error("Comparison error: {0}")]
    Comparison(String),
    #[error("Monitoring error: {0}")]
    Monitoring(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

impl From<notify::Error> for FsError {
    fn from(e: notify::Error) -> Self {
        FsError::Monitoring(e.to_string())
    }
}

/// Result type for filesystem operations
pub type FsResult<T> = std::result::Result<T, FsError>;
