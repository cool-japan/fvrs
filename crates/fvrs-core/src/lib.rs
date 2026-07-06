//! Core file system operations and async runtime for FVRS
//!
//! This crate provides the core functionality for file system operations,
//! configuration management, and the async runtime used by the GUI.
//!
//! The crate is organized into focused modules:
//!
//! - [`fs`] — [`FileSystem`] and [`FileEntry`]: listing, create/remove, recursive copy, move
//! - [`permissions`] — [`FilePermissions`] with per-platform attribute handling
//! - [`search`] — regex name/content search via [`SearchOptions`]
//! - [`hash`] — file/directory hashing over six algorithms through one generic helper
//! - [`compare`] — binary and text file/directory comparison
//! - [`monitor`] — watcher lifecycle, [`FsEvent`] delivery, filters, settings, history
//! - [`config`] — application configuration
//! - [`plugin`] — plugin system trait
//! - [`error`] — [`FsError`] / [`FsResult`]
//!
//! The historical `fvrs_core::core::*` paths remain available through the
//! [`core`] facade module, and the most-used types are re-exported at the
//! crate root.

use std::path::PathBuf;
use thiserror::Error;

pub mod compare;
pub mod config;
pub mod error;
pub mod fs;
pub mod hash;
pub mod monitor;
pub mod permissions;
pub mod plugin;
pub mod search;

pub use compare::{ComparisonResult, ComparisonType, Difference};
pub use error::{FsError, FsResult};
pub use fs::{FileEntry, FileSystem};
pub use hash::{HashAlgorithm, HashResult};
pub use monitor::{FsEvent, FsEventType, MonitoringFilter, MonitoringHistory, MonitoringSettings};
pub use permissions::FilePermissions;
pub use search::SearchOptions;

/// Backwards-compatible facade preserving the historical `fvrs_core::core::*` paths
pub mod core {
    pub use crate::compare::{ComparisonResult, ComparisonType, Difference};
    pub use crate::error::{FsError, FsResult};
    pub use crate::fs::{FileEntry, FileSystem};
    pub use crate::hash::{HashAlgorithm, HashResult};
    pub use crate::monitor::{
        FsEvent, FsEventType, MonitoringFilter, MonitoringHistory, MonitoringSettings,
    };
    pub use crate::permissions::FilePermissions;
    pub use crate::search::SearchOptions;
}

/// Core error type for FVRS
#[derive(Error, Debug)]
pub enum CoreError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Operation not supported: {0}")]
    NotSupported(String),
}

/// Result type for core operations
pub type Result<T> = std::result::Result<T, CoreError>;

/// Core configuration
#[derive(Debug, Clone)]
pub struct Config {
    pub default_path: PathBuf,
    pub show_hidden: bool,
    pub sort_by: SortBy,
}

/// File sorting options
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortBy {
    Name,
    Size,
    Modified,
    Created,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_path: PathBuf::from("."),
            show_hidden: false,
            sort_by: SortBy::Name,
        }
    }
}

/// Initialize the core runtime
pub async fn init() -> Result<()> {
    // Initialize Tokio runtime
    Ok(())
}

/// Shutdown the core runtime
pub async fn shutdown() -> Result<()> {
    // Cleanup resources
    Ok(())
}
