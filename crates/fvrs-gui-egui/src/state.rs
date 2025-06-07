use std::collections::VecDeque;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    pub current_path: PathBuf,
    pub navigation_history: VecDeque<PathBuf>,
    pub history_position: usize,
    pub search_query: String,
    pub view_mode: ViewMode,
    pub sort_column: SortColumn,
    pub sort_ascending: bool,
    pub selected_items: Vec<PathBuf>,
    pub clipboard: Option<ClipboardOperation>,
    pub show_hidden: bool,
    pub sidebar_width: f32,
    pub show_delete_dialog: bool,
    pub delete_dialog_items: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ViewMode {
    List,
    Grid,
    Details,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SortColumn {
    Name,
    Size,
    Modified,
    Type,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClipboardOperation {
    Cut(Vec<PathBuf>),
    Copy(Vec<PathBuf>),
}

#[derive(Debug, Clone)]
pub enum DragState {
    None,
    Dragging { items: Vec<PathBuf>, start_pos: egui::Pos2 },
}

#[derive(Debug, Clone)]
pub enum FileOperation {
    Move { from: PathBuf, to: PathBuf },
    Copy { from: PathBuf, to: PathBuf },
    Delete { path: PathBuf },
    Rename { from: PathBuf, to: PathBuf },
    CreateFolder { path: PathBuf },
}

impl Default for AppState {
    fn default() -> Self {
        // より安全なデフォルトパス選択
        let default_path = std::env::current_dir()
            .or_else(|_| std::env::home_dir().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Home directory not found")))
            .unwrap_or_else(|_| {
                // Windows環境でのフォールバック
                if cfg!(windows) {
                    PathBuf::from("C:\\Users")
                } else {
                    PathBuf::from("/")
                }
            });
            
        Self {
            current_path: default_path,
            navigation_history: VecDeque::with_capacity(100),
            history_position: 0,
            search_query: String::new(),
            view_mode: ViewMode::Details,
            sort_column: SortColumn::Name,
            sort_ascending: true,
            selected_items: Vec::new(),
            clipboard: None,
            show_hidden: false,
            sidebar_width: 250.0,
            show_delete_dialog: false,
            delete_dialog_items: Vec::new(),
        }
    }
} 