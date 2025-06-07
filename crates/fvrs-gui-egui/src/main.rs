//! FVRS GUI - 次世代ファイルビューア
//! 
//! 最新技術とedition2024を使用した高度なファイル管理システム

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::path::{Path, PathBuf};
use std::ffi::OsStr;
use std::time::SystemTime;

use eframe::egui::{self, *};
use egui_extras::{TableBuilder, Column};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use tokio::runtime::Runtime;

use fvrs_core::core::{FileSystem, FileEntry};

// アプリケーション状態の管理
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewMode {
    List,
    Grid,
    Details,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

// メインアプリケーション構造体
pub struct FileVisorApp {
    state: AppState,
    file_system: Arc<Mutex<FileSystem>>,
    runtime: Arc<Runtime>,
    
    // キャッシュとパフォーマンス
    directory_cache: HashMap<PathBuf, Vec<FileEntry>>,
    _thumbnail_cache: HashMap<PathBuf, Vec<u8>>,
    
    // UI状態
    address_bar_text: String,
    _search_active: bool,
    _context_menu_pos: Option<Pos2>,
    _drag_state: DragState,
    
    // 高度な機能
    _file_watcher: Option<tokio::sync::mpsc::Receiver<PathBuf>>,
    _undo_stack: Vec<FileOperation>,
    _redo_stack: Vec<FileOperation>,
    
    // パフォーマンス監視
    frame_time_history: VecDeque<f32>,
    _memory_usage: usize,
}

#[derive(Debug, Clone)]
pub enum DragState {
    None,
    Dragging { items: Vec<PathBuf>, start_pos: Pos2 },
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
        Self {
            current_path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("C:\\")),
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
        }
    }
}

impl FileVisorApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // ログ設定（既に初期化されている場合はスキップ）
        let _ = tracing_subscriber::fmt::try_init();
        
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

        let app = Self {
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
        };

        app
    }

    // ディレクトリ読み込み（キャッシュ付き）
    fn load_directory(&mut self, path: &Path) -> Result<&Vec<FileEntry>, String> {
        if !self.directory_cache.contains_key(path) {
            let fs = self.file_system.lock().unwrap();
            // FileSystemは非同期APIなので、ランタイムを使用
            match self.runtime.block_on(fs.list_files(Some(path.to_path_buf()))) {
                Ok(entries) => {
                    let mut filtered_entries: Vec<FileEntry> = entries
                        .into_iter()
                        .filter(|entry| {
                            self.state.show_hidden || 
                            !entry.name.starts_with('.')
                        })
                        .collect();

                    // ソート適用
                    self.sort_entries(&mut filtered_entries);
                    self.directory_cache.insert(path.to_path_buf(), filtered_entries);
                }
                Err(e) => return Err(format!("ディレクトリ読み込みエラー: {:?}", e)),
            }
        }
        
        Ok(self.directory_cache.get(path).unwrap())
    }

    // ファイルソート
    fn sort_entries(&self, entries: &mut Vec<FileEntry>) {
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

    // ディレクトリナビゲーション
    fn navigate_to(&mut self, path: PathBuf) {
        // 現在のパスを履歴に追加
        if !self.state.navigation_history.is_empty() && 
           self.state.history_position < self.state.navigation_history.len() - 1 {
            // 履歴の途中にいる場合、それ以降を削除
            self.state.navigation_history.truncate(self.state.history_position + 1);
        }
        
        self.state.navigation_history.push_back(self.state.current_path.clone());
        if self.state.navigation_history.len() > 100 {
            self.state.navigation_history.pop_front();
        } else {
            self.state.history_position += 1;
        }

        self.state.current_path = path;
        self.address_bar_text = self.state.current_path.to_string_lossy().to_string();
        self.state.selected_items.clear();
        
        // キャッシュをクリア（メモリ管理）
        if self.directory_cache.len() > 50 {
            self.directory_cache.clear();
        }
    }

    // 戻る操作
    fn go_back(&mut self) {
        if self.state.history_position > 0 {
            self.state.history_position -= 1;
            let path = self.state.navigation_history[self.state.history_position].clone();
            self.state.current_path = path;
            self.address_bar_text = self.state.current_path.to_string_lossy().to_string();
            self.state.selected_items.clear();
        }
    }

    // 進む操作
    fn go_forward(&mut self) {
        if self.state.history_position < self.state.navigation_history.len() - 1 {
            self.state.history_position += 1;
            let path = self.state.navigation_history[self.state.history_position].clone();
            self.state.current_path = path;
            self.address_bar_text = self.state.current_path.to_string_lossy().to_string();
            self.state.selected_items.clear();
        }
    }

    // 上位ディレクトリへ
    fn go_up(&mut self) {
        if let Some(parent) = self.state.current_path.parent() {
            self.navigate_to(parent.to_path_buf());
        }
    }

    // ファイル削除
    fn delete_selected_files(&mut self) {
        for path in &self.state.selected_items {
            let fs = self.file_system.lock().unwrap();
            if let Err(e) = self.runtime.block_on(fs.remove(path)) {
                tracing::error!("ファイル削除エラー: {:?} at {:?}", e, path);
            } else {
                // Undo操作のために記録
                self._undo_stack.push(FileOperation::Delete { path: path.clone() });
            }
        }
        self.directory_cache.remove(&self.state.current_path);
        self.state.selected_items.clear();
    }

    // フォルダ作成
    fn create_new_folder(&mut self, name: &str) {
        let new_path = self.state.current_path.join(name);
        let fs = self.file_system.lock().unwrap();
        if let Err(e) = self.runtime.block_on(fs.create_dir(&new_path)) {
            tracing::error!("フォルダ作成エラー: {:?}", e);
        } else {
            self._undo_stack.push(FileOperation::CreateFolder { path: new_path });
            self.directory_cache.remove(&self.state.current_path);
        }
    }

    // ファイル名取得でエラーハンドリング
    fn get_display_name(path: &Path) -> String {
        path.file_name()
            .and_then(|os_str| os_str.to_str())
            .unwrap_or("?")
            .to_string()
    }

    // ファイルサイズのフォーマット
    fn format_file_size(size: u64) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
        let mut size = size as f64;
        let mut unit_index = 0;

        while size >= 1024.0 && unit_index < UNITS.len() - 1 {
            size /= 1024.0;
            unit_index += 1;
        }

        if unit_index == 0 {
            format!("{} {}", size as u64, UNITS[unit_index])
        } else {
            format!("{:.1} {}", size, UNITS[unit_index])
        }
    }

    // 時刻フォーマット
    fn format_time(time: DateTime<Local>) -> String {
        time.format("%Y/%m/%d %H:%M").to_string()
    }
}

