use std::path::Path;

pub struct DialogsUI;

impl DialogsUI {
    /// 削除確認ダイアログ
    pub fn show_delete_dialog(
        ctx: &egui::Context,
        show_dialog: &mut bool,
        items_to_delete: &[std::path::PathBuf],
        _current_path: &Path,
        delete_callback: &mut dyn FnMut(),
        cancel_callback: &mut dyn FnMut(),
    ) {
        if *show_dialog {
            egui::Window::new("削除確認")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(10.0);
                        ui.colored_label(egui::Color32::RED, "⚠️ 削除確認");
                        ui.add_space(10.0);
                        
                        if items_to_delete.len() == 1 {
                            ui.label("以下のアイテムを削除しますか？");
                            ui.add_space(5.0);
                            let name = items_to_delete[0]
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| "不明なアイテム".to_string());
                            ui.monospace(&name);
                        } else {
                            ui.label(format!("{}個のアイテムを削除しますか？", items_to_delete.len()));
                            ui.add_space(5.0);
                            ui.label("削除対象:");
                            
                            egui::ScrollArea::vertical()
                                .max_height(150.0)
                                .show(ui, |ui| {
                                    for item in items_to_delete.iter().take(10) {
                                        let name = item
                                            .file_name()
                                            .map(|n| n.to_string_lossy().to_string())
                                            .unwrap_or_else(|| "不明なアイテム".to_string());
                                        ui.monospace(&name);
                                    }
                                    if items_to_delete.len() > 10 {
                                        ui.label(format!("... 他{}個", items_to_delete.len() - 10));
                                    }
                                });
                        }
                        
                        ui.add_space(10.0);
                        ui.colored_label(egui::Color32::GRAY, "この操作は元に戻せません");
                        ui.add_space(20.0);
                        
                        ui.horizontal(|ui| {
                            if ui.button("🗑️ 削除").clicked() {
                                delete_callback();
                                *show_dialog = false;
                            }
                            ui.add_space(10.0);
                            if ui.button("❌ キャンセル").clicked() {
                                cancel_callback();
                                *show_dialog = false;
                            }
                        });
                        
                        ui.add_space(10.0);
                    });
                });
        }
    }

    /// ショートカットキー一覧ダイアログ
    pub fn show_shortcuts_dialog(ctx: &egui::Context, show_dialog: &mut bool) {
        if *show_dialog {
            egui::Window::new("ショートカットキー一覧")
                .collapsible(false)
                .resizable(true)
                .default_width(600.0)
                .default_height(500.0)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.vertical(|ui| {
                        ui.label("🔥 ワンタッチキー (A～Z)");
                        ui.separator();
                        
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            egui::Grid::new("shortcuts_grid")
                                .num_columns(3)
                                .striped(true)
                                .show(ui, |ui| {
                                    ui.strong("キー");
                                    ui.strong("機能");
                                    ui.strong("説明");
                                    ui.end_row();

                                    let shortcuts = [
                                        ("A", "属性変更", "ファイル・フォルダの属性を変更"),
                                        ("B", "バイナリ編集", "バイナリエディタで開く"),
                                        ("C", "コピー", "選択アイテムをクリップボードにコピー"),
                                        ("D", "削除", "選択アイテムを削除"),
                                        ("E", "エディタで編集", "テキストエディタで開く"),
                                        ("F", "検索", "ファイル・フォルダを検索"),
                                        ("G", "履歴", "ナビゲーション履歴を表示"),
                                        ("H", "連結", "ファイルを連結"),
                                        ("I", "ファイル情報", "選択アイテムの詳細情報を表示"),
                                        ("K", "フォルダの作成", "新しいフォルダを作成"),
                                        ("L", "フォルダを開く", "選択したフォルダに移動"),
                                        ("M", "移動", "選択アイテムを切り取り"),
                                        ("N", "分割", "ファイルを分割"),
                                        ("O", "開く", "選択アイテムを開く"),
                                        ("P", "圧縮書庫の作成", "選択アイテムを圧縮"),
                                        ("Q", "ホットキー", "ホットキーメニューを表示"),
                                        ("R", "名前変更", "選択アイテムの名前を変更"),
                                        ("S", "ソート条件の変更", "ソート方法を切り替え"),
                                        ("T", "フルパス名をコピー", "フルパスをクリップボードにコピー"),
                                        ("U", "圧縮書庫の解凍", "圧縮ファイルを解凍"),
                                        ("V", "ファイル閲覧", "ファイルビューアで開く"),
                                        ("W", "フィルタ", "表示フィルタを設定"),
                                        ("X", "実行", "選択ファイルを実行"),
                                        ("Y", "関連付け", "ファイルの関連付けを表示"),
                                        ("Z", "一括選択", "すべてのアイテムを選択"),
                                    ];

                                    for (key, name, desc) in shortcuts {
                                        ui.monospace(key);
                                        ui.label(name);
                                        ui.label(desc);
                                        ui.end_row();
                                    }
                                });
                        });

                        ui.separator();
                        ui.label("🎯 その他のショートカット");
                        
                        egui::Grid::new("other_shortcuts_grid")
                            .num_columns(2)
                            .striped(true)
                            .show(ui, |ui| {
                                ui.strong("キー");
                                ui.strong("機能");
                                ui.end_row();

                                let other_shortcuts = [
                                    ("F5", "ディレクトリ更新"),
                                    ("Delete", "削除"),
                                    ("Alt + ←", "戻る"),
                                    ("Alt + →", "進む"),
                                ];

                                for (key, desc) in other_shortcuts {
                                    ui.monospace(key);
                                    ui.label(desc);
                                    ui.end_row();
                                }
                            });

                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            if ui.button("閉じる").clicked() {
                                *show_dialog = false;
                            }
                        });
                    });
                });
        }
    }
} 