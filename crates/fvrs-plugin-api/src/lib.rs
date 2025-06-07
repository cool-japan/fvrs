//! Plugin API for FVRS
//!
//! This crate defines the plugin interface and types used by FVRS plugins.

use std::path::PathBuf;
use thiserror::Error;

/// Error type for plugin operations
#[derive(Error, Debug)]
pub enum PluginError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Plugin initialization failed: {0}")]
    Initialization(String),
    #[error("Plugin operation failed: {0}")]
    Operation(String),
}

/// Result type for plugin operations
pub type PluginResult<T> = Result<T, PluginError>;

/// Basic trait for plugins
pub trait Plugin {
    /// Get plugin name
    fn name(&self) -> &str;
    
    /// Get plugin version
    fn version(&self) -> &str;
    
    /// Initialize the plugin
    fn initialize(&mut self) -> PluginResult<()>;
    
    /// Shutdown the plugin
    fn shutdown(&mut self) -> PluginResult<()>;
}

/// Trait for file operation plugins
pub trait FileOperationPlugin: Plugin {
    /// Copy a file
    fn copy_file(&self, src: &PathBuf, dest: &PathBuf) -> PluginResult<()>;
    
    /// Move a file
    fn move_file(&self, src: &PathBuf, dest: &PathBuf) -> PluginResult<()>;
    
    /// Delete a file
    fn delete_file(&self, path: &PathBuf) -> PluginResult<()>;
}

/// Trait for file view plugins
pub trait FileViewPlugin: Plugin {
    /// Get file preview
    fn get_preview(&self, path: &PathBuf) -> PluginResult<String>;
    
    /// Get file metadata
    fn get_metadata(&self, path: &PathBuf) -> PluginResult<FileMetadata>;
}

/// File metadata structure
#[derive(Debug, Clone)]
pub struct FileMetadata {
    /// File size
    pub size: u64,
    /// Creation time
    pub created: chrono::DateTime<chrono::Local>,
    /// Last modification time
    pub modified: chrono::DateTime<chrono::Local>,
    /// File type
    pub file_type: String,
    /// Additional metadata
    pub additional_info: std::collections::HashMap<String, String>,
}

/// Command specification for plugins
#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub name: String,
    pub description: String,
    pub shortcut: Option<String>,
}

/// Command execution context
pub struct CommandContext {
    pub current_path: PathBuf,
    pub selected_files: Vec<PathBuf>,
}

/// Plugin trait that all FVRS plugins must implement
pub trait Plugin {
    /// Get the plugin name
    fn name(&self) -> &str;
    
    /// Get the plugin description
    fn description(&self) -> &str;
    
    /// Get the list of commands provided by this plugin
    fn commands(&self) -> Vec<CommandSpec>;
    
    /// Execute a command
    fn run(&self, ctx: &mut CommandContext) -> Result<()>;
} 