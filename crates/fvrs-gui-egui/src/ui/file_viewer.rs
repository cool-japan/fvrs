use std::path::{Path, PathBuf};
use crate::app::FileVisorApp;

pub struct FileViewerUI;

impl FileViewerUI {
    /// ファイル閲覧・編集パネルを表示
    pub fn show_file_viewer(
        ui: &mut egui::Ui,
        app: &mut FileVisorApp,
    ) {
        if app.state.show_file_viewer {
            ui.heading("ファイル閲覧・編集");
            ui.separator();
            
            // ツールバー
            ui.horizontal(|ui| {
                // ファイル名表示
                if let Some(file_path) = &app.state.viewed_file_path {
                    let file_name = file_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("不明なファイル");
                    ui.label(format!("📄 {}", file_name));
                    
                    // バイナリファイル表示
                    if !Self::is_text_file(file_path) {
                        ui.colored_label(egui::Color32::LIGHT_BLUE, "[バイナリ]");
                    }
                    
                    if app.state.is_file_modified {
                        ui.colored_label(egui::Color32::YELLOW, "●");
                        ui.label("変更あり");
                    }
                } else {
                    ui.label("ファイルが選択されていません");
                }
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // 閉じるボタン
                    if ui.button("❌").clicked() {
                        Self::close_file_viewer(app);
                    }
                    
                    // 保存ボタン（編集モードの場合）
                    if app.state.view_mode_text && app.state.is_file_modified {
                        if ui.button("💾 保存").clicked() {
                            Self::save_file(app);
                        }
                    }
                    
                    // 行番号表示切替（編集モードのみ）
                    if app.state.view_mode_text {
                        if ui.button(if app.state.show_line_numbers { "🔢 行番号OFF" } else { "🔢 行番号ON" }).clicked() {
                            app.state.show_line_numbers = !app.state.show_line_numbers;
                        }
                    }
                    
                    // モード切替（バイナリファイルは編集不可）
                    if let Some(file_path) = &app.state.viewed_file_path {
                        if Self::is_text_file(file_path) {
                            if app.state.view_mode_text {
                                if ui.button("👁 閲覧モード").clicked() {
                                    app.state.view_mode_text = false;
                                }
                                ui.label("✏️ 編集中");
                            } else {
                                if ui.button("✏️ 編集モード").clicked() {
                                    app.state.view_mode_text = true;
                                }
                                ui.label("👁 閲覧中");
                            }
                        } else {
                            ui.label("👁 閲覧専用");
                        }
                    }
                });
            });
            
            ui.separator();
            
