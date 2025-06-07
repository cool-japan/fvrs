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

/// ドラッグ&ドロップの操作タイプ
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DragOp {
    Copy,
    Move,
}

/// ドラッグ&ドロップの状態を管理する構造体
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
    
    /// ドラッグ開始
    pub fn start_drag(&mut self, path: PathBuf, operation: DragOp) {
        self.source_path = Some(path);
        self.operation = Some(operation);
    }
    
    /// ドラッグ終了
    pub fn end_drag(&mut self) {
        self.source_path = None;
        self.operation = None;
    }
    
    /// ドロップ処理
    pub fn handle_drop(&self, target_path: &PathBuf) -> Result<(), DragDropError> {
        if let (Some(source), Some(operation)) = (&self.source_path, self.operation) {
            if source == target_path {
                return Err(DragDropError::InvalidOperation("同じパスへのドロップはできません".into()));
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
    
    /// ドラッグ中かどうか
    pub fn is_dragging(&self) -> bool {
        self.source_path.is_some()
    }
    
    /// 現在の操作タイプを取得
    pub fn current_operation(&self) -> Option<DragOp> {
        self.operation
    }
}

/// ディレクトリを再帰的にコピー
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

/// ドラッグ&ドロップのイベントハンドラを設定
pub fn init_drag_drop(window: &nwg::Window, list_view: &nwg::ListView) -> Result<()> {
    // ドラッグ開始
    nwg::bind_event_handler(&list_view.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnDragStart {
            // ドラッグ開始時の処理
            let selected = list_view.selected_items();
            if !selected.is_empty() {
                let path = PathBuf::from(selected[0].text());
                let operation = if nwg::is_key_pressed(nwg::VirtualKey::Shift) {
                    DragOp::Move
                } else {
                    DragOp::Copy
                };
                
                // ドラッグ開始
                let mut drag_drop = DragDrop::new();
                drag_drop.start_drag(path, operation);
            }
        }
    })?;
    
    // ドラッグ中
    nwg::bind_event_handler(&list_view.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnDragOver {
            // ドラッグ中はカーソルを変更
            nwg::set_cursor(nwg::Cursor::Drag);
        }
    })?;
    
    // ドロップ
    nwg::bind_event_handler(&list_view.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnDrop {
            // ドロップ時の処理
            let target = list_view.hit_test(nwg::get_cursor_pos()).unwrap();
            let target_path = PathBuf::from(target.text());
            
            if let Err(e) = drag_drop.handle_drop(&target_path) {
                nwg::modal_info_message(window, "Error", &format!("Failed to handle drop: {}", e));
            }
            
            // ドラッグ終了
            drag_drop.end_drag();
        }
    })?;
    
    Ok(())
} 