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
use std::ffi::OsStr;
use notify::event::ModifyKind;
use md5;
use hex;
use std::cmp::min;
use std::time::SystemTime;
use std::collections::HashMap;
use std::collections::HashSet;
use glob;
use serde_json;
use std::fs::File;
use std::io::BufWriter;
use notify::EventKind;
use std::io::BufReader as StdBufReader;
use tokio::io::BufReader as TokioBufReader;
use windows::Win32::Storage::FileSystem::{GetFileAttributesW, FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_SYSTEM, FILE_ATTRIBUTE_ARCHIVE};
use std::os::windows::ffi::OsStrExt;
use tokio::io::{AsyncReadExt, AsyncBufReadExt};
use sha1::Digest;
use sha2::{Sha256, Sha512};
use blake3;
use ripemd::Ripemd160;
use std::io::Read;
use windows::core::PCWSTR;
// use base64;
// use aes;
// use cipher;

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

    /// File permissions structure
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct FilePermissions {
        /// Read permission
        pub readable: bool,
        /// Write permission
        pub writable: bool,
        /// Execute permission
        pub executable: bool,
        /// Hidden attribute (Windows)
        #[cfg(windows)]
        pub hidden: bool,
        /// System attribute (Windows)
        #[cfg(windows)]
        pub system: bool,
        /// Archive attribute (Windows)
        #[cfg(windows)]
        pub archive: bool,
        /// Unix mode (Unix-like systems)
        #[cfg(unix)]
        pub mode: u32,
    }

    impl FilePermissions {
        /// Create new permissions with default values
        pub fn new() -> Self {
            Self {
                readable: true,
                writable: true,
                executable: false,
                #[cfg(windows)]
                hidden: false,
                #[cfg(windows)]
                system: false,
                #[cfg(windows)]
                archive: false,
                #[cfg(unix)]
                mode: 0o644,
            }
        }

        /// Create permissions from file metadata
        pub async fn from_path(path: &PathBuf) -> FsResult<Self> {
            let metadata = fs::metadata(path).await?;
            let _permissions = metadata.permissions();

            #[cfg(unix)]
            {
                let mode = _permissions.mode();
                Ok(Self {
                    readable: mode & 0o444 != 0,
                    writable: mode & 0o222 != 0,
                    executable: mode & 0o111 != 0,
                    mode,
                })
            }

            #[cfg(windows)]
            {
                let wide: Vec<u16> = OsStr::new(path).encode_wide().chain(Some(0)).collect();
                let attrs = unsafe { GetFileAttributesW(PCWSTR(wide.as_ptr())) };
                if attrs == u32::MAX {
                    return Err(FsError::Permission("Failed to get file attributes".to_string()));
                }
                Ok(Self {
                    readable: true,
                    writable: attrs & (FILE_ATTRIBUTE_READONLY.0 as u32) == 0,
                    executable: false,
                    hidden: attrs & (FILE_ATTRIBUTE_HIDDEN.0 as u32) != 0,
                    system: attrs & (FILE_ATTRIBUTE_SYSTEM.0 as u32) != 0,
                    archive: attrs & (FILE_ATTRIBUTE_ARCHIVE.0 as u32) != 0,
                })
            }
        }

        /// Apply permissions to a file
        pub async fn apply(&self, path: &PathBuf) -> FsResult<()> {
            let permissions = fs::metadata(path).await?.permissions();

            #[cfg(unix)]
            {
                let mut mode = 0;
                if self.readable {
                    mode |= 0o444;
                }
                if self.writable {
                    mode |= 0o222;
                }
                if self.executable {
                    mode |= 0o111;
                }
                permissions.set_mode(mode);
            }

            #[cfg(windows)]
            {
                // permissions.file_attributes() や permissions.set_file_attributes(attrs) の行をコメントアウト
            }

            fs::set_permissions(path, permissions).await?;
            Ok(())
        }
    }

    impl FileEntry {
        /// Get file permissions
        pub async fn get_permissions(&self) -> FsResult<FilePermissions> {
            let metadata = fs::metadata(&self.path).await?;
            let _permissions = metadata.permissions();
            FilePermissions::from_path(&self.path).await
        }

        /// Set file permissions
        pub async fn set_permissions(&self, permissions: FilePermissions) -> FsResult<()> {
            permissions.apply(&self.path).await
        }
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

    /// Hash algorithm type
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum HashAlgorithm {
        /// MD5 hash
        MD5,
        /// SHA-1 hash
        SHA1,
        /// SHA-256 hash
        SHA256,
        /// SHA-512 hash
        SHA512,
        /// BLAKE3 hash
        BLAKE3,
        /// RIPEMD-160 hash
        RIPEMD160,
    }

    /// Hash result structure
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct HashResult {
        /// Hash algorithm used
        pub algorithm: HashAlgorithm,
        /// Hash value in hexadecimal
        pub hash: String,
        /// File size in bytes
        pub size: u64,
        /// Time taken to compute hash in milliseconds
        pub time_ms: u64,
    }

    /// Comparison type
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ComparisonType {
        /// Binary comparison
        Binary,
        /// Text comparison
        Text,
        /// Text comparison ignoring whitespace
        TextIgnoreWhitespace,
        /// Text comparison ignoring case
        TextIgnoreCase,
    }

    /// Difference type
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Difference {
        /// Different bytes at offset
        BinaryDiff {
            offset: u64,
            left: u8,
            right: u8,
        },
        /// Different lines
        TextDiff {
            line: usize,
            left: String,
            right: String,
        },
        /// File size difference
        SizeDiff {
            left_size: u64,
            right_size: u64,
        },
        /// File type difference
        TypeDiff {
            left_type: String,
            right_type: String,
        },
    }

    /// Comparison result
    #[derive(Debug, Clone)]
    pub struct ComparisonResult {
        /// Whether files are identical
        pub identical: bool,
        /// List of differences
        pub differences: Vec<Difference>,
        /// Total number of differences
        pub total_differences: usize,
        /// Comparison time in milliseconds
        pub time_ms: u64,
    }

    /// Event type for file system monitoring
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
    pub enum FsEventType {
        /// File or directory created
        Create,
        /// File or directory modified
        Modify,
        /// File or directory removed
        Remove,
        /// File or directory renamed
        Rename,
        /// File or directory accessed
        Access,
        /// File or directory metadata changed
        Metadata,
    }

    /// File system event
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct FsEvent {
        /// Event type
        pub event_type: FsEventType,
        /// Path to the file or directory
        pub path: PathBuf,
        /// Timestamp of the event
        pub timestamp: DateTime<Local>,
        /// Additional metadata
        pub metadata: HashMap<String, String>,
    }

    /// Monitoring filter
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct MonitoringFilter {
        /// File patterns to include (glob)
        pub include_patterns: Vec<String>,
        /// File patterns to exclude (glob)
        pub exclude_patterns: Vec<String>,
        /// Event types to monitor
        pub event_types: HashSet<FsEventType>,
        /// Minimum file size to monitor
        pub min_size: Option<u64>,
        /// Maximum file size to monitor
        pub max_size: Option<u64>,
        /// File extensions to monitor
        pub extensions: HashSet<String>,
    }

    /// Monitoring settings
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct MonitoringSettings {
        /// Path to monitor
        pub path: PathBuf,
        /// Whether to monitor recursively
        pub recursive: bool,
        /// Filter settings
        pub filter: MonitoringFilter,
        /// Maximum number of events to keep in history
        pub max_history: usize,
        /// Debounce time in milliseconds
        pub debounce_ms: u64,
    }

    /// Monitoring history
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct MonitoringHistory {
        /// List of events
        pub events: VecDeque<FsEvent>,
        /// Maximum number of events to keep
        pub max_events: usize,
    }

    impl MonitoringHistory {
        /// Create new monitoring history
        pub fn new(max_events: usize) -> Self {
            Self {
                events: VecDeque::with_capacity(max_events),
                max_events,
            }
        }

        /// Add event to history
        pub fn add_event(&mut self, event: FsEvent) {
            if self.events.len() >= self.max_events {
                self.events.pop_front();
            }
            self.events.push_back(event);
        }

        /// Get events within time range
        pub fn get_events_in_range(&self, start: DateTime<Local>, end: DateTime<Local>) -> Vec<&FsEvent> {
            self.events
                .iter()
                .filter(|event| event.timestamp >= start && event.timestamp <= end)
                .collect()
        }

        /// Get events by type
        pub fn get_events_by_type(&self, event_type: &FsEventType) -> Vec<&FsEvent> {
            self.events
                .iter()
                .filter(|event| event.event_type == *event_type)
                .collect()
        }

        /// Save history to file
        pub fn save_to_file(&self, path: &PathBuf) -> FsResult<()> {
            let file = File::create(path)?;
            let writer = BufWriter::new(file);
            serde_json::to_writer_pretty(writer, &self.events)
                .map_err(|e| FsError::Serialization(e.to_string()))?;
            Ok(())
        }

        /// Load history from file
        pub fn load_from_file(path: &PathBuf, max_events: usize) -> FsResult<Self> {
            let file = File::open(path)?;
            let reader = StdBufReader::new(file);
            let events: Vec<FsEvent> = serde_json::from_reader(reader)
                .map_err(|e| FsError::Serialization(e.to_string()))?;
            
            let mut history = Self::new(max_events);
            for event in events {
                history.add_event(event);
            }
            Ok(history)
        }
    }

    impl MonitoringFilter {
        /// Create new monitoring filter
        pub fn new() -> Self {
            Self {
                include_patterns: Vec::new(),
                exclude_patterns: Vec::new(),
                event_types: HashSet::new(),
                min_size: None,
                max_size: None,
                extensions: HashSet::new(),
            }
        }

        /// Check if path matches filter
        pub fn matches(&self, path: &PathBuf) -> bool {
            // Check include patterns
            if !self.include_patterns.is_empty() {
                let matches_include = self.include_patterns.iter().any(|pattern| {
                    glob::Pattern::new(pattern)
                        .map(|p| p.matches(path.to_str().unwrap_or("")))
                        .unwrap_or(false)
                });
                if !matches_include {
                    return false;
                }
            }

            // Check exclude patterns
            if self.exclude_patterns.iter().any(|pattern| {
                glob::Pattern::new(pattern)
                    .map(|p| p.matches(path.to_str().unwrap_or("")))
                    .unwrap_or(false)
            }) {
                return false;
            }

            // Check extensions
            if !self.extensions.is_empty() {
                if let Some(ext) = path.extension() {
                    if let Some(ext_str) = ext.to_str() {
                        if !self.extensions.contains(ext_str) {
                            return false;
                        }
                    }
                }
            }

            true
        }
    }

    /// Structure providing basic filesystem operations
    pub struct FileSystem {
        /// Current working directory
        current_dir: PathBuf,
        /// File system event receiver
        event_receiver: Option<mpsc::Receiver<FsEvent>>,
    }

    impl FileSystem {
        /// Create a new FileSystem instance
        pub fn new() -> Self {
            Self {
                current_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
                event_receiver: None,
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
            self.event_receiver = Some(rx);
            let mut watcher = notify::recommended_watcher(move |res: std::result::Result<Event, notify::Error>| {
                match res {
                    Ok(event) => {
                        if let Some(_path) = event.paths.first() {
                            let _ = tx.send(FsEvent::from(event));
                        }
                    }
                    Err(_e) => {
                        // Handle error
                    }
                }
            })?;
            watcher.watch(path, RecursiveMode::Recursive)?;
            Ok(())
        }

        /// Get the next file system event
        pub fn next_event(&self) -> Option<FsEvent> {
            self.event_receiver.as_ref()
                .and_then(|rx| rx.try_recv().ok())
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

        /// Calculate hash of a file
        pub async fn calculate_hash(&self, path: &PathBuf, algorithm: HashAlgorithm) -> FsResult<HashResult> {
            let start_time = SystemTime::now();
            let mut file = File::open(path)?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)?;
            let size = buffer.len() as u64;

            let hash = match algorithm {
                HashAlgorithm::MD5 => {
                    let digest = md5::compute(&buffer);
                    hex::encode(digest.0)
                }
                HashAlgorithm::SHA1 => {
                    let mut hasher = sha1::Sha1::new();
                    hasher.update(&buffer);
                    hex::encode(hasher.finalize())
                }
                HashAlgorithm::SHA256 => {
                    let mut hasher = Sha256::new();
                    hasher.update(&buffer);
                    hex::encode(hasher.finalize())
                }
                HashAlgorithm::SHA512 => {
                    let mut hasher = Sha512::new();
                    hasher.update(&buffer);
                    hex::encode(hasher.finalize())
                }
                HashAlgorithm::BLAKE3 => {
                    let mut hasher = blake3::Hasher::new();
                    hasher.update(&buffer);
                    hex::encode(hasher.finalize().as_bytes())
                }
                HashAlgorithm::RIPEMD160 => {
                    let mut hasher = Ripemd160::new();
                    hasher.update(&buffer);
                    hex::encode(hasher.finalize())
                }
            };

            let end_time = SystemTime::now();
            let duration = end_time.duration_since(start_time).unwrap();
            let time_ms = duration.as_millis() as u64;

            Ok(HashResult {
                algorithm,
                hash,
                size,
                time_ms,
            })
        }

        /// Verify file hash
        pub async fn verify_hash(&self, path: &PathBuf, expected_hash: &str, algorithm: HashAlgorithm) -> FsResult<bool> {
            let result = self.calculate_hash(path, algorithm).await?;
            Ok(result.hash == expected_hash)
        }

        /// Calculate hash of a directory (recursive)
        pub async fn calculate_directory_hash(&self, path: &PathBuf, algorithm: HashAlgorithm) -> FsResult<HashResult> {
            let start_time = std::time::Instant::now();
            let hash = match algorithm {
                HashAlgorithm::MD5 => {
                    let mut buffer = Vec::new();
                    for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
                        let path = entry.path();
                        if path.is_file() {
                            let mut file = fs::File::open(path).await?;
                            let mut file_buffer = Vec::new();
                            file.read_to_end(&mut file_buffer).await?;
                            buffer.extend(file_buffer);
                        }
                    }
                    let digest = md5::compute(&buffer);
                    hex::encode(digest.0)
                }
                HashAlgorithm::SHA1 => {
                    let mut hasher = sha1::Sha1::new();
                    for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
                        let path = entry.path();
                        if path.is_file() {
                            let mut file = fs::File::open(path).await?;
                            let mut buffer = Vec::new();
                            file.read_to_end(&mut buffer).await?;
                            hasher.update(&buffer);
                        }
                    }
                    hex::encode(hasher.finalize())
                }
                HashAlgorithm::SHA256 => {
                    let mut hasher = sha2::Sha256::new();
                    for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
                        let path = entry.path();
                        if path.is_file() {
                            let mut file = fs::File::open(path).await?;
                            let mut buffer = Vec::new();
                            file.read_to_end(&mut buffer).await?;
                            hasher.update(&buffer);
                        }
                    }
                    hex::encode(hasher.finalize())
                }
                HashAlgorithm::SHA512 => {
                    let mut hasher = sha2::Sha512::new();
                    for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
                        let path = entry.path();
                        if path.is_file() {
                            let mut file = fs::File::open(path).await?;
                            let mut buffer = Vec::new();
                            file.read_to_end(&mut buffer).await?;
                            hasher.update(&buffer);
                        }
                    }
                    hex::encode(hasher.finalize())
                }
                HashAlgorithm::BLAKE3 => {
                    let mut hasher = blake3::Hasher::new();
                    for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
                        let path = entry.path();
                        if path.is_file() {
                            let mut file = fs::File::open(path).await?;
                            let mut buffer = Vec::new();
                            file.read_to_end(&mut buffer).await?;
                            hasher.update(&buffer);
                        }
                    }
                    hex::encode(hasher.finalize().as_bytes())
                }
                HashAlgorithm::RIPEMD160 => {
                    let mut hasher = ripemd::Ripemd160::new();
                    for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
                        let path = entry.path();
                        if path.is_file() {
                            let mut file = fs::File::open(path).await?;
                            let mut buffer = Vec::new();
                            file.read_to_end(&mut buffer).await?;
                            hasher.update(&buffer);
                        }
                    }
                    hex::encode(hasher.finalize())
                }
            };
            let time_ms = start_time.elapsed().as_millis() as u64;
            let size = WalkDir::new(path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
                .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
                .sum();
            Ok(HashResult {
                algorithm,
                hash,
                size,
                time_ms,
            })
        }

        /// Compare two files
        pub async fn compare_files(&self, left: &PathBuf, right: &PathBuf, comparison_type: ComparisonType) -> FsResult<ComparisonResult> {
            let start_time = std::time::Instant::now();
            let mut differences = Vec::new();

            // Check file sizes
            let left_metadata = fs::metadata(left).await?;
            let right_metadata = fs::metadata(right).await?;

            if left_metadata.len() != right_metadata.len() {
                differences.push(Difference::SizeDiff {
                    left_size: left_metadata.len(),
                    right_size: right_metadata.len(),
                });
            }

            match comparison_type {
                ComparisonType::Binary => {
                    self.compare_binary(left, right, &mut differences).await?;
                }
                ComparisonType::Text | ComparisonType::TextIgnoreWhitespace | ComparisonType::TextIgnoreCase => {
                    self.compare_text(left, right, comparison_type, &mut differences).await?;
                }
            }

            let time_ms = start_time.elapsed().as_millis() as u64;

            let diff_clone = differences.clone();
            Ok(ComparisonResult {
                identical: diff_clone.is_empty(),
                differences,
                total_differences: diff_clone.len(),
                time_ms,
            })
        }

        async fn compare_binary(&self, left: &PathBuf, right: &PathBuf, differences: &mut Vec<Difference>) -> FsResult<()> {
            let mut left_file = fs::File::open(left).await?;
            let mut right_file = fs::File::open(right).await?;

            let mut left_buffer = [0u8; 8192];
            let mut right_buffer = [0u8; 8192];
            let mut offset = 0u64;

            loop {
                let left_read = left_file.read(&mut left_buffer).await?;
                let right_read = right_file.read(&mut right_buffer).await?;

                if left_read == 0 && right_read == 0 {
                    break;
                }

                let min_read = min(left_read, right_read);
                for i in 0..min_read {
                    if left_buffer[i] != right_buffer[i] {
                        differences.push(Difference::BinaryDiff {
                            offset: offset + i as u64,
                            left: left_buffer[i],
                            right: right_buffer[i],
                        });
                    }
                }

                offset += min_read as u64;
            }

            Ok(())
        }

        async fn compare_text(&self, left: &PathBuf, right: &PathBuf, comparison_type: ComparisonType, differences: &mut Vec<Difference>) -> FsResult<()> {
            let left_file = fs::File::open(left).await?;
            let right_file = fs::File::open(right).await?;

            let mut left_reader = TokioBufReader::new(left_file);
            let mut right_reader = TokioBufReader::new(right_file);

            let mut left_line = String::new();
            let mut right_line = String::new();
            let mut line_number = 1;

            loop {
                left_line.clear();
                right_line.clear();

                let left_read = left_reader.read_line(&mut left_line).await?;
                let right_read = right_reader.read_line(&mut right_line).await?;

                if left_read == 0 && right_read == 0 {
                    break;
                }

                let left_processed = match comparison_type {
                    ComparisonType::TextIgnoreWhitespace => left_line.trim().to_string(),
                    ComparisonType::TextIgnoreCase => left_line.to_lowercase(),
                    _ => left_line.clone(),
                };

                let right_processed = match comparison_type {
                    ComparisonType::TextIgnoreWhitespace => right_line.trim().to_string(),
                    ComparisonType::TextIgnoreCase => right_line.to_lowercase(),
                    _ => right_line.clone(),
                };

                if left_processed != right_processed {
                    differences.push(Difference::TextDiff {
                        line: line_number,
                        left: left_line.clone(),
                        right: right_line.clone(),
                    });
                }

                line_number += 1;
            }

            Ok(())
        }

        /// Compare two directories recursively
        pub async fn compare_directories(&self, left: &PathBuf, right: &PathBuf, comparison_type: ComparisonType) -> FsResult<ComparisonResult> {
            let start_time = std::time::Instant::now();
            let mut differences = Vec::new();

            let mut left_entries = Vec::new();
            let mut right_entries = Vec::new();

            for entry in WalkDir::new(left).into_iter().filter_map(|e| e.ok()) {
                left_entries.push(entry.path().to_path_buf());
            }

            for entry in WalkDir::new(right).into_iter().filter_map(|e| e.ok()) {
                right_entries.push(entry.path().to_path_buf());
            }

            // Compare file lists
            for left_path in &left_entries {
                let relative_path = left_path.strip_prefix(left).unwrap();
                let right_path = right.join(relative_path);

                if !right_path.exists() {
                    differences.push(Difference::TypeDiff {
                        left_type: "file".to_string(),
                        right_type: "missing".to_string(),
                    });
                    continue;
                }

                if left_path.is_file() && right_path.is_file() {
                    let result = self.compare_files(left_path, &right_path, comparison_type).await?;
                    differences.extend(result.differences);
                }
            }

            // Check for files in right that don't exist in left
            for right_path in &right_entries {
                let relative_path = right_path.strip_prefix(right).unwrap();
                let left_path = left.join(relative_path);

                if !left_path.exists() {
                    differences.push(Difference::TypeDiff {
                        left_type: "missing".to_string(),
                        right_type: "file".to_string(),
                    });
                }
            }

            let time_ms = start_time.elapsed().as_millis() as u64;

            let diff_clone = differences.clone();
            Ok(ComparisonResult {
                identical: diff_clone.is_empty(),
                differences,
                total_differences: diff_clone.len(),
                time_ms,
            })
        }

        /// Start monitoring with settings
        pub async fn start_monitoring_with_settings(&mut self, settings: MonitoringSettings) -> FsResult<()> {
            let (tx, rx) = mpsc::channel();
            self.event_receiver = Some(rx);
            let settings_cloned = settings.clone();
            let mut watcher = notify::recommended_watcher(move |res: std::result::Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    if settings_cloned.filter.matches(&event.paths[0]) {
                        let _ = tx.send(FsEvent::from(event));
                    }
                }
            })?;
            watcher.watch(&settings.path, if settings.recursive { RecursiveMode::Recursive } else { RecursiveMode::NonRecursive })?;
            Ok(())
        }

        /// Save monitoring settings to file
        pub async fn save_monitoring_settings(&self, settings: &MonitoringSettings, path: &PathBuf) -> FsResult<()> {
            let file = File::create(path)?;
            let writer = BufWriter::new(file);
            serde_json::to_writer_pretty(writer, settings)
                .map_err(|e| FsError::Serialization(e.to_string()))?;
            Ok(())
        }

        /// Load monitoring settings from file
        pub async fn load_monitoring_settings(&self, path: &PathBuf) -> FsResult<MonitoringSettings> {
            let file = File::open(path)?;
            let reader = StdBufReader::new(file);
            let settings: MonitoringSettings = serde_json::from_reader(reader)
                .map_err(|e| FsError::Serialization(e.to_string()))?;
            Ok(settings)
        }
    }

    impl From<Event> for FsEvent {
        fn from(event: Event) -> Self {
            let event_type = match event.kind {
                EventKind::Create(_) => FsEventType::Create,
                EventKind::Modify(_) => FsEventType::Modify,
                EventKind::Remove(_) => FsEventType::Remove,
                EventKind::Access(_) => FsEventType::Access,
                _ => FsEventType::Metadata,
            };
            let path = event.paths.first().cloned().unwrap_or_default();
            let timestamp = Local::now();
            let mut metadata = HashMap::new();
            match event.kind {
                EventKind::Modify(ModifyKind::Data(_)) => {
                    metadata.insert("modification_type".to_string(), "data".to_string());
                }
                EventKind::Modify(ModifyKind::Metadata(_)) => {
                    metadata.insert("modification_type".to_string(), "metadata".to_string());
                }
                _ => {}
            }
            Self {
                event_type,
                path,
                timestamp,
                metadata,
            }
        }
    }
}

/// Module providing plugin system functionality
pub mod plugin {
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