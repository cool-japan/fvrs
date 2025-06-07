use native_windows_gui as nwg;
use std::path::PathBuf;
use thiserror::Error;
use std::fs;
use std::io;

#[derive(Debug, Error)]
pub enum DragDropError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
}

/// Drag & Drop operation type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DragOp {
    Copy,
    Move,
}

/// Structure to manage drag & drop state
pub struct DragDrop {
    source_path: Option<PathBuf>,
    operation: Option<DragOp>,
}

impl DragDrop {
    pub fn new() -> Self {
        Self {
            source_path: None,
            operation: None,
        }
    }
    
    /// Start drag
    pub fn start_drag(&mut self, path: PathBuf, operation: DragOp) {
        self.source_path = Some(path);
        self.operation = Some(operation);
    }
    
    /// End drag
    pub fn end_drag(&mut self) {
        self.source_path = None;
        self.operation = None;
    }
    
    /// Handle drop
    pub fn handle_drop(&self, target_path: &PathBuf) -> Result<(), DragDropError> {
        if let (Some(source), Some(operation)) = (&self.source_path, self.operation) {
            if source == target_path {
                return Err(DragDropError::InvalidOperation("Cannot drop to the same path".into()));
            }
            
            match operation {
                DragOp::Copy => {
                    if source.is_dir() {
                        copy_dir(source, target_path)?;
                    } else {
                        fs::copy(source, target_path)?;
                    }
                }
                DragOp::Move => {
                    if source.is_dir() {
                        copy_dir(source, target_path)?;
                        fs::remove_dir_all(source)?;
                    } else {
                        fs::copy(source, target_path)?;
                        fs::remove_file(source)?;
                    }
                }
            }
        }
        Ok(())
    }
    
    /// Whether dragging is in progress
    pub fn is_dragging(&self) -> bool {
        self.source_path.is_some()
    }
    
    /// Get current operation type
    pub fn current_operation(&self) -> Option<DragOp> {
        self.operation
    }
}

/// Recursively copy directory
fn copy_dir(src: &PathBuf, dest: &PathBuf) -> Result<(), DragDropError> {
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

/// Set drag & drop event handler
pub fn init_drag_drop(window: &nwg::Window, list_view: &nwg::ListView) -> Result<()> {
    // Start drag
    nwg::bind_event_handler(&list_view.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnDragStart {
            // Process at drag start
            let selected = list_view.selected_items();
            if !selected.is_empty() {
                let path = PathBuf::from(selected[0].text());
                let operation = if nwg::is_key_pressed(nwg::VirtualKey::Shift) {
                    DragOp::Move
                } else {
                    DragOp::Copy
                };
                
                // Start drag
                let mut drag_drop = DragDrop::new();
                drag_drop.start_drag(path, operation);
            }
        }
    })?;
    
    // While dragging
    nwg::bind_event_handler(&list_view.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnDragOver {
            // Change cursor while dragging
            nwg::set_cursor(nwg::Cursor::Drag);
        }
    })?;
    
    // Drop
    nwg::bind_event_handler(&list_view.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnDrop {
            // Process at drop
            let target = list_view.hit_test(nwg::get_cursor_pos()).unwrap();
            let target_path = PathBuf::from(target.text());
            
            if let Err(e) = drag_drop.handle_drop(&target_path) {
                nwg::modal_info_message(window, "Error", &format!("Failed to handle drop: {}", e));
            }
            
            // End drag
            drag_drop.end_drag();
        }
    })?;
    
    Ok(())
} 