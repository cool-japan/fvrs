//! Core filesystem primitives: directory listing, create/remove/copy/move

use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};
use notify::RecommendedWatcher;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::sync::mpsc::Receiver;
use walkdir::WalkDir;

use crate::error::{FsError, FsResult};
use crate::monitor::FsEvent;

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

/// Structure providing basic filesystem operations
pub struct FileSystem {
    /// Current working directory
    pub(crate) current_dir: PathBuf,
    /// File system event receiver (bounded — see `monitor::EVENT_CHANNEL_CAPACITY`)
    pub(crate) event_receiver: Option<Receiver<FsEvent>>,
    /// Active file system watcher — kept alive here so events keep flowing
    pub(crate) watcher: Option<RecommendedWatcher>,
}

impl Default for FileSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl FileSystem {
    /// Create a new FileSystem instance
    pub fn new() -> Self {
        Self {
            current_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            event_receiver: None,
            watcher: None,
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

    /// Copy a file or directory (directories are copied recursively)
    pub async fn copy(&self, src: &Path, dest: &Path) -> FsResult<()> {
        if src.is_dir() {
            Self::copy_dir_all(src, dest).await?;
        } else {
            fs::copy(src, dest).await?;
        }
        Ok(())
    }

    /// Recursively copy a directory tree (iterative, avoids unbounded async recursion)
    async fn copy_dir_all(src: &Path, dst: &Path) -> FsResult<()> {
        let mut pending = vec![(src.to_path_buf(), dst.to_path_buf())];
        while let Some((current_src, current_dst)) = pending.pop() {
            fs::create_dir_all(&current_dst).await?;
            let mut read_dir = fs::read_dir(&current_src).await?;
            while let Some(entry) = read_dir.next_entry().await? {
                let file_type = entry.file_type().await?;
                let target = current_dst.join(entry.file_name());
                if file_type.is_dir() {
                    pending.push((entry.path(), target));
                } else {
                    fs::copy(entry.path(), &target).await?;
                }
            }
        }
        Ok(())
    }

    /// Move a file or directory
    pub async fn move_file(&self, src: &PathBuf, dest: &PathBuf) -> FsResult<()> {
        fs::rename(src, dest).await?;
        Ok(())
    }
}
