use egui::{Color32, RichText, ScrollArea};
use crate::app::FileVisorApp;
use crate::file_info::{DetailedFileInfo, FileInfoCollector, format_size};

pub struct FileInfoDialog;

impl FileInfoDialog {
    /// ファイル情報ダイアログを表示
    pub fn show(ctx: &egui::Context, app: &mut FileVisorApp) {
        if !app.state.show_file_info_dialog {
            return;
        }

        let target_path = app.state.file_info_target.clone();
        let mut close_requested = false;
        
        egui::Window::new("ファイル情報")
            .default_width(600.0)
            .default_height(500.0)
            .resizable(true)
            .collapsible(false)
            .open(&mut app.state.show_file_info_dialog)
            .show(ctx, |ui| {
                if let Some(target_path) = &target_path {
                    match FileInfoCollector::collect_detailed_info(target_path) {
                        Ok(info) => {
                            Self::show_file_info_content(ui, &info);
                        }
                        Err(err) => {
                            ui.colored_label(Color32::RED, format!("エラー: {}", err));
                        }
                    }
                } else {
                    ui.label("ファイルが選択されていません");
                }
                
                ui.separator();
                
                // ボタン
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("閉じる").clicked() {
                            close_requested = true;
                        }
                        
                        if ui.button("更新").clicked() {
                            // 強制的に情報を再取得
                        }
                    });
                });
            });
        
        if close_requested {
            app.state.show_file_info_dialog = false;
        }
    }
    
    /// ファイル情報の詳細内容を表示
    fn show_file_info_content(ui: &mut egui::Ui, info: &DetailedFileInfo) {
        ScrollArea::vertical().show(ui, |ui| {
            // ファイルアイコンと基本情報
            ui.horizontal(|ui| {
                // アイコン（簡略化）
                let icon = if info.is_directory {
                    "📁"
                } else {
                    Self::get_file_icon(&info.file_extension)
                };
                ui.label(RichText::new(icon).size(32.0));
                
                ui.vertical(|ui| {
                    ui.label(RichText::new(&info.name).size(16.0).strong());
                    ui.label(RichText::new(&info.file_type).color(Color32::GRAY));
                });
            });
            
            ui.add_space(10.0);
            
            // タブ形式で情報を整理（簡易実装）
            ui.horizontal(|ui| {
                let _ = ui.selectable_label(true, "全般");
                let _ = ui.selectable_label(false, "詳細");
                let _ = ui.selectable_label(false, "セキュリティ");
            });
            
            ui.separator();
            Self::show_general_info(ui, info);
            
            ui.separator();
            ui.collapsing("詳細情報", |ui| {
                Self::show_detailed_info(ui, info);
            });
            
            ui.collapsing("セキュリティ情報", |ui| {
                Self::show_security_info(ui, info);
            });
        });
    }
    
    /// 全般情報タブ
    fn show_general_info(ui: &mut egui::Ui, info: &DetailedFileInfo) {
        ui.add_space(10.0);
        
        egui::Grid::new("general_info_grid")
            .num_columns(2)
            .spacing([20.0, 8.0])
            .striped(true)
            .show(ui, |ui| {
                // 基本情報
                ui.label(RichText::new("場所:").strong());
                ui.label(info.full_path.parent().map_or("不明".to_string(), |p| p.to_string_lossy().to_string()));
                ui.end_row();
                
                ui.label(RichText::new("種類:").strong());
                ui.label(&info.file_type);
                ui.end_row();
                
                if !info.is_directory {
                    ui.label(RichText::new("サイズ:").strong());
                    ui.label(format_size(info.size));
                    ui.end_row();
                    
                    ui.label(RichText::new("ディスク上のサイズ:").strong());
                    ui.label(format_size(info.size_on_disk));
                    ui.end_row();
                }
                
                // 時刻情報
                if let Some(created) = &info.created {
                    ui.label(RichText::new("作成日時:").strong());
                    ui.label(created.format("%Y年%m月%d日 %H:%M:%S").to_string());
                    ui.end_row();
                }
                
                if let Some(modified) = &info.modified {
                    ui.label(RichText::new("更新日時:").strong());
                    ui.label(modified.format("%Y年%m月%d日 %H:%M:%S").to_string());
                    ui.end_row();
                }
                
                if let Some(accessed) = &info.accessed {
                    ui.label(RichText::new("アクセス日時:").strong());
                    ui.label(accessed.format("%Y年%m月%d日 %H:%M:%S").to_string());
                    ui.end_row();
                }
                
                // 属性
                ui.label(RichText::new("属性:").strong());
                let mut attributes = Vec::new();
                if info.is_readonly { attributes.push("読み取り専用"); }
                if info.is_hidden { attributes.push("隠しファイル"); }
                if info.is_system { attributes.push("システム"); }
                if attributes.is_empty() { attributes.push("標準"); }
                ui.label(attributes.join(", "));
                ui.end_row();
            });
        
        ui.add_space(20.0);
        
        // 関連付け情報
        if let Some(program) = &info.associated_program {
            ui.separator();
            ui.label(RichText::new("このファイルを実行するプログラム").strong());
            ui.add_space(5.0);
            
            egui::Grid::new("association_grid")
                .num_columns(2)
                .spacing([20.0, 8.0])
                .show(ui, |ui| {
                    ui.label("プログラム:");
                    ui.label(program);
                    ui.end_row();
                    
                    if let Some(command) = &info.open_with_command {
                        ui.label("コマンド:");
                        ui.label(command);
                        ui.end_row();
                    }
                });
        }
    }
    
    /// 詳細情報タブ
    fn show_detailed_info(ui: &mut egui::Ui, info: &DetailedFileInfo) {
        ui.add_space(10.0);
        
        egui::Grid::new("detailed_info_grid")
            .num_columns(2)
            .spacing([20.0, 8.0])
            .striped(true)
            .show(ui, |ui| {
                ui.label(RichText::new("フルパス:").strong());
                ui.label(info.full_path.to_string_lossy().to_string());
                ui.end_row();
                
                if !info.file_extension.is_empty() {
                    ui.label(RichText::new("拡張子:").strong());
                    ui.label(&info.file_extension);
                    ui.end_row();
                }
                
                ui.label(RichText::new("MIMEタイプ:").strong());
                ui.label(&info.mime_type);
                ui.end_row();
                
                // PE ファイル情報
                if let Some(version) = &info.version {
                    ui.label(RichText::new("ファイルバージョン:").strong());
                    ui.label(version);
                    ui.end_row();
                }
                
                if let Some(company) = &info.company {
                    ui.label(RichText::new("会社名:").strong());
                    ui.label(company);
                    ui.end_row();
                }
                
                if let Some(description) = &info.description {
                    ui.label(RichText::new("説明:").strong());
                    ui.label(description);
                    ui.end_row();
                }
                
                if let Some(copyright) = &info.copyright {
                    ui.label(RichText::new("著作権:").strong());
                    ui.label(copyright);
                    ui.end_row();
                }
                
                // システム情報
                ui.label(RichText::new("コンピューター:").strong());
                ui.label(&info.computer_name);
                ui.end_row();
            });
        
        ui.add_space(20.0);
        
        // ディスク容量情報
        if let Some(disk_info) = &info.disk_space {
            ui.separator();
            ui.label(RichText::new("ディスク容量情報").strong());
            ui.add_space(5.0);
            
            egui::Grid::new("disk_info_grid")
                .num_columns(2)
                .spacing([20.0, 8.0])
                .show(ui, |ui| {
                    ui.label("合計サイズ:");
                    ui.label(format_size(disk_info.total_space));
                    ui.end_row();
                    
                    ui.label("空き領域:");
                    ui.label(format_size(disk_info.free_space));
                    ui.end_row();
                    
                    ui.label("使用済み:");
                    ui.label(format_size(disk_info.used_space));
                    ui.end_row();
                });
        }
    }
    
    /// セキュリティ情報タブ
    fn show_security_info(ui: &mut egui::Ui, info: &DetailedFileInfo) {
        ui.add_space(10.0);
        
        egui::Grid::new("security_info_grid")
            .num_columns(2)
            .spacing([20.0, 8.0])
            .striped(true)
            .show(ui, |ui| {
                ui.label(RichText::new("所有者:").strong());
                ui.label(&info.owner);
                ui.end_row();
                
                ui.label(RichText::new("アクセス許可:").strong());
                ui.label(&info.permissions);
                ui.end_row();
                
                // セキュリティ属性
                ui.label(RichText::new("セキュリティ属性:").strong());
                let mut security_attrs = Vec::new();
                if info.is_readonly { security_attrs.push("読み取り専用"); }
                if info.is_hidden { security_attrs.push("隠しファイル"); }
                if info.is_system { security_attrs.push("システムファイル"); }
                if security_attrs.is_empty() { security_attrs.push("標準"); }
                ui.label(security_attrs.join(", "));
                ui.end_row();
            });
    }
    
    /// ファイル拡張子に基づいてアイコンを取得
    fn get_file_icon(extension: &str) -> &'static str {
        match extension.to_lowercase().as_str() {
            "txt" => "📄",
            "doc" | "docx" => "📝",
            "xls" | "xlsx" => "📊",
            "ppt" | "pptx" => "📊",
            "pdf" => "📕",
            "jpg" | "jpeg" | "png" | "gif" | "bmp" => "🖼️",
            "mp4" | "avi" | "mkv" | "mov" => "🎬",
            "mp3" | "wav" | "ogg" | "flac" => "🎵",
            "zip" | "rar" | "7z" | "tar" | "gz" => "📦",
            "exe" | "msi" => "⚙️",
            "dll" => "🔧",
            "rs" | "py" | "js" | "html" | "css" => "💻",
            "json" | "xml" | "yaml" | "yml" | "toml" => "📋",
            _ => "📄",
        }
    }
} 