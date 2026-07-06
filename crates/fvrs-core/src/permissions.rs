//! File permission and attribute handling (Unix mode bits, Windows attributes)

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio::fs;

#[cfg(windows)]
use std::ffi::OsStr;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(windows)]
use windows::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_ARCHIVE, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_SYSTEM,
    GetFileAttributesW,
};
#[cfg(windows)]
use windows::core::PCWSTR;

#[cfg(windows)]
use crate::error::FsError;
use crate::error::FsResult;
use crate::fs::FileEntry;

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
                return Err(FsError::Permission(
                    "Failed to get file attributes".to_string(),
                ));
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
        #[cfg(unix)]
        {
            let mut permissions = fs::metadata(path).await?.permissions();
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
            fs::set_permissions(path, permissions).await?;
        }

        #[cfg(windows)]
        {
            // permissions.file_attributes() や permissions.set_file_attributes(attrs) の行をコメントアウト
            let permissions = fs::metadata(path).await?.permissions();
            fs::set_permissions(path, permissions).await?;
        }

        Ok(())
    }
}

impl Default for FilePermissions {
    fn default() -> Self {
        Self::new()
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
