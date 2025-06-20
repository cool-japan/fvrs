use std::path::PathBuf;
use std::collections::HashMap;
use thiserror::Error;
use std::fs;
use std::io;
use std::sync::Arc;
use std::sync::Mutex;
use libloading::{Library, Symbol};
use fvrs_plugin_api::{Plugin, PluginInfo, PluginType, PluginError as ApiError};
use serde::{Serialize, Deserialize};

#[derive(Debug, Error)]
pub enum PluginError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    
    #[error("Plugin error: {0}")]
    Plugin(#[from] ApiError),
    
    #[error("Invalid plugin: {0}")]
    InvalidPlugin(String),
    
    #[error("Plugin not found: {0}")]
    PluginNotFound(String),
    
    #[error("Library error: {0}")]
    Library(#[from] libloading::Error),
}

type Result<T> = std::result::Result<T, PluginError>;

/// Plugin configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    pub enabled: bool,
    pub settings: HashMap<String, String>,
}

/// Plugin manager structure
pub struct PluginManager {
    plugins: HashMap<String, Box<dyn Plugin>>,
    configs: HashMap<String, PluginConfig>,
    plugin_dir: PathBuf,
    libraries: Vec<Library>,
}

impl PluginManager {
    pub fn new(plugin_dir: PathBuf) -> Result<Self> {
        let mut manager = Self {
            plugins: HashMap::new(),
            configs: HashMap::new(),
            plugin_dir,
            libraries: Vec::new(),
        };
        
        // Create plugin directory
        if !manager.plugin_dir.exists() {
            fs::create_dir_all(&manager.plugin_dir)?;
        }
        
        // Load configuration file
        manager.load_configs()?;
        
        // Load plugins
        manager.load_plugins()?;
        
        Ok(manager)
    }
    
    /// Load plugins
    fn load_plugins(&mut self) -> Result<()> {
        for entry in fs::read_dir(&self.plugin_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_file() && path.extension().map_or(false, |ext| ext == "dll") {
                if let Ok(plugin) = self.load_plugin(&path) {
                    let info = plugin.info();
                    self.plugins.insert(info.name.clone(), plugin);
                }
            }
        }
        Ok(())
    }
    
    /// Load individual plugin
    fn load_plugin(&mut self, path: &PathBuf) -> Result<Box<dyn Plugin>> {
        unsafe {
            // Load library
            let library = Library::new(path)?;
            
            // Get plugin entry point
            let create_plugin: Symbol<unsafe extern "C" fn() -> Box<dyn Plugin>> = 
                library.get(b"create_plugin")?;
            
            // Create plugin
            let plugin = create_plugin();
            
            // Keep library
            self.libraries.push(library);
            
            Ok(plugin)
        }
    }
    
    /// Reload plugin
    pub fn reload_plugin(&mut self, name: &str) -> Result<()> {
        // Unload existing plugin
        self.unload_plugin(name)?;
        
        // Get plugin path
        let plugin_path = self.plugin_dir.join(format!("{}.dll", name));
        if !plugin_path.exists() {
            return Err(PluginError::PluginNotFound(name.into()));
        }
        
        // Reload plugin
        let plugin = self.load_plugin(&plugin_path)?;
        let info = plugin.info();
        self.plugins.insert(info.name.clone(), plugin);
        
        Ok(())
    }
    
    /// Unload plugin
    fn unload_plugin(&mut self, name: &str) -> Result<()> {
        // Remove plugin
        self.plugins.remove(name);
        
        // Unload library
        if let Some(library) = self.libraries.iter().position(|lib| {
            lib.path().map_or(false, |p| p.file_stem().map_or(false, |s| s == name))
        }) {
            self.libraries.remove(library);
        }
        
        Ok(())
    }
    
    /// Load configuration
    fn load_configs(&mut self) -> Result<()> {
        let config_path = self.plugin_dir.join("config.json");
        if config_path.exists() {
            let content = fs::read_to_string(config_path)?;
            self.configs = serde_json::from_str(&content)?;
        }
        Ok(())
    }
    
    /// Save configuration
    fn save_configs(&self) -> Result<()> {
        let config_path = self.plugin_dir.join("config.json");
        let content = serde_json::to_string_pretty(&self.configs)?;
        fs::write(config_path, content)?;
        Ok(())
    }
    
    /// Enable/disable plugin
    pub fn set_plugin_enabled(&mut self, name: &str, enabled: bool) -> Result<()> {
        if let Some(config) = self.configs.get_mut(name) {
            config.enabled = enabled;
            self.save_configs()?;
        }
        Ok(())
    }
    
    /// Update plugin settings
    pub fn update_plugin_settings(&mut self, name: &str, settings: HashMap<String, String>) -> Result<()> {
        if let Some(config) = self.configs.get_mut(name) {
            config.settings = settings;
            self.save_configs()?;
        }
        Ok(())
    }
    
    /// Get list of plugins
    pub fn list_plugins(&self) -> Vec<PluginInfo> {
        self.plugins.values()
            .map(|p| p.info())
            .collect()
    }
    
    /// Get list of enabled plugins
    pub fn list_enabled_plugins(&self) -> Vec<PluginInfo> {
        self.plugins.values()
            .filter(|p| self.configs.get(&p.info().name)
                .map_or(false, |c| c.enabled))
            .map(|p| p.info())
            .collect()
    }
    
    /// Get plugins by type
    pub fn get_plugins_by_type(&self, plugin_type: PluginType) -> Vec<&Box<dyn Plugin>> {
        self.plugins.values()
            .filter(|p| p.info().plugin_type == plugin_type)
            .collect()
    }
    
    /// Get plugin configuration
    pub fn get_plugin_config(&self, name: &str) -> Option<&PluginConfig> {
        self.configs.get(name)
    }
    
    /// Execute plugin
    pub fn execute_plugin(&self, name: &str, input: &[u8]) -> Result<Vec<u8>> {
        if let Some(plugin) = self.plugins.get(name) {
            if self.configs.get(name).map_or(false, |c| c.enabled) {
                plugin.execute(input)
            } else {
                Err(PluginError::InvalidPlugin("Plugin is disabled".into()))
            }
        } else {
            Err(PluginError::PluginNotFound(name.into()))
        }
    }
}

impl Drop for PluginManager {
    fn drop(&mut self) {
        // Unload plugins
        self.plugins.clear();
        self.libraries.clear();
    }
} 