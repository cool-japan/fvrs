use std::path::PathBuf;
use std::collections::HashSet;
use thiserror::Error;
use std::fs;
use std::io;

#[derive(Debug, Error)]
pub enum ClipboardError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
}

/// Clipboard operation type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClipboardOp {
    Copy,
    Cut,
}

/// Structure to manage clipboard state
pub struct Clipboard {
    operation: Option<ClipboardOp>,
    items: HashSet<PathBuf>,
}

impl Clipboard {
    pub fn new() -> Self {
        Self {
            operation: None,
            items: HashSet::new(),
        }
    }
    
    /// Set items to clipboard
    pub fn set_items(&mut self, items: HashSet<PathBuf>, operation: ClipboardOp) {
        self.operation = Some(operation);
        self.items = items;
    }
    
    /// Get clipboard contents
    pub fn get_items(&self) -> Option<(&HashSet<PathBuf>, ClipboardOp)> {
        self.operation.map(|op| (&self.items, op))
    }
    
    /// Clear clipboard
    pub fn clear(&mut self) {
        self.operation = None;
        self.items.clear();
    }
    
    /// Paste clipboard contents
    pub fn paste(&self, dest: &PathBuf) -> Result<(), ClipboardError> {
        if let Some((items, operation)) = self.get_items() {
            for source in items {
                let dest_path = dest.join(source.file_name().unwrap());
                
                match operation {
                    ClipboardOp::Copy => {
                        if source.is_dir() {
                            copy_dir(source, &dest_path)?;
                        } else {
                            fs::copy(source, &dest_path)?;
                        }
                    }
                    ClipboardOp::Cut => {
                        if source.is_dir() {
                            copy_dir(source, &dest_path)?;
                            fs::remove_dir_all(source)?;
                        } else {
                            fs::copy(source, &dest_path)?;
                            fs::remove_file(source)?;
                        }
                    }
                }
            }
            Ok(())
        } else {
            Err(ClipboardError::InvalidOperation("Clipboard is empty".into()))
        }
    }
}

/// Recursively copy directory
fn copy_dir(src: &PathBuf, dest: &PathBuf) -> Result<(), ClipboardError> {
    if !dest.exists() {
        fs::create_dir_all(dest)?;
    }
    
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let dest_path = dest.join(path.file_name().unwrap());
        
        if path.is_dir() {
            copy_dir(&path, &dest_path)?;
        } else {
            fs::copy(&path, &dest_path)?;
        }
    }
    
    Ok(())
} 