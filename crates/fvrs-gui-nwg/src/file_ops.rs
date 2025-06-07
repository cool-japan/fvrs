use native_windows_gui as nwg;
use std::path::{Path, PathBuf};
use std::fs;
use std::io;
use std::time::SystemTime;
use thiserror::Error;
use crate::Result;
use fvrs_core::SortBy;

/// File operation error type
#[derive(Error, Debug)]
pub enum FileOpError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    
    #[error("Source file not found: {0}")]
    SourceNotFound(PathBuf),
    
    #[error("Destination already exists: {0}")]
    DestinationExists(PathBuf),
    
    #[error("Operation cancelled by user")]
    Cancelled,
}

/// Result type for file operations
type FileOpResult<T> = std::result::Result<T, FileOpError>;

/// Copy a file or directory
pub fn copy_file(source: &Path, dest: &Path) -> FileOpResult<()> {
    if !source.exists() {
        return Err(FileOpError::SourceNotFound(source.to_path_buf()));
    }
    
    if dest.exists() {
        return Err(FileOpError::DestinationExists(dest.to_path_buf()));
    }
    
    if source.is_file() {
        fs::copy(source, dest)?;
    } else if source.is_dir() {
        copy_dir(source, dest)?;
    }
    
    Ok(())
}

/// Copy a directory recursively
fn copy_dir(source: &Path, dest: &Path) -> FileOpResult<()> {
    fs::create_dir(dest)?;
    
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        let dest_path = dest.join(path.file_name().unwrap());
        
        if path.is_file() {
            fs::copy(&path, &dest_path)?;
        } else if path.is_dir() {
            copy_dir(&path, &dest_path)?;
        }
    }
    
    Ok(())
}

/// Move a file or directory
pub fn move_file(source: &Path, dest: &Path) -> FileOpResult<()> {
    if !source.exists() {
        return Err(FileOpError::SourceNotFound(source.to_path_buf()));
    }
    
    if dest.exists() {
        return Err(FileOpError::DestinationExists(dest.to_path_buf()));
    }
    
    fs::rename(source, dest)?;
    Ok(())
}

/// Delete a file or directory
pub fn delete_file(path: &Path) -> FileOpResult<()> {
    if !path.exists() {
        return Err(FileOpError::SourceNotFound(path.to_path_buf()));
    }
    
    if path.is_file() {
        fs::remove_file(path)?;
    } else if path.is_dir() {
        fs::remove_dir_all(path)?;
    }
    
    Ok(())
}

/// Show a file operation confirmation dialog
pub fn show_confirm_dialog(title: &str, message: &str) -> bool {
    let mut dialog = nwg::ModalInfo::default();
    dialog.title(title);
    dialog.message(message);
    dialog.buttons(nwg::MessageButtons::YesNo);
    dialog.icon(nwg::MessageIcon::Question);
    
    nwg::modal_info(&dialog) == nwg::MessageResponse::Yes
}

/// Show a file operation error dialog
pub fn show_error_dialog(title: &str, message: &str) {
    let mut dialog = nwg::ModalInfo::default();
    dialog.title(title);
    dialog.message(message);
    dialog.buttons(nwg::MessageButtons::Ok);
    dialog.icon(nwg::MessageIcon::Error);
    
    nwg::modal_info(&dialog);
}

/// File entry information
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: SystemTime,
}

impl FileEntry {
    pub fn new(path: PathBuf) -> io::Result<Self> {
        let metadata = fs::metadata(&path)?;
        let name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        
        Ok(Self {
            path,
            name,
            is_dir: metadata.is_dir(),
            size: metadata.len(),
            modified: metadata.modified()?,
        })
    }
}

/// Get directory entries sorted by the specified criteria
pub fn get_sorted_entries(path: &Path, sort_by: SortBy) -> FileOpResult<Vec<FileEntry>> {
    let mut entries = Vec::new();
    
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if let Ok(file_entry) = FileEntry::new(path) {
            entries.push(file_entry);
        }
    }
    
    match sort_by {
        SortBy::Name => {
            entries.sort_by(|a, b| a.name.cmp(&b.name));
        }
        SortBy::Size => {
            entries.sort_by(|a, b| b.size.cmp(&a.size));
        }
        SortBy::Modified => {
            entries.sort_by(|a, b| b.modified.cmp(&a.modified));
        }
        SortBy::Created => {
            // TODO: Implement created time sorting
            entries.sort_by(|a, b| a.name.cmp(&b.name));
        }
    }
    
    Ok(entries)
}

/// Drag and drop operation type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragOp {
    Copy,
    Move,
}

