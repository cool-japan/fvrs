use std::path::PathBuf;
use egui::{Context, Color32, Stroke, Response};
use crate::app::FileVisorApp;
use crate::state::ActivePane;

pub struct ExplorerTreeUI;

impl ExplorerTreeUI {
    pub fn show_explorer_tree(
        ui: &mut egui::Ui,
        app: &mut FileVisorApp,
        _ctx: &Context,
    ) -> Response {
        let is_active = app.state.active_pane == ActivePane::LeftSidebar;
        
        // ペイン全体のスタイル設定
        let frame = egui::Frame::side_top_panel(&ui.style())
            .stroke(if is_active {
                Stroke::new(2.0, Color32::from_rgb(0, 120, 215)) // 青い枠
            } else {
                Stroke::new(1.0, Color32::GRAY) // グレーの枠
            });
        
        frame.show(ui, |ui| {
            ui.heading("📁 エクスプローラー");
            ui.separator();
            
            egui::ScrollArea::vertical()
                .id_salt("explorer_tree")
                .show(ui, |ui| {
                    Self::show_drives(ui, app);
                    ui.separator();
                    Self::show_directory_tree(ui, app);
                });
        }).response
    }
    
    fn show_drives(ui: &mut egui::Ui, app: &mut FileVisorApp) {
        ui.label("💾 ドライブ");
        
        // Windowsドライブ一覧
        for drive in ["C:", "D:", "E:", "F:", "G:", "H:"].iter() {
            let drive_path = PathBuf::from(format!("{}\\", drive));
            if drive_path.exists() {
                let is_selected = app.state.sidebar_selected_item
                    .as_ref()
                    .map(|p| p.starts_with(&drive_path))
                    .unwrap_or(false);
                
                let is_current = app.state.current_path.starts_with(&drive_path);
                
                let response = ui.selectable_label(
                    is_selected,
                    format!("💾 {}", drive)
                );
                
                if response.clicked() {
                    app.state.active_pane = ActivePane::LeftSidebar;
                    app.state.sidebar_selected_item = Some(drive_path.clone());
                    
                    if response.double_clicked() {
                        app.navigate_to(drive_path);
                    }
                }
                
                // 現在のパスを薄く表示
                if is_current && !is_selected {
                    response.highlight();
                }
            }
        }
        
        // Unixルートディスクトリ
        if cfg!(unix) {
            let root_path = PathBuf::from("/");
            let is_selected = app.state.sidebar_selected_item
                .as_ref()
                .map(|p| *p == root_path)
                .unwrap_or(false);
            
            let response = ui.selectable_label(is_selected, "💾 /");
            
            if response.clicked() {
                app.state.active_pane = ActivePane::LeftSidebar;
                app.state.sidebar_selected_item = Some(root_path.clone());
                
                if response.double_clicked() {
                    app.navigate_to(root_path);
                }
            }
        }
    }
    
    fn show_directory_tree(ui: &mut egui::Ui, app: &mut FileVisorApp) {
        ui.label("📂 フォルダーツリー");
        
        // 現在のパスをクローンして借用問題を回避
        let current_path = app.state.current_path.clone();
        
        // 親ディレクトリがあれば表示
        if let Some(parent) = current_path.parent() {
            Self::show_folder_item(ui, app, &parent.to_path_buf(), 0, false);
        }
        
        // 現在のディレクトリを表示（展開状態）
        Self::show_folder_item(ui, app, &current_path, 0, true);
    }
    
