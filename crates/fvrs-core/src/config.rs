//! Application configuration management

use std::path::PathBuf;

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
