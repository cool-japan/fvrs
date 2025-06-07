use std::path::PathBuf;

pub struct DialogsUI;

impl DialogsUI {
    /// 削除確認ダイアログを表示
    pub fn show_delete_dialog(
        ctx: &egui::Context,
        show_dialog: &mut bool,
        items_to_delete: &[PathBuf],
        current_path: &PathBuf,
        delete_callback: &mut dyn FnMut(),
        cancel_callback: &mut dyn FnMut(),
    ) {
        if *show_dialog {
            egui::Window::new("ファイルの削除")
                .collapsible(false)
                .resizable(false)
                .auto_sized()
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("⚠️");
                        ui.vertical(|ui| {
                            ui.label("現在のフォルダ:");
                            ui.label(format!("{}", current_path.display()));
                        });
                    });
                    
                    ui.add_space(10.0);
                    
                    ui.label("削除するファイルやフォルダ:");
                    ui.separator();
                    
                    egui::ScrollArea::vertical()
                        .max_height(150.0)
                        .show(ui, |ui| {
                            for path in items_to_delete {
                                let name = path.file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("不明");
                                let icon = if path.is_dir() { "📁" } else { "📄" };
                                ui.label(format!("{} {}", icon, name));
                            }
                        });
                    
                    ui.add_space(10.0);
                    ui.separator();
                    
                    ui.horizontal(|ui| {
                        if ui.button("削除").clicked() {
                            delete_callback();
                        }
                        
                        if ui.button("すべて削除(A)").clicked() {
                            delete_callback();
                        }
                        
                        if ui.button("キャンセル").clicked() {
                            cancel_callback();
                        }
                    });
                });
        }
    }
} 