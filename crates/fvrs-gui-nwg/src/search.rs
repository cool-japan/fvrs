use native_windows_gui as nwg;
use native_windows_derive as nwd;
use std::path::PathBuf;
use std::rc::Rc;
use std::cell::RefCell;
use std::sync::mpsc;
use std::thread;
use thiserror::Error;
use walkdir::WalkDir;
use regex::Regex;
use std::fs;
use std::io::Read;
use std::collections::HashMap;

use crate::Result;

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Invalid regex: {0}")]
    InvalidRegex(#[from] regex::Error),
    
    #[error("Search cancelled")]
    Cancelled,
}

/// Search panel structure
#[derive(Default, NwgUi)]
pub struct SearchPanel {
    #[nwg_control(parent: window, size: (780, 150), position: (10, 10))]
    panel: nwg::Panel,
    
    #[nwg_control(parent: panel, text: "Search:", size: (60, 20), position: (10, 10))]
    search_label: nwg::Label,
    
    #[nwg_control(parent: panel, size: (500, 20), position: (80, 10))]
    search_input: nwg::TextInput,
    
    #[nwg_control(parent: panel, text: "Case sensitive", size: (100, 20), position: (590, 10))]
    case_sensitive: nwg::CheckBox,
    
    #[nwg_control(parent: panel, text: "Use regex", size: (100, 20), position: (590, 40))]
    use_regex: nwg::CheckBox,
    
    #[nwg_control(parent: panel, text: "Search in content", size: (120, 20), position: (590, 70))]
    search_content: nwg::CheckBox,
    
    #[nwg_control(parent: panel, text: "Start", size: (80, 30), position: (690, 10))]
    start_button: nwg::Button,
    
    #[nwg_control(parent: panel, text: "Stop", size: (80, 30), position: (690, 50))]
    stop_button: nwg::Button,
    
    #[nwg_control(parent: panel, text: "Results: 0", size: (200, 20), position: (10, 120))]
    results_label: nwg::Label,
    
    // Internal state
    current_path: RefCell<PathBuf>,
    search_running: RefCell<bool>,
    search_results: RefCell<HashMap<PathBuf, Vec<(usize, String)>>>,
    search_tx: RefCell<Option<mpsc::Sender<()>>>,
}

impl SearchPanel {
    pub fn new() -> Result<Self> {
        let mut panel = Self::default();
        panel.build()?;
        
        // Initialize state
        *panel.search_running.borrow_mut() = false;
        *panel.search_results.borrow_mut() = HashMap::new();
        
        Ok(panel)
    }
    
    /// Set the current path
    pub fn set_path(&self, path: &PathBuf) {
        *self.current_path.borrow_mut() = path.clone();
    }
    
    /// Start the search
    pub fn start_search(&self) -> Result<()> {
        if *self.search_running.borrow() {
            return Ok(());
        }
        
        let pattern = self.search_input.text();
        if pattern.is_empty() {
            return Ok(());
        }
        
        let case_sensitive = self.case_sensitive.checked();
        let use_regex = self.use_regex.checked();
        let search_content = self.search_content.checked();
        let path = self.current_path.borrow().clone();
        
        // Create regex if needed
        let regex = if use_regex {
            Some(Regex::new(&pattern).map_err(|e| SearchError::InvalidRegex(e))?)
        } else {
            None
        };
        
        // Clear previous results
        self.search_results.borrow_mut().clear();
        self.results_label.set_text("Results: 0");
        
        // Create channel for cancellation
        let (tx, rx) = mpsc::channel();
        *self.search_tx.borrow_mut() = Some(tx);
        *self.search_running.borrow_mut() = true;
        
        // Start search in a separate thread
        let search_results = self.search_results.clone();
        let results_label = self.results_label.handle.clone();
        thread::spawn(move || {
            let mut results = HashMap::new();
            let mut count = 0;
            
            for entry in WalkDir::new(&path).into_iter().filter_map(|e| e.ok()) {
                // Check for cancellation
                if rx.try_recv().is_ok() {
                    break;
                }
                
                let path = entry.path();
                if path.is_dir() {
                    continue;
                }
                
                // Search in filename
                let name = path.file_name().unwrap().to_str().unwrap();
                let name_to_check = if case_sensitive { name } else { &name.to_lowercase() };
                
                let matches = if let Some(ref regex) = regex {
                    regex.is_match(name_to_check)
                } else {
                    name_to_check.contains(&pattern)
                };
                
                if matches {
                    results.insert(path.to_path_buf(), vec![(0, "Filename match".to_string())]);
                    count += 1;
                    nwg::dispatch_thread_events();
                    nwg::Control::set_text(&results_label, &format!("Results: {}", count));
                }
                
                // Search in content if enabled
                if search_content && path.is_file() {
                    if let Ok(mut file) = fs::File::open(path) {
                        let mut content = String::new();
                        if file.read_to_string(&mut content).is_ok() {
                            let content_to_check = if case_sensitive { &content } else { &content.to_lowercase() };
                            
                            if let Some(ref regex) = regex {
                                for (i, line) in content_to_check.lines().enumerate() {
                                    if regex.is_match(line) {
                                        results.entry(path.to_path_buf())
                                            .or_insert_with(Vec::new)
                                            .push((i + 1, line.trim().to_string()));
                                        count += 1;
                                        nwg::dispatch_thread_events();
                                        nwg::Control::set_text(&results_label, &format!("Results: {}", count));
                                    }
                                }
                            } else {
                                for (i, line) in content_to_check.lines().enumerate() {
                                    if line.contains(&pattern) {
                                        results.entry(path.to_path_buf())
                                            .or_insert_with(Vec::new)
                                            .push((i + 1, line.trim().to_string()));
                                        count += 1;
                                        nwg::dispatch_thread_events();
                                        nwg::Control::set_text(&results_label, &format!("Results: {}", count));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            *search_results.borrow_mut() = results;
            nwg::dispatch_thread_events();
            nwg::Control::set_text(&results_label, &format!("Results: {}", count));
        });
        
        Ok(())
    }
    
    /// Stop the search
    pub fn stop_search(&self) -> Result<()> {
        if let Some(tx) = self.search_tx.borrow_mut().take() {
            let _ = tx.send(());
        }
        *self.search_running.borrow_mut() = false;
        Ok(())
    }
    
    /// Get the search results
    pub fn get_results(&self) -> HashMap<PathBuf, Vec<(usize, String)>> {
        self.search_results.borrow().clone()
    }
    
    /// Check if search is running
    pub fn is_running(&self) -> bool {
        *self.search_running.borrow()
    }
}

impl Drop for SearchPanel {
    fn drop(&mut self) {
        self.stop_search();
    }
} 