    fn show_folder_item(
        ui: &mut egui::Ui,
        app: &mut FileVisorApp,
        folder_path: &PathBuf,
        depth: usize,
        force_expanded: bool,
    ) {
        let indent = depth as f32 * 15.0;
        
        let has_children = Self::has_subdirectories_cached(folder_path);
        let is_expanded = force_expanded || app.state.expanded_folders.contains(folder_path);
        
        ui.horizontal(|ui| {
            ui.add_space(indent);
            
            // 展開/折りたたみアイコン
            if has_children {
                let expand_icon = if is_expanded { "▼" } else { "▶" };
                if ui.small_button(expand_icon).clicked() {
                    if app.state.expanded_folders.contains(folder_path) {
                        app.state.expanded_folders.remove(folder_path);
                    } else {
                        app.state.expanded_folders.insert(folder_path.clone());
                    }
                }
            } else {
                ui.add_space(20.0);
            }
            
            // フォルダー名
            let folder_name = folder_path
                .file_name()
                .map(|n| n.to_str().unwrap_or("?"))
                .unwrap_or("ルート");
            
            // 選択状態の判定
            let is_selected = app.state.sidebar_selected_item
                .as_ref()
                .map(|p| *p == *folder_path)
                .unwrap_or(false);
            
            let is_current_path = app.state.current_path == *folder_path;
            
            let response = ui.selectable_label(is_selected, format!("📁 {}", folder_name));
            
            if response.clicked() {
                app.state.active_pane = ActivePane::LeftSidebar;
                app.state.sidebar_selected_item = Some(folder_path.clone());
            }
            
            if response.double_clicked() {
                app.navigate_to(folder_path.clone());
            }
            
            // 現在のパスをハイライト
            if is_current_path && !is_selected {
                response.highlight();
            }
        });
        
        // 展開されている場合、子フォルダーを表示
        if is_expanded && has_children {
            Self::show_child_folders_simple(ui, app, folder_path, depth + 1);
        }
    }
    
    fn show_child_folders_simple(
        ui: &mut egui::Ui,
        app: &mut FileVisorApp,
        parent_path: &PathBuf,
        depth: usize,
    ) {
        // 高速化のため、最大表示数を制限
        const MAX_FOLDERS: usize = 50;
        
        if let Ok(entries) = std::fs::read_dir(parent_path) {
            let mut subdirs: Vec<_> = entries
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.path().is_dir())
                .filter(|entry| {
                    let file_name = entry.file_name();
                    let name = file_name.to_string_lossy();
                    app.state.show_hidden || !name.starts_with('.')
                })
                .map(|entry| entry.path())
                .take(MAX_FOLDERS)
                .collect();
            
            subdirs.sort();
            
            for subdir in subdirs {
                Self::show_folder_item(ui, app, &subdir, depth, false);
            }
        }
    }
    
    fn has_subdirectories_cached(path: &PathBuf) -> bool {
        // 極めて高速なチェック：ディレクトリエントリの存在確認のみ
        match std::fs::read_dir(path) {
            Ok(mut entries) => {
                // 最初のディレクトリエントリが見つかった時点で true を返す
                entries.find_map(|entry| {
                    entry.ok()?.path().is_dir().then_some(true)
                }).unwrap_or(false)
            }
            Err(_) => false,
        }
    }
    

    
    pub fn handle_tree_navigation(app: &mut FileVisorApp, ctx: &Context) {
        if app.state.active_pane != ActivePane::LeftSidebar {
            return;
        }
        
        ctx.input(|i| {
            if i.key_pressed(egui::Key::ArrowUp) {
                Self::navigate_up(app);
            }
            if i.key_pressed(egui::Key::ArrowDown) {
                Self::navigate_down(app);
            }
            if i.key_pressed(egui::Key::ArrowLeft) {
                Self::collapse_current(app);
            }
            if i.key_pressed(egui::Key::ArrowRight) {
                Self::expand_current(app);
            }
            if i.key_pressed(egui::Key::Enter) {
                Self::enter_selected(app);
            }
        });
    }
    
    fn navigate_up(_app: &mut FileVisorApp) {
        // TODO: 実装
        tracing::info!("ツリー上へ移動");
    }
    
    fn navigate_down(_app: &mut FileVisorApp) {
        // TODO: 実装
        tracing::info!("ツリー下へ移動");
    }
    
    fn collapse_current(app: &mut FileVisorApp) {
        if let Some(selected) = &app.state.sidebar_selected_item {
            app.state.expanded_folders.remove(selected);
        }
    }
    
    fn expand_current(app: &mut FileVisorApp) {
        if let Some(selected) = &app.state.sidebar_selected_item {
            app.state.expanded_folders.insert(selected.clone());
        }
    }
    
    fn enter_selected(app: &mut FileVisorApp) {
        if let Some(selected) = &app.state.sidebar_selected_item {
            app.navigate_to(selected.clone());
        }
    }
} 