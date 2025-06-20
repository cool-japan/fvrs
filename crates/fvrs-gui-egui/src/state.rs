use std::collections::VecDeque;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

use crate::archive::{ArchiveEntry, ArchiveType};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ActivePane {
    LeftSidebar,
    MainList,
}

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
    pub last_selected_index: Option<usize>,
    pub clipboard: Option<ClipboardOperation>,
    pub show_hidden: bool,
    pub sidebar_width: f32,
    pub show_delete_dialog: bool,
    pub delete_dialog_items: Vec<PathBuf>,
    pub show_shortcuts_dialog: bool,
    
    // ペイン管理
    pub active_pane: ActivePane,
    pub sidebar_selected_item: Option<PathBuf>,
    pub sidebar_last_selected_index: Option<usize>,
    pub expanded_folders: std::collections::HashSet<PathBuf>,
    
    // ファイル閲覧・編集機能
    pub show_file_viewer: bool,
    pub viewed_file_path: Option<PathBuf>,
    pub viewed_file_content: String,
    pub file_viewer_width: f32,
    pub is_file_modified: bool,
    pub view_mode_text: bool, // true: 編集モード, false: 閲覧モード
    // 未保存変更確認ダイアログ
    pub show_unsaved_dialog: bool,
    pub pending_close_action: bool,
    // エディタオプション
    pub show_line_numbers: bool,
    // 新規ファイル作成ダイアログ
    pub show_create_file_dialog: bool,
    pub new_file_name: String,
    // 新規フォルダ作成ダイアログ
    pub show_create_folder_dialog: bool,
    pub new_folder_name: String,
    
    // リネームダイアログ
    pub show_rename_dialog: bool,
    pub rename_new_name: String,
    pub rename_target_path: Option<PathBuf>,
    
    // 圧縮ファイル関連
    pub show_unpack_dialog: bool,
    pub show_pack_dialog: bool,
    pub show_archive_viewer: bool,
    pub archive_entries: Vec<ArchiveEntry>,
    pub current_archive: Option<PathBuf>,
    pub unpack_destination: String,
    pub pack_filename: String,
    pub pack_format: ArchiveType,
    
    // ファイル情報ダイアログ
    pub show_file_info_dialog: bool,
    pub file_info_target: Option<PathBuf>,
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
            last_selected_index: None,
            clipboard: None,
            show_hidden: false,
            sidebar_width: 250.0,
            show_delete_dialog: false,
            delete_dialog_items: Vec::new(),
            show_shortcuts_dialog: false,
            
            // ペイン管理
            active_pane: ActivePane::MainList,
            sidebar_selected_item: None,
            sidebar_last_selected_index: None,
            expanded_folders: std::collections::HashSet::new(),
            
            // ファイル閲覧・編集機能
            show_file_viewer: false,
            viewed_file_path: None,
            viewed_file_content: String::new(),
            file_viewer_width: 800.0,
            is_file_modified: false,
            view_mode_text: false, // true: 編集モード, false: 閲覧モード
            // 未保存変更確認ダイアログ
            show_unsaved_dialog: false,
            pending_close_action: false,
            // エディタオプション
            show_line_numbers: false,
            // 新規ファイル作成ダイアログ
            show_create_file_dialog: false,
            new_file_name: String::new(),
            // 新規フォルダ作成ダイアログ
            show_create_folder_dialog: false,
            new_folder_name: String::new(),
            
            // リネームダイアログ
            show_rename_dialog: false,
            rename_new_name: String::new(),
            rename_target_path: None,
            
            // 圧縮ファイル関連
            show_unpack_dialog: false,
            show_pack_dialog: false,
            show_archive_viewer: false,
            archive_entries: Vec::new(),
            current_archive: None,
            unpack_destination: String::new(),
            pack_filename: String::new(),
            pack_format: ArchiveType::Zip,
            
            // ファイル情報ダイアログ
            show_file_info_dialog: false,
            file_info_target: None,
        }
    }
} 