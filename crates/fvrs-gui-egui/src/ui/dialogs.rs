use std::path::Path;
use crate::archive::ArchiveType;

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
                                        ("N", "新規ファイル作成", "空ファイルを作成"),
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

    /// 未保存変更確認ダイアログ
    pub fn show_unsaved_changes_dialog(
        ctx: &egui::Context,
        show_dialog: &mut bool,
        file_name: &str,
        save_callback: &mut dyn FnMut(),
        discard_callback: &mut dyn FnMut(),
        cancel_callback: &mut dyn FnMut(),
    ) {
        if *show_dialog {
            egui::Window::new("未保存の変更")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(10.0);
                        ui.colored_label(egui::Color32::YELLOW, "⚠️ 未保存の変更があります");
                        ui.add_space(10.0);
                        
                        ui.label("以下のファイルに未保存の変更があります:");
                        ui.add_space(5.0);
                        ui.monospace(file_name);
                        
                        ui.add_space(10.0);
                        ui.label("変更を保存しますか？");
                        ui.add_space(20.0);
                        
                        ui.horizontal(|ui| {
                            if ui.button("💾 保存して閉じる").clicked() {
                                save_callback();
                                *show_dialog = false;
                            }
                            ui.add_space(10.0);
                            if ui.button("🗑️ 破棄して閉じる").clicked() {
                                discard_callback();
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

    /// 新規ファイル作成ダイアログ
    pub fn show_create_file_dialog(
        ctx: &egui::Context,
        show_dialog: &mut bool,
        file_name: &mut String,
        create_callback: &mut dyn FnMut(&str),
        cancel_callback: &mut dyn FnMut(),
    ) {
        if *show_dialog {
            egui::Window::new("新規ファイル作成")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(10.0);
                        ui.colored_label(egui::Color32::GREEN, "📄 新規ファイルを作成");
                        ui.add_space(15.0);
                        
                        ui.label("ファイル名を入力してください:");
                        ui.add_space(5.0);
                        
                        let text_edit_response = ui.add(
                            egui::TextEdit::singleline(file_name)
                                .desired_width(300.0)
                                .hint_text("例: document.txt")
                        );
                        
                        // ダイアログが初回表示される時にフォーカスを設定
                        text_edit_response.request_focus();
                        
                        ui.add_space(5.0);
                        ui.colored_label(egui::Color32::GRAY, "💡 拡張子を含めてください（.txt, .md, .rs など）");
                        ui.add_space(15.0);
                        
                        // ファイル名の検証
                        let is_valid_name = !file_name.trim().is_empty() 
                            && !file_name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
                            && file_name.trim() != "." 
                            && file_name.trim() != "..";
                        
                        if !is_valid_name && !file_name.is_empty() {
                            ui.colored_label(egui::Color32::RED, "⚠️ 無効なファイル名です");
                            ui.add_space(10.0);
                        }
                        
                        ui.horizontal(|ui| {
                            ui.add_enabled_ui(is_valid_name, |ui| {
                                if ui.button("📄 作成").clicked() {
                                    create_callback(file_name.trim());
                                    *show_dialog = false;
                                }
                            });
                            
                            ui.add_space(10.0);
                            
                            if ui.button("❌ キャンセル").clicked() {
                                cancel_callback();
                                *show_dialog = false;
                            }
                        });
                        
                        ui.add_space(5.0);
                        ui.colored_label(egui::Color32::GRAY, "Enter キーで作成、Escape キーでキャンセル");
                        ui.add_space(10.0);
                        
                        // キーボードショートカット
                        if ui.input(|i| i.key_pressed(egui::Key::Enter)) && is_valid_name {
                            create_callback(file_name.trim());
                            *show_dialog = false;
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            cancel_callback();
                            *show_dialog = false;
                        }
                    });
                });
        }
    }

    /// 新規フォルダ作成ダイアログ
    pub fn show_create_folder_dialog(
        ctx: &egui::Context,
        show_dialog: &mut bool,
        folder_name: &mut String,
        create_callback: &mut dyn FnMut(&str),
        cancel_callback: &mut dyn FnMut(),
    ) {
        if *show_dialog {
            egui::Window::new("新規フォルダ作成")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(10.0);
                        ui.colored_label(egui::Color32::BLUE, "📁 新規フォルダを作成");
                        ui.add_space(15.0);
                        
                        ui.label("フォルダ名を入力してください:");
                        ui.add_space(5.0);
                        
                        let text_edit_response = ui.add(
                            egui::TextEdit::singleline(folder_name)
                                .desired_width(300.0)
                                .hint_text("例: 新しいフォルダ")
                        );
                        
                        // ダイアログが初回表示される時にフォーカスを設定
                        text_edit_response.request_focus();
                        
                        ui.add_space(5.0);
                        ui.colored_label(egui::Color32::GRAY, "💡 わかりやすい名前を付けてください");
                        ui.add_space(15.0);
                        
                        // フォルダ名の検証
                        let is_valid_name = !folder_name.trim().is_empty() 
                            && !folder_name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
                            && folder_name.trim() != "." 
                            && folder_name.trim() != "..";
                        
                        if !is_valid_name && !folder_name.is_empty() {
                            ui.colored_label(egui::Color32::RED, "⚠️ 無効なフォルダ名です");
                            ui.add_space(10.0);
                        }
                        
                        ui.horizontal(|ui| {
                            ui.add_enabled_ui(is_valid_name, |ui| {
                                if ui.button("📁 作成").clicked() {
                                    create_callback(folder_name.trim());
                                    *show_dialog = false;
                                }
                            });
                            
                            ui.add_space(10.0);
                            
                            if ui.button("❌ キャンセル").clicked() {
                                cancel_callback();
                                *show_dialog = false;
                            }
                        });
                        
                        ui.add_space(5.0);
                        ui.colored_label(egui::Color32::GRAY, "Enter キーで作成、Escape キーでキャンセル");
                        ui.add_space(10.0);
                        
                        // キーボードショートカット
                        if ui.input(|i| i.key_pressed(egui::Key::Enter)) && is_valid_name {
                            create_callback(folder_name.trim());
                            *show_dialog = false;
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            cancel_callback();
                            *show_dialog = false;
                        }
                    });
                });
        }
    }

    /// 解凍ダイアログ
    pub fn show_unpack_dialog(ctx: &egui::Context, app: &mut crate::app::FileVisorApp) {
        if !app.state.show_unpack_dialog {
            return;
        }

        egui::Window::new("ファイル解凍")
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    if let Some(archive_path) = &app.state.current_archive {
                        ui.label(format!("圧縮ファイル: {}", archive_path.file_name().unwrap_or_default().to_string_lossy()));
                        ui.add_space(10.0);
                        
                        ui.label("解凍先:");
                        ui.text_edit_singleline(&mut app.state.unpack_destination);
                        ui.add_space(10.0);
                        
                        ui.horizontal(|ui| {
                            if ui.button("参照").clicked() {
                                // ファイルダイアログで解凍先を選択
                                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                    app.state.unpack_destination = path.to_string_lossy().to_string();
                                }
                            }
                            
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button("キャンセル").clicked() {
                                    app.state.show_unpack_dialog = false;
                                }
                                
                                if ui.button("解凍").clicked() {
                                    app.extract_archive();
                                }
                            });
                        });
                    }
                });
                
                // Escapeキーでダイアログを閉じる
                if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                    app.state.show_unpack_dialog = false;
                }
            });
    }

    /// 圧縮ダイアログ
    pub fn show_pack_dialog(ctx: &egui::Context, app: &mut crate::app::FileVisorApp) {
        if !app.state.show_pack_dialog {
            return;
        }

        egui::Window::new("ファイル圧縮")
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    ui.label(format!("選択されたアイテム: {} 個", app.state.selected_items.len()));
                    ui.add_space(10.0);
                    
                    ui.label("ファイル名:");
                    ui.text_edit_singleline(&mut app.state.pack_filename);
                    ui.add_space(10.0);
                    
                    ui.label("圧縮形式:");
                    egui::ComboBox::from_label("")
                        .selected_text(format!("{:?}", app.state.pack_format))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut app.state.pack_format, ArchiveType::Zip, "ZIP");
                            ui.selectable_value(&mut app.state.pack_format, ArchiveType::Tar, "TAR");
                            ui.selectable_value(&mut app.state.pack_format, ArchiveType::TarGz, "TAR.GZ");
                        });
                    ui.add_space(10.0);
                    
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("キャンセル").clicked() {
                                app.state.show_pack_dialog = false;
                            }
                            
                            if ui.button("圧縮").clicked() {
                                app.create_archive();
                            }
                        });
                    });
                });
                
                // Escapeキーでダイアログを閉じる
                if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                    app.state.show_pack_dialog = false;
                }
            });
    }

    /// 圧縮ファイルビューア
    pub fn show_archive_viewer(ctx: &egui::Context, app: &mut crate::app::FileVisorApp) {
        if !app.state.show_archive_viewer {
            return;
        }

        egui::Window::new("圧縮ファイル内容")
            .resizable(true)
            .default_size([600.0, 400.0])
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    if let Some(archive_path) = &app.state.current_archive {
                        ui.label(format!("ファイル: {}", archive_path.file_name().unwrap_or_default().to_string_lossy()));
                        ui.separator();
                        
                        // ヘッダー
                        ui.horizontal(|ui| {
                            ui.label("名前");
                            ui.separator();
                            ui.label("サイズ");
                            ui.separator();
                            ui.label("圧縮サイズ");
                            ui.separator();
                            ui.label("種類");
                        });
                        ui.separator();
                        
                        // エントリリスト
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for entry in &app.state.archive_entries {
                                ui.horizontal(|ui| {
                                    if entry.is_dir {
                                        ui.label("📁");
                                    } else {
                                        ui.label("📄");
                                    }
                                    ui.label(&entry.name);
                                    ui.separator();
                                    ui.label(format!("{}", entry.size));
                                    ui.separator();
                                    ui.label(format!("{}", entry.compressed_size));
                                    ui.separator();
                                    ui.label(if entry.is_dir { "フォルダ" } else { "ファイル" });
                                });
                            }
                        });
                        
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button("閉じる").clicked() {
                                    app.close_archive_viewer();
                                }
                            });
                        });
                    }
                });
                
                // Escapeキーでビューアを閉じる
                if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                    app.close_archive_viewer();
                }
            });
    }
} 