/// Handle a dropped file
pub fn handle_drop(source: &Path, dest: &Path, op: DragOp) -> FileOpResult<()> {
    if !source.exists() {
        return Err(FileOpError::SourceNotFound(source.to_path_buf()));
    }
    
    if !dest.exists() {
        return Err(FileOpError::DestinationExists(dest.to_path_buf()));
    }
    
    let dest_path = if fs::metadata(dest)?.is_dir() {
        dest.join(source.file_name().ok_or_else(|| {
            FileOpError::SourceNotFound(source.to_path_buf())
        })?)
    } else {
        dest.to_path_buf()
    };
    
    match op {
        DragOp::Copy => {
            if fs::metadata(source)?.is_dir() {
                copy_dir(source, &dest_path)?;
            } else {
                fs::copy(source, &dest_path)?;
            }
        }
        DragOp::Move => {
            fs::rename(source, &dest_path)?;
        }
    }
    
    Ok(())
}

/// Get the default drag operation based on modifier keys
pub fn get_drag_op(ctrl: bool, shift: bool) -> DragOp {
    if ctrl {
        DragOp::Copy
    } else {
        DragOp::Move
    }
}

/// Format a file size for display
pub fn format_size(size: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut size = size as f64;
    let mut unit_index = 0;
    
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    
    format!("{:.1} {}", size, UNITS[unit_index])
}

/// Format a timestamp for display
pub fn format_time(time: SystemTime) -> String {
    if let Ok(duration) = time.duration_since(SystemTime::UNIX_EPOCH) {
        let secs = duration.as_secs();
        let datetime = chrono::DateTime::from_timestamp(secs as i64, 0)
            .unwrap_or_default();
        datetime.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        "Unknown".to_string()
    }
}

#[derive(Debug, Error)]
pub enum FileOpsError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Invalid path: {0}")]
    InvalidPath(String),
    
    #[error("Operation failed: {0}")]
    OperationFailed(String),
}

pub type Result<T> = std::result::Result<T, FileOpsError>;

/// Open a file with the default application
pub fn open_file(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(FileOpsError::InvalidPath(path.display().to_string()));
    }
    
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        Command::new("cmd")
            .args(&["/C", "start", "", path.to_str().unwrap()])
            .output()?;
    }
    
    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        Command::new("xdg-open")
            .arg(path)
            .output()?;
    }
    
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        Command::new("open")
            .arg(path)
            .output()?;
    }
    
    Ok(())
}

/// Open a file with a specific application
pub fn open_file_with(path: &Path, app: &Path) -> Result<()> {
    if !path.exists() {
        return Err(FileOpsError::InvalidPath(path.display().to_string()));
    }
    
    if !app.exists() {
        return Err(FileOpsError::InvalidPath(app.display().to_string()));
    }
    
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        Command::new(app)
            .arg(path)
            .output()?;
    }
    
    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        Command::new(app)
            .arg(path)
            .output()?;
    }
    
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        Command::new("open")
            .arg("-a")
            .arg(app)
            .arg(path)
            .output()?;
    }
    
    Ok(())
}

/// Rename a file
pub fn rename_file(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(FileOpsError::InvalidPath(path.display().to_string()));
    }
    
    let mut dialog = nwg::InputDialog::default();
    dialog.title("Rename");
    dialog.text("Enter new name:");
    dialog.default_text(path.file_name().unwrap().to_str().unwrap());
    
    if dialog.show() {
        if let Some(new_name) = dialog.text() {
            let new_path = path.parent().unwrap().join(new_name);
            if new_path.exists() {
                return Err(FileOpsError::OperationFailed(format!(
                    "A file with the name '{}' already exists",
                    new_name
                )));
            }
            fs::rename(path, new_path)?;
        }
    }
    
    Ok(())
}

/// Show file properties
pub fn show_properties(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(FileOpsError::InvalidPath(path.display().to_string()));
    }
    
    let metadata = fs::metadata(path)?;
    let mut info = String::new();
    
    info.push_str(&format!("Name: {}\n", path.file_name().unwrap().to_str().unwrap()));
    info.push_str(&format!("Type: {}\n", if metadata.is_dir() { "Folder" } else { "File" }));
    info.push_str(&format!("Size: {}\n", format_size(metadata.len())));
    info.push_str(&format!("Created: {}\n", format_time(metadata.created()?)));
    info.push_str(&format!("Modified: {}\n", format_time(metadata.modified()?)));
    
    if metadata.is_file() {
        info.push_str(&format!("Read-only: {}\n", metadata.permissions().readonly()));
    }
    
    nwg::modal_info_message(&nwg::Window::default(), "Properties", &info);
    
    Ok(())
}

/// Paste files from clipboard
pub fn paste_files(dest: &Path) -> Result<()> {
    if !dest.exists() {
        return Err(FileOpsError::InvalidPath(dest.display().to_string()));
    }
    
    if !dest.is_dir() {
        return Err(FileOpsError::OperationFailed("Destination must be a directory".to_string()));
    }
    
    // TODO: Implement clipboard operations
    // This would require platform-specific clipboard APIs
    
    Ok(())
} 