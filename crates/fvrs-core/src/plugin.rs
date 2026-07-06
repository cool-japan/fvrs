//! Plugin system functionality

/// Basic trait for plugins
pub trait Plugin {
    /// Get plugin name
    fn name(&self) -> &str;

    /// Get plugin version
    fn version(&self) -> &str;

    /// Initialize the plugin
    fn initialize(&mut self) -> anyhow::Result<()>;
}
