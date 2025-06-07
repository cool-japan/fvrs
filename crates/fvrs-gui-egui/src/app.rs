use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;
use fvrs_core::core::{FileEntry, FileSystem};
use crate::state::{AppState, DragState, FileOperation, SortColumn};
use crate::utils::setup_japanese_fonts;

pub struct FileVisorApp {
    pub state: AppState,
    pub file_system: Arc<Mutex<FileSystem>>,
    pub runtime: Arc<Runtime>,
    
    // キャッシュとパフォーマンス
    pub directory_cache: HashMap<PathBuf, Vec<FileEntry>>,
    pub _thumbnail_cache: HashMap<PathBuf, Vec<u8>>,
    
    // UI状態
    pub address_bar_text: String,
    pub _search_active: bool,
    pub _context_menu_pos: Option<egui::Pos2>,
    pub _drag_state: DragState,
    
    // 高度な機能
    pub _file_watcher: Option<tokio::sync::mpsc::Receiver<PathBuf>>,
    pub _undo_stack: Vec<FileOperation>,
    pub _redo_stack: Vec<FileOperation>,
    
    // パフォーマンス監視
    pub frame_time_history: VecDeque<f32>,
    pub _memory_usage: usize,
}

impl FileVisorApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // ログ設定（既に初期化されている場合はスキップ）
        let _ = tracing_subscriber::fmt::try_init();
        
        // 日本語フォント設定
        setup_japanese_fonts(&cc.egui_ctx);
        
        // 状態復元の試行
        let state = if let Some(storage) = cc.storage {
            storage.get_string("app_state")
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            AppState::default()
        };

        let runtime = Arc::new(
            tokio::runtime::Runtime::new()
                .expect("tokio runtimeの作成に失敗")
        );

        Self {
            address_bar_text: state.current_path.to_string_lossy().to_string(),
            state,
            file_system: Arc::new(Mutex::new(FileSystem::new())),
            runtime,
            directory_cache: HashMap::new(),
            _thumbnail_cache: HashMap::new(),
            _search_active: false,
            _context_menu_pos: None,
            _drag_state: DragState::None,
            _file_watcher: None,
            _undo_stack: Vec::new(),
            _redo_stack: Vec::new(),
            frame_time_history: VecDeque::with_capacity(60),
            _memory_usage: 0,
        }
    }

    /// ディレクトリ読み込み（キャッシュ付き）- Windows対応改善版
    pub fn load_directory(&mut self, path: &Path) -> Result<&Vec<FileEntry>, String> {
        if !self.directory_cache.contains_key(path) {
            // まずパスの存在確認
            if !path.exists() {
                return Err(format!("パスが存在しません: {}", path.display()));
            }
            
            if !path.is_dir() {
                return Err(format!("ディレクトリではありません: {}", path.display()));
            }

            // 標準ライブラリを使用してより安全にディレクトリを読み込み
            match std::fs::read_dir(path) {
                Ok(entries) => {
                    let mut file_entries = Vec::new();
                    
                    for entry_result in entries {
                        match entry_result {
                            Ok(entry) => {
                                let path = entry.path();
                                let name = entry.file_name().to_string_lossy().to_string();
                                
                                if let Ok(metadata) = entry.metadata() {
                                    let file_entry = FileEntry {
                                        name,
                                        path: path.clone(),
                                        size: metadata.len(),
                                        is_dir: metadata.is_dir(),
                                        created: metadata.created()
                                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                                            .into(),
                                        modified: metadata.modified()
                                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                                            .into(),
                                        extension: path.extension()
                                            .and_then(|ext| ext.to_str())
                                            .map(|s| s.to_string()),
                                    };
                                    
                                    // 隠しファイルのフィルタリング
                                    if self.state.show_hidden || !file_entry.name.starts_with('.') {
                                        file_entries.push(file_entry);
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!("エントリ読み込みエラー: {:?}", e);
                                continue;
                            }
                        }
                    }

                    // ソート適用
                    self.sort_entries(&mut file_entries);
                    self.directory_cache.insert(path.to_path_buf(), file_entries);
                }
                Err(e) => {
                    return Err(format!("ディレクトリアクセスエラー: {} ({})", e, path.display()));
                }
            }
        }
        
        Ok(self.directory_cache.get(path).unwrap())
    }

    /// ファイルソート
    pub fn sort_entries(&self, entries: &mut Vec<FileEntry>) {
        entries.sort_by(|a, b| {
            // ディレクトリを最初に
            if a.is_dir && !b.is_dir {
                return std::cmp::Ordering::Less;
            }
            if !a.is_dir && b.is_dir {
                return std::cmp::Ordering::Greater;
            }

            let ordering = match self.state.sort_column {
                SortColumn::Name => a.name.cmp(&b.name),
                SortColumn::Size => a.size.cmp(&b.size),
                SortColumn::Modified => a.modified.cmp(&b.modified),
                SortColumn::Type => {
                    let ext_a = a.extension.as_deref().unwrap_or("");
                    let ext_b = b.extension.as_deref().unwrap_or("");
                    ext_a.cmp(ext_b)
                }
            };

            if self.state.sort_ascending {
                ordering
            } else {
                ordering.reverse()
            }
        });
    }

    /// ナビゲーション
    pub fn navigate_to(&mut self, path: PathBuf) {
        if path.exists() && path.is_dir() {
            // 履歴管理
            if self.state.history_position < self.state.navigation_history.len() {
                self.state.navigation_history.truncate(self.state.history_position + 1);
            }
            
            self.state.navigation_history.push_back(path.clone());
            self.state.history_position = self.state.navigation_history.len().saturating_sub(1);
            
            // 履歴サイズ制限
            if self.state.navigation_history.len() > 100 {
                self.state.navigation_history.pop_front();
                self.state.history_position = self.state.history_position.saturating_sub(1);
            }

            self.state.current_path = path;
            self.address_bar_text = self.state.current_path.to_string_lossy().to_string();
            self.state.selected_items.clear();
            self.state.last_selected_index = None;
        }
    }

    pub fn go_back(&mut self) {
        if self.state.history_position > 0 {
            self.state.history_position -= 1;
            if let Some(path) = self.state.navigation_history.get(self.state.history_position) {
                self.state.current_path = path.clone();
                self.address_bar_text = self.state.current_path.to_string_lossy().to_string();
                self.state.selected_items.clear();
                self.state.last_selected_index = None;
            }
        }
    }

    pub fn go_forward(&mut self) {
        if self.state.history_position < self.state.navigation_history.len().saturating_sub(1) {
            self.state.history_position += 1;
            if let Some(path) = self.state.navigation_history.get(self.state.history_position) {
                self.state.current_path = path.clone();
                self.address_bar_text = self.state.current_path.to_string_lossy().to_string();
                self.state.selected_items.clear();
                self.state.last_selected_index = None;
            }
        }
    }

    pub fn go_up(&mut self) {
        if let Some(parent) = self.state.current_path.parent() {
            self.navigate_to(parent.to_path_buf());
        }
    }

    /// 削除確認ダイアログを表示
    pub fn show_delete_confirmation(&mut self) {
        if !self.state.selected_items.is_empty() {
            self.state.delete_dialog_items = self.state.selected_items.clone();
            self.state.show_delete_dialog = true;
        }
    }

    /// ファイル削除（実際の削除処理）
    pub fn delete_selected_files(&mut self) {
        for path in &self.state.delete_dialog_items {
            // 標準ライブラリを使用してより確実に削除
            if path.is_dir() {
                match std::fs::remove_dir_all(path) {
                    Ok(_) => {
                        tracing::info!("フォルダを削除しました: {:?}", path);
                        self._undo_stack.push(FileOperation::Delete { path: path.clone() });
                    }
                    Err(e) => tracing::error!("フォルダ削除エラー: {:?} at {:?}", e, path),
                }
            } else {
                match std::fs::remove_file(path) {
                    Ok(_) => {
                        tracing::info!("ファイルを削除しました: {:?}", path);
                        self._undo_stack.push(FileOperation::Delete { path: path.clone() });
                    }
                    Err(e) => tracing::error!("ファイル削除エラー: {:?} at {:?}", e, path),
                }
            }
        }
        
        // 状態をクリア
        self.directory_cache.remove(&self.state.current_path);
        self.state.selected_items.clear();
        self.state.last_selected_index = None;
        self.state.delete_dialog_items.clear();
        self.state.show_delete_dialog = false;
    }

    /// フォルダ作成
    pub fn create_new_folder(&mut self, name: &str) {
        let new_path = self.state.current_path.join(name);
        let fs = self.file_system.lock().unwrap();
        if let Err(e) = self.runtime.block_on(fs.create_dir(&new_path)) {
            tracing::error!("フォルダ作成エラー: {:?}", e);
        } else {
            self._undo_stack.push(FileOperation::CreateFolder { path: new_path });
            self.directory_cache.remove(&self.state.current_path);
        }
    }
} 