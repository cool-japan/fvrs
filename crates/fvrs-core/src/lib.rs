//! Core file system operations and async runtime for FVRS
//!
//! This crate provides the core functionality for file system operations,
//! configuration management, and the async runtime used by the GUI.

use std::path::PathBuf;
use thiserror::Error;
use tokio::fs;
use chrono::{DateTime, Local};
use serde::{Serialize, Deserialize};
use walkdir::WalkDir;
use notify::{Watcher, RecursiveMode, Event};
use std::sync::mpsc;
use regex::Regex;
use std::collections::VecDeque;

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

/// Core module providing FVRS functionality
pub mod core {
    use super::*;

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
    }

    /// Result type for filesystem operations
    pub type FsResult<T> = Result<T, FsError>;

    /// File entry information
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct FileEntry {
        /// File name
        pub name: String,
        /// Full path
        pub path: PathBuf,
        /// File size in bytes
        pub size: u64,
        /// Is directory
        pub is_dir: bool,
        /// Creation time
        pub created: DateTime<Local>,
        /// Last modification time
        pub modified: DateTime<Local>,
        /// File extension
        pub extension: Option<String>,
    }

    /// Search options for file search
    #[derive(Debug, Clone)]
    pub struct SearchOptions {
        /// Search pattern (regex)
        pub pattern: String,
        /// Search in file contents
        pub search_contents: bool,
        /// Case sensitive search
        pub case_sensitive: bool,
        /// Maximum depth for recursive search
        pub max_depth: Option<usize>,
        /// File extensions to include (None means all)
        pub extensions: Option<Vec<String>>,
        /// Maximum number of results
        pub max_results: Option<usize>,
    }

    impl Default for SearchOptions {
        fn default() -> Self {
            Self {
                pattern: String::new(),
                search_contents: false,
                case_sensitive: true,
                max_depth: None,
                extensions: None,
                max_results: None,
            }
        }
    }

    /// Structure providing basic filesystem operations
    pub struct FileSystem {
        /// Current working directory
        current_dir: PathBuf,
        /// File system event sender
        event_sender: Option<mpsc::Sender<FsEvent>>,
    }

    /// File system event
    #[derive(Debug, Clone)]
    pub enum FsEvent {
        /// File created
        Created(PathBuf),
        /// File modified
        Modified(PathBuf),
        /// File removed
        Removed(PathBuf),
        /// File renamed
        Renamed(PathBuf, PathBuf),
    }

    impl FileSystem {
        /// Create a new FileSystem instance
        pub fn new() -> Self {
            Self {
                current_dir: PathBuf::from("."),
                event_sender: None,
            }
        }

        /// Set the current working directory
        pub async fn set_current_dir(&mut self, path: PathBuf) -> FsResult<()> {
            if !fs::try_exists(&path).await? {
                return Err(FsError::InvalidPath(format!("Directory does not exist: {:?}", path)));
            }
            self.current_dir = path;
            Ok(())
        }

        /// Get the current working directory
        pub fn current_dir(&self) -> &PathBuf {
            &self.current_dir
        }

        /// List files in the specified path
        pub async fn list_files(&self, path: Option<PathBuf>) -> FsResult<Vec<FileEntry>> {
            let target_path = path.unwrap_or_else(|| self.current_dir.clone());
            let mut entries = Vec::new();

            for entry in WalkDir::new(&target_path)
                .min_depth(1)
                .max_depth(1)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path().to_path_buf();
                let metadata = fs::metadata(&path).await?;
                
                entries.push(FileEntry {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    path,
                    size: metadata.len(),
                    is_dir: metadata.is_dir(),
                    created: DateTime::from(metadata.created()?),
                    modified: DateTime::from(metadata.modified()?),
                    extension: entry.path()
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(String::from),
                });
            }

            Ok(entries)
        }

        /// Check if a file exists
        pub async fn exists(&self, path: &PathBuf) -> FsResult<bool> {
            Ok(fs::try_exists(path).await?)
        }

        /// Create a new directory
        pub async fn create_dir(&self, path: &PathBuf) -> FsResult<()> {
            fs::create_dir_all(path).await?;
            Ok(())
        }

        /// Remove a file or directory
        pub async fn remove(&self, path: &PathBuf) -> FsResult<()> {
            if path.is_dir() {
                fs::remove_dir_all(path).await?;
            } else {
                fs::remove_file(path).await?;
            }
            Ok(())
        }

        /// Copy a file or directory
        pub async fn copy(&self, src: &PathBuf, dest: &PathBuf) -> FsResult<()> {
            if src.is_dir() {
                return Err(FsError::NotSupported("Directory copy not implemented".into()));
            }
            fs::copy(src, dest).await?;
            Ok(())
        }

        /// Move a file or directory
        pub async fn move_file(&self, src: &PathBuf, dest: &PathBuf) -> FsResult<()> {
            fs::rename(src, dest).await?;
            Ok(())
        }

        /// Start watching for file system events
        pub async fn watch_directory(&mut self, path: &PathBuf) -> FsResult<()> {
            let (tx, rx) = mpsc::channel();
            self.event_sender = Some(tx);

            let mut watcher = notify::recommended_watcher(move |res: Result<Event, _>| {
                if let Ok(event) = res {
                    match event.kind {
                        notify::EventKind::Create(_) => {
                            for path in event.paths {
                                let _ = tx.send(FsEvent::Created(path));
                            }
                        }
                        notify::EventKind::Modify(_) => {
                            for path in event.paths {
                                let _ = tx.send(FsEvent::Modified(path));
                            }
                        }
                        notify::EventKind::Remove(_) => {
                            for path in event.paths {
                                let _ = tx.send(FsEvent::Removed(path));
                            }
                        }
                        notify::EventKind::Rename(_, _) => {
                            if event.paths.len() >= 2 {
                                let _ = tx.send(FsEvent::Renamed(
                                    event.paths[0].clone(),
                                    event.paths[1].clone(),
                                ));
                            }
                        }
                        _ => {}
                    }
                }
            })?;

            watcher.watch(path, RecursiveMode::Recursive)?;
            Ok(())
        }

        /// Get the next file system event
        pub fn next_event(&self) -> Option<FsEvent> {
            self.event_sender.as_ref()
                .and_then(|tx| tx.recv().ok())
        }

        /// Search for files matching the given pattern
        pub async fn search_files(&self, options: SearchOptions) -> FsResult<Vec<FileEntry>> {
            let pattern = if options.case_sensitive {
                Regex::new(&options.pattern)
            } else {
                Regex::new(&format!("(?i){}", options.pattern))
            }.map_err(|e| FsError::InvalidRegex(e.to_string()))?;

            let mut results = Vec::new();
            let mut walker = WalkDir::new(&self.current_dir)
                .min_depth(1);

            if let Some(max_depth) = options.max_depth {
                walker = walker.max_depth(max_depth);
            }

            for entry in walker.into_iter().filter_map(|e| e.ok()) {
                if let Some(max_results) = options.max_results {
                    if results.len() >= max_results {
                        break;
                    }
                }

                let path = entry.path();
                let name = entry.file_name().to_string_lossy();

                // Check file extension if specified
                if let Some(ref extensions) = options.extensions {
                    if let Some(ext) = path.extension() {
                        if !extensions.contains(&ext.to_string_lossy().to_lowercase()) {
                            continue;
                        }
                    }
                }

                // Check filename match
                if pattern.is_match(&name) {
                    if let Ok(metadata) = fs::metadata(path).await {
                        results.push(FileEntry {
                            name: name.into_owned(),
                            path: path.to_path_buf(),
                            size: metadata.len(),
                            is_dir: metadata.is_dir(),
                            created: DateTime::from(metadata.created()?),
                            modified: DateTime::from(metadata.modified()?),
                            extension: path.extension()
                                .and_then(|e| e.to_str())
                                .map(String::from),
                        });
                    }
                    continue;
                }

                // Check file contents if requested
                if options.search_contents && !path.is_dir() {
                    if let Ok(contents) = fs::read_to_string(path).await {
                        if pattern.is_match(&contents) {
                            if let Ok(metadata) = fs::metadata(path).await {
                                results.push(FileEntry {
                                    name: name.into_owned(),
                                    path: path.to_path_buf(),
                                    size: metadata.len(),
                                    is_dir: metadata.is_dir(),
                                    created: DateTime::from(metadata.created()?),
                                    modified: DateTime::from(metadata.modified()?),
                                    extension: path.extension()
                                        .and_then(|e| e.to_str())
                                        .map(String::from),
                                });
                            }
                        }
                    }
                }
            }

            Ok(results)
        }

        /// Find files by pattern (simplified search)
        pub async fn find_files(&self, pattern: &str) -> FsResult<Vec<FileEntry>> {
            let options = SearchOptions {
                pattern: pattern.to_string(),
                search_contents: false,
                case_sensitive: false,
                max_depth: None,
                extensions: None,
                max_results: None,
            };
            self.search_files(options).await
        }

        /// Find files by extension
        pub async fn find_files_by_extension(&self, extension: &str) -> FsResult<Vec<FileEntry>> {
            let options = SearchOptions {
                pattern: format!("\\.{}$", extension),
                search_contents: false,
                case_sensitive: false,
                max_depth: None,
                extensions: Some(vec![extension.to_string()]),
                max_results: None,
            };
            self.search_files(options).await
        }
    }
}

/// Module providing plugin system functionality
pub mod plugin {
    use super::*;

    /// Basic trait for plugins
    pub trait Plugin {
        /// Get plugin name
        fn name(&self) -> &str;
        
        /// Get plugin version
        fn version(&self) -> &str;
        
        /// Initialize the plugin
        fn initialize(&mut self) -> anyhow::Result<()>;
    }
}

/// Module for managing application configuration
pub mod config {
    use super::*;
    use serde::{Deserialize, Serialize};

    /// Application configuration
    #[derive(Debug, Serialize, Deserialize)]
    pub struct Config {
        /// Default working directory
        pub default_working_dir: PathBuf,
        /// Plugin directory
        pub plugin_dir: PathBuf,
    }

    impl Default for Config {
        fn default() -> Self {
            Self {
                default_working_dir: PathBuf::from("."),
                plugin_dir: PathBuf::from("./plugins"),
            }
        }
    }
} 