            // ファイル内容表示・編集エリア
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if app.state.view_mode_text {
                        // 編集モード
                        if app.state.show_line_numbers {
                            Self::show_editor_with_line_numbers(ui, app);
                        } else {
                            let response = ui.add(
                                egui::TextEdit::multiline(&mut app.state.viewed_file_content)
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(30)
                                    .code_editor()
                            );
                            
                            if response.changed() {
                                app.state.is_file_modified = true;
                            }
                        }
                        
                        // Ctrl+S で保存
                        if ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::S)) {
                            Self::save_file(app);
                        }
                    } else {
                        // 閲覧モード
                        if app.state.show_line_numbers {
                            Self::show_text_with_line_numbers(ui, &app.state.viewed_file_content);
                        } else {
                            ui.add(
                                egui::Label::new(&app.state.viewed_file_content)
                                    .wrap()
                            );
                        }
                    }
                });
                
            ui.separator();
            
            // ステータス情報
            ui.horizontal(|ui| {
                if let Some(file_path) = &app.state.viewed_file_path {
                    // ファイルサイズ
                    if let Ok(metadata) = std::fs::metadata(file_path) {
                        ui.label(format!("サイズ: {} バイト", metadata.len()));
                    }
                    
                    ui.separator();
                    
                    // 行数・文字数
                    let lines = app.state.viewed_file_content.lines().count();
                    let chars = app.state.viewed_file_content.chars().count();
                    ui.label(format!("行数: {} / 文字数: {}", lines, chars));
                }
            });
        } else {
            // ファイル閲覧パネルが非表示の場合の案内
            ui.vertical_centered(|ui| {
                ui.add_space(50.0);
                ui.heading("ファイル閲覧・編集");
                ui.add_space(20.0);
                ui.label("ファイルを選択してください");
                ui.add_space(10.0);
                ui.colored_label(egui::Color32::GRAY, "💡 ショートカット:");
                ui.colored_label(egui::Color32::GRAY, "V キー: ファイル閲覧");
                ui.colored_label(egui::Color32::GRAY, "E キー: エディタで編集");
            });
        }
    }
    
    /// ファイルを開く（閲覧モード）
    pub fn open_file_for_viewing(app: &mut FileVisorApp, file_path: PathBuf) {
        if Self::is_text_file(&file_path) {
            match std::fs::read_to_string(&file_path) {
                Ok(content) => {
                    app.state.viewed_file_path = Some(file_path);
                    app.state.viewed_file_content = content;
                    app.state.show_file_viewer = true;
                    app.state.view_mode_text = false; // 閲覧モード
                    app.state.is_file_modified = false;
                    tracing::info!("ファイルを閲覧モードで開きました: {:?}", app.state.viewed_file_path);
                }
                Err(e) => {
                    tracing::error!("ファイル読み込みエラー: {:?}", e);
                    // TODO: エラーダイアログを表示
                }
            }
        } else {
            // バイナリファイルの場合は16進表示で開く
            Self::open_binary_file_for_viewing(app, file_path);
        }
    }
    
    /// ファイルを開く（編集モード）
    pub fn open_file_for_editing(app: &mut FileVisorApp, file_path: PathBuf) {
        if Self::is_text_file(&file_path) {
            match std::fs::read_to_string(&file_path) {
                Ok(content) => {
                    app.state.viewed_file_path = Some(file_path);
                    app.state.viewed_file_content = content;
                    app.state.show_file_viewer = true;
                    app.state.view_mode_text = true; // 編集モード
                    app.state.is_file_modified = false;
                    tracing::info!("ファイルを編集モードで開きました: {:?}", app.state.viewed_file_path);
                }
                Err(e) => {
                    tracing::error!("ファイル読み込みエラー: {:?}", e);
                    // TODO: エラーダイアログを表示
                }
            }
        } else {
            // バイナリファイルは編集不可として閲覧モードで開く
            tracing::warn!("バイナリファイルは編集できません。閲覧モードで開きます: {:?}", file_path);
            Self::open_binary_file_for_viewing(app, file_path);
        }
    }
    
    /// バイナリファイルを16進表示で開く
    fn open_binary_file_for_viewing(app: &mut FileVisorApp, file_path: PathBuf) {
        match std::fs::read(&file_path) {
            Ok(data) => {
                let hex_content = Self::format_as_hex(&data);
                app.state.viewed_file_path = Some(file_path);
                app.state.viewed_file_content = hex_content;
                app.state.show_file_viewer = true;
                app.state.view_mode_text = false; // 閲覧モード（編集不可）
                app.state.is_file_modified = false;
                tracing::info!("バイナリファイルを16進表示で開きました: {:?}", app.state.viewed_file_path);
            }
            Err(e) => {
                tracing::error!("バイナリファイル読み込みエラー: {:?}", e);
                // TODO: エラーダイアログを表示
            }
        }
    }
    
    /// バイナリデータを16進文字列に変換
    fn format_as_hex(data: &[u8]) -> String {
        let mut result = String::new();
        
        for (i, chunk) in data.chunks(16).enumerate() {
            // オフセット表示
            result.push_str(&format!("{:08X}  ", i * 16));
            
            // 16進数表示
            for (j, byte) in chunk.iter().enumerate() {
                if j == 8 {
                    result.push(' '); // 8バイト目で区切り
                }
                result.push_str(&format!("{:02X} ", byte));
            }
            
            // 不足分を空白で埋める
            let remaining = 16 - chunk.len();
            for j in 0..remaining {
                if chunk.len() + j == 8 {
                    result.push(' ');
                }
                result.push_str("   ");
            }
            
            result.push(' ');
            
            // ASCII表示
            for byte in chunk {
                if byte.is_ascii_graphic() || *byte == b' ' {
                    result.push(*byte as char);
                } else {
                    result.push('.');
                }
            }
            
            result.push('\n');
        }
        
        // ファイルサイズ情報を先頭に追加
        let header = format!(
            "バイナリファイル - サイズ: {} バイト ({} KB)\n\n",
            data.len(),
            (data.len() + 1023) / 1024
        );
        
        header + &result
    }
    
    /// ファイルを保存
    fn save_file(app: &mut FileVisorApp) {
        if let Some(file_path) = &app.state.viewed_file_path {
            match std::fs::write(file_path, &app.state.viewed_file_content) {
                Ok(_) => {
                    app.state.is_file_modified = false;
                    tracing::info!("ファイルを保存しました: {:?}", file_path);
                }
                Err(e) => {
                    tracing::error!("ファイル保存エラー: {:?}", e);
                    // TODO: エラーダイアログを表示
                }
            }
        }
    }
    
    /// ファイル閲覧パネルを閉じる
    fn close_file_viewer(app: &mut FileVisorApp) {
        if app.state.is_file_modified {
            // 未保存の変更がある場合は確認ダイアログを表示
            app.state.show_unsaved_dialog = true;
            app.state.pending_close_action = true;
        } else {
            // 変更がない場合は直接閉じる
            Self::force_close_file_viewer(app);
        }
    }
    
    /// ファイル閲覧パネルを強制的に閉じる（未保存変更があっても）
    pub fn force_close_file_viewer(app: &mut FileVisorApp) {
        app.state.show_file_viewer = false;
        app.state.viewed_file_path = None;
        app.state.viewed_file_content.clear();
        app.state.is_file_modified = false;
        app.state.show_unsaved_dialog = false;
        app.state.pending_close_action = false;
    }
    
    /// ファイルを保存して閉じる
    pub fn save_and_close_file_viewer(app: &mut FileVisorApp) {
        Self::save_file(app);
        Self::force_close_file_viewer(app);
    }
    
    /// 行番号付きエディタを表示
    fn show_editor_with_line_numbers(ui: &mut egui::Ui, app: &mut FileVisorApp) {
        ui.horizontal(|ui| {
            // 行番号エリア
            let line_count = app.state.viewed_file_content.lines().count();
            let line_number_width = (line_count.to_string().len() as f32 * 10.0).max(40.0);
            
            ui.allocate_ui_with_layout(
                [line_number_width, ui.available_height()].into(),
                egui::Layout::top_down(egui::Align::RIGHT),
                |ui| {
                    ui.add_space(5.0);
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for line_num in 1..=line_count.max(1) {
                                ui.monospace(format!("{:4}", line_num));
                            }
                        });
                }
            );
            
            ui.separator();
            
            // エディタエリア
            let response = ui.add(
                egui::TextEdit::multiline(&mut app.state.viewed_file_content)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY)
                    .desired_rows(30)
                    .code_editor()
            );
            
            if response.changed() {
                app.state.is_file_modified = true;
            }
        });
    }
    
    /// 行番号付きテキストを表示（閲覧モード）
    fn show_text_with_line_numbers(ui: &mut egui::Ui, content: &str) {
        ui.horizontal(|ui| {
            // 行番号エリア
            let lines: Vec<&str> = content.lines().collect();
            let line_count = lines.len();
            let line_number_width = (line_count.to_string().len() as f32 * 10.0).max(40.0);
            
            ui.allocate_ui_with_layout(
                [line_number_width, ui.available_height()].into(),
                egui::Layout::top_down(egui::Align::RIGHT),
                |ui| {
                    ui.add_space(5.0);
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for line_num in 1..=line_count.max(1) {
                                ui.monospace(format!("{:4}", line_num));
                            }
                        });
                }
            );
            
            ui.separator();
            
            // テキストエリア
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for line in lines {
                        ui.monospace(line);
                    }
                });
        });
    }

    /// テキストファイルかどうかを判定
    fn is_text_file(file_path: &Path) -> bool {
        if let Some(extension) = file_path.extension() {
            if let Some(ext_str) = extension.to_str() {
                let text_extensions = [
                    "txt", "md", "rs", "py", "js", "html", "css", "json", "xml", "yaml", "yml",
                    "toml", "ini", "cfg", "conf", "log", "csv", "sql", "sh", "bat", "cmd",
                    "c", "cpp", "h", "hpp", "java", "kt", "swift", "go", "php", "rb", "pl",
                    "ts", "jsx", "tsx", "vue", "svelte", "scss", "less", "sass", "dockerfile",
                    "gitignore", "gitattributes", "license", "readme", "changelog", "makefile"
                ];
                
                return text_extensions.contains(&ext_str.to_lowercase().as_str());
            }
        }
        
        // 拡張子がない場合は、ファイル名で判定
        if let Some(file_name) = file_path.file_name() {
            if let Some(name_str) = file_name.to_str() {
                let text_files = [
                    "readme", "license", "changelog", "makefile", "dockerfile",
                    "gitignore", "gitattributes", "cargo.toml", "package.json"
                ];
                
                return text_files.contains(&name_str.to_lowercase().as_str());
            }
        }
        
        false
    }
} 