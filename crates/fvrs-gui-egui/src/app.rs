use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;
use fvrs_core::core::{FileEntry, FileSystem};
use crate::state::{AppState, DragState, FileOperation, SortColumn};
use crate::utils::setup_japanese_fonts;
use crate::archive::{ArchiveHandler, ArchiveType};




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

            // 高速なディレクトリ読み込み（直接実装）
            const MAX_ENTRIES: usize = 1000;
            
            match std::fs::read_dir(path) {
                Ok(entries) => {
                    let mut file_entries = Vec::new();
                    
                    // 親ディレクトリエントリを追加
                    if path.parent().is_some() {
                        file_entries.push(FileEntry {
                            name: "..".to_string(),
                            path: path.to_path_buf(),
                            size: 0,
                            is_dir: true,
                            created: chrono::Local::now(),
                            modified: chrono::Local::now(),
                            extension: None,
                        });
                    }
                    
                    // エントリを効率的に処理
                    let dir_entries: Vec<_> = entries
                        .filter_map(|entry| entry.ok())
                        .take(MAX_ENTRIES)
                        .filter(|entry| {
                            let file_name = entry.file_name();
                            let name = file_name.to_string_lossy();
                            self.state.show_hidden || !name.starts_with('.')
                        })
                        .collect();
                    
                    for entry in dir_entries {
                        let path = entry.path();
                        let name = entry.file_name().to_string_lossy().to_string();
                        
                        if let Ok(metadata) = entry.metadata() {
                            let size = if metadata.is_file() { metadata.len() } else { 0 };
                            let created = metadata.created()
                                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                                .into();
                            let modified = metadata.modified()
                                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                                .into();
                            
                            file_entries.push(FileEntry {
                                name,
                                path: path.clone(),
                                size,
                                is_dir: metadata.is_dir(),
                                created,
                                modified,
                                extension: path.extension()
                                    .and_then(|ext| ext.to_str())
                                    .map(|s| s.to_string()),
                            });
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
    
    /// 新規ファイル作成
    pub fn create_new_file(&mut self, file_name: &str) {
        let new_file_path = self.state.current_path.join(file_name);
        
        // ファイルが既に存在するかチェック
        if new_file_path.exists() {
            tracing::error!("ファイルが既に存在します: {:?}", new_file_path);
            return;
        }
        
        // 標準ライブラリを使用してファイル作成
        match std::fs::write(&new_file_path, "") {
            Ok(_) => {
                tracing::info!("新規ファイルを作成しました: {:?}", new_file_path);
                // ディレクトリキャッシュを更新
                self.directory_cache.remove(&self.state.current_path);
                
                // 作成したファイルを選択状態にする
                self.state.selected_items.clear();
                self.state.selected_items.push(new_file_path.clone());
                self.state.last_selected_index = None;
                
                // ダイアログを閉じる
                self.state.show_create_file_dialog = false;
                self.state.new_file_name.clear();
            }
            Err(e) => {
                tracing::error!("ファイル作成エラー: {:?}", e);
            }
        }
    }
    
    /// 新規フォルダ作成（ダイアログ経由）
    pub fn create_new_folder_dialog(&mut self, folder_name: &str) {
        let new_folder_path = self.state.current_path.join(folder_name);
        
        // フォルダが既に存在するかチェック
        if new_folder_path.exists() {
            tracing::error!("フォルダが既に存在します: {:?}", new_folder_path);
            return;
        }
        
        // 標準ライブラリを使用してフォルダ作成
        match std::fs::create_dir(&new_folder_path) {
            Ok(_) => {
                tracing::info!("新規フォルダを作成しました: {:?}", new_folder_path);
                // ディレクトリキャッシュを更新
                self.directory_cache.remove(&self.state.current_path);
                
                // 作成したフォルダを選択状態にする
                self.state.selected_items.clear();
                self.state.selected_items.push(new_folder_path.clone());
                self.state.last_selected_index = None;
                
                // ダイアログを閉じる
                self.state.show_create_folder_dialog = false;
                self.state.new_folder_name.clear();
            }
            Err(e) => {
                tracing::error!("フォルダ作成エラー: {:?}", e);
            }
        }
    }

    /// 解凍ダイアログを表示
    pub fn show_unpack_dialog(&mut self) {
        // 選択されたファイルが圧縮ファイルかチェック
        if let Some(selected_path) = self.state.selected_items.first() {
            let full_path = selected_path.clone();
            if ArchiveHandler::is_archive(&full_path) {
                self.state.current_archive = Some(full_path);
                self.state.unpack_destination = self.state.current_path.to_string_lossy().to_string();
                self.state.show_unpack_dialog = true;
            } else {
                // self.state.status_message = "選択されたファイルは圧縮ファイルではありません".to_string();
            }
        } else {
            // self.state.status_message = "解凍するファイルを選択してください".to_string();
        }
    }

    /// 圧縮ダイアログを表示
    pub fn show_pack_dialog(&mut self) {
        if !self.state.selected_items.is_empty() {
            self.state.pack_filename = "archive.zip".to_string();
            self.state.pack_format = ArchiveType::Zip;
            self.state.show_pack_dialog = true;
        } else {
            // self.state.status_message = "圧縮するファイルやフォルダを選択してください".to_string();
        }
    }

    /// 圧縮ファイルビューアを表示
    pub fn show_archive_viewer(&mut self, archive_path: PathBuf) {
        match ArchiveHandler::list_archive_contents(&archive_path) {
            Ok(entries) => {
                self.state.archive_entries = entries;
                self.state.current_archive = Some(archive_path);
                self.state.show_archive_viewer = true;
            }
            Err(e) => {
                // self.state.status_message = format!("圧縮ファイル読み込みエラー: {}", e);
                tracing::error!("圧縮ファイル読み込みエラー: {}", e);
            }
        }
    }

    /// 圧縮ファイルを解凍
    pub fn extract_archive(&mut self) {
        if let Some(archive_path) = &self.state.current_archive.clone() {
            let destination = PathBuf::from(&self.state.unpack_destination);
            
            match ArchiveHandler::extract_archive(archive_path, &destination) {
                Ok(()) => {
                    // self.state.status_message = format!("解凍完了: {}", destination.display());
                    self.state.show_unpack_dialog = false;
                    self.reload_current_directory();
                    tracing::info!("圧縮ファイルを解凍しました: {:?} -> {:?}", archive_path, destination);
                }
                Err(e) => {
                    // self.state.status_message = format!("解凍エラー: {}", e);
                    tracing::error!("解凍エラー: {}", e);
                }
            }
        }
    }

    /// ファイル・フォルダを圧縮
    pub fn create_archive(&mut self) {
        let selected_paths: Vec<PathBuf> = self.state.selected_items.clone();

        if selected_paths.is_empty() {
            // self.state.status_message = "圧縮するファイルやフォルダを選択してください".to_string();
            return;
        }

        let archive_path = self.state.current_path.join(&self.state.pack_filename);
        
        match ArchiveHandler::create_archive(&selected_paths, &archive_path, self.state.pack_format.clone()) {
            Ok(()) => {
                // self.state.status_message = format!("圧縮完了: {}", archive_path.display());
                self.state.show_pack_dialog = false;
                self.reload_current_directory();
                tracing::info!("ファイルを圧縮しました: {:?} -> {:?}", selected_paths, archive_path);
            }
            Err(e) => {
                // self.state.status_message = format!("圧縮エラー: {}", e);
                tracing::error!("圧縮エラー: {}", e);
            }
        }
    }

    /// 圧縮ファイルビューアを閉じる
    pub fn close_archive_viewer(&mut self) {
        self.state.show_archive_viewer = false;
        self.state.archive_entries.clear();
        self.state.current_archive = None;
    }
    
    /// 現在のディレクトリをリロード
    pub fn reload_current_directory(&mut self) {
        self.directory_cache.remove(&self.state.current_path);
        // キャッシュをクリアすることで次回表示時に再読み込みされる
    }
    
    /// リネームダイアログを表示
    pub fn show_rename_dialog(&mut self) {
        if let Some(selected_path) = self.state.selected_items.first() {
            self.state.rename_target_path = Some(selected_path.clone());
            self.state.rename_new_name = selected_path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("")
                .to_string();
            self.state.show_rename_dialog = true;
        }
    }
    
    /// ファイル・フォルダをリネーム
    pub fn rename_item(&mut self) {
        if let Some(old_path) = &self.state.rename_target_path.clone() {
            let new_name = &self.state.rename_new_name;
            if new_name.is_empty() {
                return;
            }
            
            let new_path = old_path.parent()
                .map(|parent| parent.join(new_name))
                .unwrap_or_else(|| PathBuf::from(new_name));
            
            match std::fs::rename(old_path, &new_path) {
                Ok(()) => {
                    tracing::info!("リネーム完了: {:?} -> {:?}", old_path, new_path);
                    
                    // 選択アイテムを更新
                    if let Some(index) = self.state.selected_items.iter().position(|path| path == old_path) {
                        self.state.selected_items[index] = new_path;
                    }
                    
                    self.state.show_rename_dialog = false;
                    self.state.rename_new_name.clear();
                    self.state.rename_target_path = None;
                    self.reload_current_directory();
                }
                Err(e) => {
                    tracing::error!("リネームエラー: {:?}", e);
                }
            }
        }
    }
} 