impl eframe::App for FileVisorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // パフォーマンス監視
        let frame_start = std::time::Instant::now();

        // キーボードショートカット
        ctx.input(|i| {
            if i.key_pressed(egui::Key::F5) {
                self.directory_cache.remove(&self.state.current_path);
            }
            if i.modifiers.ctrl && i.key_pressed(egui::Key::N) {
                // 新しいフォルダー
            }
            if i.key_pressed(egui::Key::Delete) && !self.state.selected_items.is_empty() {
                self.delete_selected_files();
            }
            if i.modifiers.alt && i.key_pressed(egui::Key::ArrowLeft) {
                self.go_back();
            }
            if i.modifiers.alt && i.key_pressed(egui::Key::ArrowRight) {
                self.go_forward();
            }
        });

        // メインUI
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("ファイル", |ui| {
                    if ui.button("新しいフォルダー").clicked() {
                        self.create_new_folder("新しいフォルダー");
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("削除").clicked() {
                        self.delete_selected_files();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("終了").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("表示", |ui| {
                    ui.radio_value(&mut self.state.view_mode, ViewMode::List, "リスト");
                    ui.radio_value(&mut self.state.view_mode, ViewMode::Grid, "グリッド");
                    ui.radio_value(&mut self.state.view_mode, ViewMode::Details, "詳細");
                    ui.separator();
                    ui.checkbox(&mut self.state.show_hidden, "隠しファイルを表示");
                });

                ui.menu_button("ツール", |ui| {
                    if ui.button("設定").clicked() {
                        ui.close_menu();
                    }
                    if ui.button("パフォーマンス情報").clicked() {
                        ui.close_menu();
                    }
                });
            });
        });

        // ツールバー
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // ナビゲーションボタン
                let back_enabled = self.state.history_position > 0;
                let forward_enabled = self.state.history_position < self.state.navigation_history.len() - 1;
                
                if ui.add_enabled(back_enabled, egui::Button::new("←")).clicked() {
                    self.go_back();
                }
                if ui.add_enabled(forward_enabled, egui::Button::new("→")).clicked() {
                    self.go_forward();
                }
                if ui.button("↑").clicked() {
                    self.go_up();
                }
                if ui.button("🔄").clicked() {
                    self.directory_cache.remove(&self.state.current_path);
                }

                ui.separator();

                // アドレスバー
                ui.label("パス:");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.address_bar_text)
                        .desired_width(300.0)
                );
                
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    let path = PathBuf::from(&self.address_bar_text);
                    if path.exists() {
                        self.navigate_to(path);
                    }
                }

                if ui.button("移動").clicked() {
                    let path = PathBuf::from(&self.address_bar_text);
                    if path.exists() {
                        self.navigate_to(path);
                    }
                }

                ui.separator();

                // 検索バー
                ui.label("検索:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.state.search_query)
                        .desired_width(200.0)
                        .hint_text("ファイル名で検索...")
                );
            });
        });

        // サイドパネル（フォルダーツリー）
        egui::SidePanel::left("folder_tree")
            .default_width(self.state.sidebar_width)
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading("フォルダー");
                ui.separator();
                
                egui::ScrollArea::vertical().show(ui, |ui| {
                    // システムドライブ一覧
                    for drive in ["C:", "D:", "E:", "F:"].iter() {
                        let drive_path = PathBuf::from(format!("{}\\", drive));
                        if drive_path.exists() {
                            if ui.selectable_label(
                                self.state.current_path.starts_with(&drive_path),
                                format!("💾 {}", drive)
                            ).clicked() {
                                self.navigate_to(drive_path);
                            }
                        }
                    }
                    
                    ui.separator();
                    
                    // 現在パスのフォルダー階層
                    let mut current = self.state.current_path.clone();
                    let mut parts = Vec::new();
                    
                    while let Some(parent) = current.parent() {
                        if let Some(name) = current.file_name() {
                            parts.push((current.clone(), name.to_string_lossy().to_string()));
                        }
                        current = parent.to_path_buf();
                    }
                    
                    parts.reverse();
                    
                    for (path, name) in parts {
                        let indent = path.components().count() as f32 * 10.0;
                        ui.horizontal(|ui| {
                            ui.add_space(indent);
                            if ui.selectable_label(path == self.state.current_path, format!("📁 {}", name)).clicked() {
                                self.navigate_to(path);
                            }
                        });
                    }
                });
            });

        // メイン表示エリア
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.load_directory(&self.state.current_path.clone()) {
                Ok(entries) => {
                    let filtered_entries: Vec<&FileEntry> = entries
                        .iter()
                        .filter(|entry| {
                            self.state.search_query.is_empty() ||
                            entry.name.to_lowercase().contains(&self.state.search_query.to_lowercase())
                        })
                        .collect();

                    self.show_file_list(ui, &filtered_entries);
                }
                Err(error_msg) => {
                    ui.vertical_centered(|ui| {
                        ui.add_space(50.0);
                        ui.colored_label(egui::Color32::RED, "❌ エラー");
                        ui.label(error_msg);
                        if ui.button("再試行").clicked() {
                            self.directory_cache.remove(&self.state.current_path);
                        }
                    });
                }
            }
        });

        // ステータスバー
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("📁 {}", self.state.current_path.display()));
                ui.separator();
                
                if let Ok(entries) = self.load_directory(&self.state.current_path.clone()) {
                    let dirs = entries.iter().filter(|e| e.is_dir).count();
                    let files = entries.len() - dirs;
                    ui.label(format!("📁 {} フォルダー, 📄 {} ファイル", dirs, files));
                }
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(selected_count) = (!self.state.selected_items.is_empty()).then_some(self.state.selected_items.len()) {
                        ui.label(format!("🔹 {} 個選択", selected_count));
                        ui.separator();
                    }
                    
                    // パフォーマンス情報
                    if !self.frame_time_history.is_empty() {
                        let avg_frame_time = self.frame_time_history.iter().sum::<f32>() / self.frame_time_history.len() as f32;
                        ui.label(format!("FPS: {:.1}", 1000.0 / avg_frame_time));
                    }
                });
            });
        });

        // フレーム時間記録
        let frame_time = frame_start.elapsed().as_millis() as f32;
        self.frame_time_history.push_back(frame_time);
        if self.frame_time_history.len() > 60 {
            self.frame_time_history.pop_front();
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Ok(state_json) = serde_json::to_string(&self.state) {
            storage.set_string("app_state", state_json);
        }
    }
}

