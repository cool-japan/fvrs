use native_windows_gui as nwg;
use native_windows_derive as nwd;
use std::path::PathBuf;
use std::rc::Rc;
use std::cell::RefCell;
use thiserror::Error;
use std::collections::HashSet;
use std::fs;

use crate::Result;

#[derive(Debug, Error)]
pub enum FilterError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Invalid filter: {0}")]
    InvalidFilter(String),
}

/// Filter panel structure
#[derive(Default, NwgUi)]
pub struct FilterPanel {
    #[nwg_control(parent: window, size: (780, 100), position: (10, 10))]
    panel: nwg::Panel,
    
    #[nwg_control(parent: panel, text: "Filter:", size: (60, 20), position: (10, 10))]
    filter_label: nwg::Label,
    
    #[nwg_control(parent: panel, size: (500, 20), position: (80, 10))]
    filter_input: nwg::TextInput,
    
    #[nwg_control(parent: panel, text: "Case sensitive", size: (100, 20), position: (590, 10))]
    case_sensitive: nwg::CheckBox,
    
    #[nwg_control(parent: panel, text: "Use regex", size: (100, 20), position: (590, 40))]
    use_regex: nwg::CheckBox,
    
    #[nwg_control(parent: panel, text: "Apply", size: (80, 30), position: (690, 10))]
    apply_button: nwg::Button,
    
    #[nwg_control(parent: panel, text: "Clear", size: (80, 30), position: (690, 50))]
    clear_button: nwg::Button,
    
    // Internal state
    current_path: RefCell<PathBuf>,
    current_filter: RefCell<Option<String>>,
    filtered_items: RefCell<HashSet<PathBuf>>,
}

impl FilterPanel {
    pub fn new() -> Result<Self> {
        let mut panel = Self::default();
        panel.build()?;
        Ok(panel)
    }
    
    /// Set the current path
    pub fn set_path(&self, path: &PathBuf) {
        *self.current_path.borrow_mut() = path.clone();
    }
    
    /// Apply the current filter
    pub fn apply_filter(&self) -> Result<()> {
        let pattern = self.filter_input.text();
        if pattern.is_empty() {
            self.clear_filter()?;
            return Ok(());
        }
        
        let case_sensitive = self.case_sensitive.checked();
        let use_regex = self.use_regex.checked();
        let path = self.current_path.borrow().clone();
        
        // Create regex if needed
        let regex = if use_regex {
            Some(regex::Regex::new(&pattern).map_err(|e| FilterError::InvalidFilter(e.to_string()))?)
        } else {
            None
        };
        
        // Clear previous filter
        self.filtered_items.borrow_mut().clear();
        
        // Apply new filter
        for entry in fs::read_dir(&path)? {
            let entry = entry?;
            let path = entry.path();
            let name = path.file_name().unwrap().to_str().unwrap();
            let name_to_check = if case_sensitive { name } else { &name.to_lowercase() };
            
            let matches = if let Some(ref regex) = regex {
                regex.is_match(name_to_check)
            } else {
                name_to_check.contains(&pattern)
            };
            
            if matches {
                self.filtered_items.borrow_mut().insert(path);
            }
        }
        
        *self.current_filter.borrow_mut() = Some(pattern);
        Ok(())
    }
    
    /// Clear the current filter
    pub fn clear_filter(&self) -> Result<()> {
        self.filter_input.set_text("");
        self.filtered_items.borrow_mut().clear();
        *self.current_filter.borrow_mut() = None;
        Ok(())
    }
    
    /// Check if an item matches the current filter
    pub fn matches_filter(&self, path: &PathBuf) -> bool {
        if let Some(ref filter) = *self.current_filter.borrow() {
            self.filtered_items.borrow().contains(path)
        } else {
            true
        }
    }
    
    /// Get the current filter pattern
    pub fn current_filter(&self) -> Option<String> {
        self.current_filter.borrow().clone()
    }
} 