impl FileVisorApp {
    // ファイルリスト表示
    fn show_file_list(&mut self, ui: &mut egui::Ui, entries: &[&FileEntry]) {
        match self.state.view_mode {
            ViewMode::Details => self.show_details_view(ui, entries),
            ViewMode::List => self.show_list_view(ui, entries),
            ViewMode::Grid => self.show_grid_view(ui, entries),
        }
    }

    // 詳細ビュー
    fn show_details_view(&mut self, ui: &mut egui::Ui, entries: &[&FileEntry]) {
        let table = TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::auto().at_least(40.0)) // アイコン
            .column(Column::remainder().at_least(200.0)) // 名前
            .column(Column::auto().at_least(80.0)) // サイズ
            .column(Column::auto().at_least(120.0)) // 更新日時
            .column(Column::auto().at_least(80.0)); // 種類

        table
            .header(20.0, |mut header| {
                header.col(|ui| {
                    ui.strong("　");
                });
                header.col(|ui| {
                    if ui.button(if self.state.sort_column == SortColumn::Name {
                        if self.state.sort_ascending { "名前 ▲" } else { "名前 ▼" }
                    } else { "名前" }).clicked() {
                        if self.state.sort_column == SortColumn::Name {
                            self.state.sort_ascending = !self.state.sort_ascending;
                        } else {
                            self.state.sort_column = SortColumn::Name;
                            self.state.sort_ascending = true;
                        }
                        self.directory_cache.remove(&self.state.current_path);
                    }
                });
                header.col(|ui| {
                    if ui.button(if self.state.sort_column == SortColumn::Size {
                        if self.state.sort_ascending { "サイズ ▲" } else { "サイズ ▼" }
                    } else { "サイズ" }).clicked() {
                        if self.state.sort_column == SortColumn::Size {
                            self.state.sort_ascending = !self.state.sort_ascending;
                        } else {
                            self.state.sort_column = SortColumn::Size;
                            self.state.sort_ascending = true;
                        }
                        self.directory_cache.remove(&self.state.current_path);
                    }
                });
                header.col(|ui| {
                    if ui.button(if self.state.sort_column == SortColumn::Modified {
                        if self.state.sort_ascending { "更新日時 ▲" } else { "更新日時 ▼" }
                    } else { "更新日時" }).clicked() {
                        if self.state.sort_column == SortColumn::Modified {
                            self.state.sort_ascending = !self.state.sort_ascending;
                        } else {
                            self.state.sort_column = SortColumn::Modified;
                            self.state.sort_ascending = true;
                        }
                        self.directory_cache.remove(&self.state.current_path);
                    }
                });
                header.col(|ui| {
                    if ui.button(if self.state.sort_column == SortColumn::Type {
                        if self.state.sort_ascending { "種類 ▲" } else { "種類 ▼" }
                    } else { "種類" }).clicked() {
                        if self.state.sort_column == SortColumn::Type {
                            self.state.sort_ascending = !self.state.sort_ascending;
                        } else {
                            self.state.sort_column = SortColumn::Type;
                            self.state.sort_ascending = true;
                        }
                        self.directory_cache.remove(&self.state.current_path);
                    }
                });
            })
            .body(|body| {
                body.rows(20.0, entries.len(), |mut row| {
                    let row_index = row.index();
                    let entry = entries[row_index];
                    let entry_path = self.state.current_path.join(&entry.name);
                    let is_selected = self.state.selected_items.contains(&entry_path);

                    let response = row.col(|ui| {
                        ui.label(if entry.is_dir { "📁" } else { "📄" });
                    });

                    row.col(|ui| {
                        let name_response = ui.selectable_label(is_selected, &entry.name);
                        if name_response.double_clicked() {
                            if entry.is_dir {
                                self.navigate_to(entry_path.clone());
                            } else {
                                // ファイルを開く
                                if let Err(e) = open::that(&entry_path) {
                                    tracing::error!("ファイルオープンエラー: {:?}", e);
                                }
                            }
                        }
                        if name_response.clicked() {
                            if ui.input(|i| i.modifiers.ctrl) {
                                // Ctrl+クリック: 複数選択
                                if is_selected {
                                    self.state.selected_items.retain(|p| p != &entry_path);
                                } else {
                                    self.state.selected_items.push(entry_path.clone());
                                }
                            } else {
                                // 通常クリック: 単一選択
                                self.state.selected_items.clear();
                                self.state.selected_items.push(entry_path.clone());
                            }
                        }
                    });

                    row.col(|ui| {
                        if entry.is_dir {
                            ui.label("―");
                        } else {
                            ui.label(Self::format_file_size(entry.size));
                        }
                    });

                    row.col(|ui| {
                        ui.label(Self::format_time(entry.modified));
                    });

                    row.col(|ui| {
                        if entry.is_dir {
                            ui.label("フォルダー");
                        } else {
                            let ext = Path::new(&entry.name)
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("");
                            ui.label(if ext.is_empty() { "ファイル" } else { ext });
                        }
                    });
                });
            });
    }

    // リストビュー
    fn show_list_view(&mut self, ui: &mut egui::Ui, entries: &[&FileEntry]) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            for entry in entries {
                let entry_path = self.state.current_path.join(&entry.name);
                let is_selected = self.state.selected_items.contains(&entry_path);

                ui.horizontal(|ui| {
                    ui.label(if entry.is_dir { "📁" } else { "📄" });
                    
                    let response = ui.selectable_label(is_selected, &entry.name);
                    
                    if response.double_clicked() {
                        if entry.is_dir {
                            self.navigate_to(entry_path.clone());
                        } else {
                            if let Err(e) = open::that(&entry_path) {
                                tracing::error!("ファイルオープンエラー: {:?}", e);
                            }
                        }
                    }
                    
                    if response.clicked() {
                        if ui.input(|i| i.modifiers.ctrl) {
                            if is_selected {
                                self.state.selected_items.retain(|p| p != &entry_path);
                            } else {
                                self.state.selected_items.push(entry_path.clone());
                            }
                        } else {
                            self.state.selected_items.clear();
                            self.state.selected_items.push(entry_path.clone());
                        }
                    }
                });
            }
        });
    }

    // グリッドビュー
    fn show_grid_view(&mut self, ui: &mut egui::Ui, entries: &[&FileEntry]) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(10.0, 10.0);
            
            let available_width = ui.available_width();
            let item_width = 120.0;
            let items_per_row = (available_width / item_width).floor() as usize;
            
            for chunk in entries.chunks(items_per_row.max(1)) {
                ui.horizontal(|ui| {
                    for entry in chunk {
                        let entry_path = self.state.current_path.join(&entry.name);
                        let is_selected = self.state.selected_items.contains(&entry_path);

                        ui.group(|ui| {
                            ui.set_width(item_width);
                            ui.vertical_centered(|ui| {
                                ui.add_space(5.0);
                                
                                // アイコン（大きく表示）
                                ui.label(egui::RichText::new(
                                    if entry.is_dir { "📁" } else { "📄" }
                                ).size(32.0));
                                
                                ui.add_space(5.0);
                                
                                // ファイル名（選択可能）
                                let response = ui.selectable_label(is_selected, &entry.name);
                                
                                if response.double_clicked() {
                                    if entry.is_dir {
                                        self.navigate_to(entry_path.clone());
                                    } else {
                                        if let Err(e) = open::that(&entry_path) {
                                            tracing::error!("ファイルオープンエラー: {:?}", e);
                                        }
                                    }
                                }
                                
                                if response.clicked() {
                                    if ui.input(|i| i.modifiers.ctrl) {
                                        if is_selected {
                                            self.state.selected_items.retain(|p| p != &entry_path);
                                        } else {
                                            self.state.selected_items.push(entry_path.clone());
                                        }
                                    } else {
                                        self.state.selected_items.clear();
                                        self.state.selected_items.push(entry_path.clone());
                                    }
                                }
                                
                                ui.add_space(5.0);
                            });
                        });
                    }
                });
            }
        });
    }
}

// アプリケーション起動
fn main() -> Result<(), eframe::Error> {
    // ログ初期化
    tracing_subscriber::fmt::init();
    
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_icon(
                eframe::icon_data::from_png_bytes(&include_bytes!("../assets/icon.png")[..])
                    .unwrap_or_default()
            ),
        ..Default::default()
    };

    eframe::run_native(
        "FVRS - 次世代ファイルビューア",
        options,
        Box::new(|cc| {
            // より良いビジュアル設定
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            
            Ok(Box::new(FileVisorApp::new(cc)))
        }),
    )
} 