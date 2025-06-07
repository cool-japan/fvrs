mod state;
mod utils;
mod app;
mod ui;
mod archive;
mod file_info;

use std::path::PathBuf;
use fvrs_core::core::FileEntry;

use app::FileVisorApp;
use state::{ViewMode, SortColumn, ActivePane};
use ui::{FileListUI, DialogsUI, ShortcutHandler, FileViewerUI, FileInfoDialog, ExplorerTreeUI};

impl eframe::App for FileVisorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // パフォーマンス監視
        let frame_start = std::time::Instant::now();

        // キーボードショートカット
        ShortcutHandler::handle_shortcuts(self, ctx);
        
        // エクスプローラーツリーのナビゲーション
        ExplorerTreeUI::handle_tree_navigation(self, ctx);

        // メニューバー
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                // ファイルメニュー
                ui.menu_button("ファイル", |ui| {
                    ui.menu_button("新規作成", |ui| {
                        if ui.button("📁 新規フォルダ").clicked() {
                            self.state.show_create_folder_dialog = true;
                            self.state.new_folder_name.clear();
                            ui.close_menu();
                        }
                        if ui.button("📄 新規ファイル").clicked() {
                            self.state.show_create_file_dialog = true;
                            self.state.new_file_name.clear();
                            ui.close_menu();
                        }
                    });
                    ui.menu_button("コピー・移動", |ui| {
                        if ui.button("コピー").clicked() { ui.close_menu(); }
                        if ui.button("移動").clicked() { ui.close_menu(); }
                    });
                    if ui.button("属性の変更").clicked() { ui.close_menu(); }
                    if ui.button("名前の変更").clicked() { ui.close_menu(); }
                    ui.separator();
                    if ui.button("削除").clicked() {
                        self.show_delete_confirmation();
                        ui.close_menu();
                    }
                    if ui.button("一括削除").clicked() { ui.close_menu(); }
                    ui.separator();
                    if ui.button("連結と分割").clicked() { ui.close_menu(); }
                    ui.menu_button("圧縮書庫ファイルの操作", |ui| {
                        if ui.button("圧縮").clicked() { ui.close_menu(); }
                        if ui.button("展開").clicked() { ui.close_menu(); }
                    });
                    if ui.button("シュレッダ").clicked() { ui.close_menu(); }
                    if ui.button("プロパティ").clicked() { ui.close_menu(); }
                    ui.separator();
                    if ui.button("関連付け").clicked() { ui.close_menu(); }
                    if ui.button("名前を指定して実行").clicked() { ui.close_menu(); }
                    ui.separator();
                    if ui.button("終了").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if ui.button("すべて終了").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                // 編集メニュー
                ui.menu_button("編集", |ui| {
                    if ui.button("オブジェクトの切り取り").clicked() { ui.close_menu(); }
                    if ui.button("オブジェクトのコピー").clicked() { ui.close_menu(); }
                    if ui.button("オブジェクトの貼り付け").clicked() { ui.close_menu(); }
                    if ui.button("ショートカットの貼り付け").clicked() { ui.close_menu(); }
                    ui.separator();
                    if ui.button("パス名をコピー").clicked() { ui.close_menu(); }
                    ui.menu_button("すべて選択・選択を反転", |ui| {
                        if ui.button("すべて選択").clicked() { ui.close_menu(); }
                        if ui.button("選択を反転").clicked() { ui.close_menu(); }
                    });
                    ui.separator();
                    if ui.button("フィルタ").clicked() { ui.close_menu(); }
                    if ui.button("一括選択").clicked() { ui.close_menu(); }
                });

                // ディスクメニュー
                ui.menu_button("ディスク", |ui| {
                    if ui.button("ディスクコピー").clicked() { ui.close_menu(); }
                    if ui.button("ボリュームラベル").clicked() { ui.close_menu(); }
                    if ui.button("ディスクフォーマット").clicked() { ui.close_menu(); }
                    if ui.button("チェックディスク").clicked() { ui.close_menu(); }
                    ui.separator();
                    ui.menu_button("ディスクイメージの作成と復元", |ui| {
                        if ui.button("イメージ作成").clicked() { ui.close_menu(); }
                        if ui.button("イメージ復元").clicked() { ui.close_menu(); }
                    });
                    ui.separator();
                    ui.menu_button("ネットワークの接続と切断", |ui| {
                        if ui.button("ネットワーク接続").clicked() { ui.close_menu(); }
                        if ui.button("ネットワーク切断").clicked() { ui.close_menu(); }
                    });
                });

                // フォルダメニュー
                ui.menu_button("フォルダ", |ui| {
                    if ui.button("ホーム").clicked() {
                        if let Ok(home_dir) = std::env::home_dir().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Home directory not found")) {
                            self.navigate_to(home_dir);
                        }
                        ui.close_menu();
                    }
                    if ui.button("指定のフォルダを開く").clicked() { ui.close_menu(); }
                    if ui.button("検索").clicked() { ui.close_menu(); }
                    ui.separator();
                    if ui.button("システムフォルダ").clicked() { ui.close_menu(); }
                    if ui.button("ごみ箱を空にする").clicked() { ui.close_menu(); }
                    ui.separator();
                    if ui.button("フォルダの同期").clicked() { ui.close_menu(); }
                    if ui.button("フォルダの同期スクリプト").clicked() { ui.close_menu(); }
                    ui.separator();
                    if ui.button("履歴").clicked() { ui.close_menu(); }
                });

                // 表示メニュー
                ui.menu_button("表示", |ui| {
                    ui.menu_button("パネル・バー", |ui| {
                        if ui.button("ツールバー").clicked() { ui.close_menu(); }
                        if ui.button("ステータスバー").clicked() { ui.close_menu(); }
                        if ui.button("アドレスバー").clicked() { ui.close_menu(); }
                    });
                    ui.menu_button("表示する種類やプロパティ", |ui| {
                        ui.radio_value(&mut self.state.view_mode, ViewMode::List, "リスト");
                        ui.radio_value(&mut self.state.view_mode, ViewMode::Grid, "グリッド");
                        ui.radio_value(&mut self.state.view_mode, ViewMode::Details, "詳細");
                    });
                    ui.menu_button("ソート方法", |ui| {
                        if ui.button("名前順").clicked() {
                            self.state.sort_column = SortColumn::Name;
                            ui.close_menu();
                        }
                        if ui.button("サイズ順").clicked() {
                            self.state.sort_column = SortColumn::Size;
                            ui.close_menu();
                        }
                        if ui.button("日付順").clicked() {
                            self.state.sort_column = SortColumn::Modified;
                            ui.close_menu();
                        }
                        if ui.button("種類順").clicked() {
                            self.state.sort_column = SortColumn::Type;
                            ui.close_menu();
                        }
                    });
                    ui.separator();
                    if ui.button("アイコンの表示").clicked() { ui.close_menu(); }
                    if ui.button("サムネイルサイズ").clicked() { ui.close_menu(); }
                    if ui.button("基本の表示スタイルを更新").clicked() { ui.close_menu(); }
                    ui.separator();
                    if ui.button("ファイル情報").clicked() { ui.close_menu(); }
                    if ui.button("ファイル容量の詳細").clicked() { ui.close_menu(); }
                    if ui.button("ドライブ情報").clicked() { ui.close_menu(); }
                    ui.separator();
                    ui.checkbox(&mut self.state.show_hidden, "隠しファイルを表示");
                    ui.separator();
                    if ui.button("ホットキーメニュー").clicked() { ui.close_menu(); }
                    if ui.button("ショートカットキー").clicked() { 
                        self.state.show_shortcuts_dialog = true;
                        ui.close_menu(); 
                    }
                    if ui.button("デスクトップをツリー表示").clicked() { ui.close_menu(); }
                });

                // ツールメニュー
                ui.menu_button("ツール", |ui| {
                    if ui.button("ファイル閲覧").clicked() { 
                        if let Some(selected_file) = self.state.selected_items.first().cloned() {
                            if selected_file.is_file() {
                                FileViewerUI::open_file_for_viewing(self, selected_file);
                            }
                        }
                        ui.close_menu(); 
                    }
                    if ui.button("バイナリ編集").clicked() { ui.close_menu(); }
                    if ui.button("エディタで編集").clicked() { 
                        if let Some(selected_file) = self.state.selected_items.first().cloned() {
                            if selected_file.is_file() {
                                FileViewerUI::open_file_for_editing(self, selected_file);
                            }
                        }
                        ui.close_menu(); 
                    }
                    ui.separator();
                    if ui.button("ファイルから文字列を検索").clicked() { ui.close_menu(); }
                    if ui.button("コマンドプロンプトを開く").clicked() { ui.close_menu(); }
                    ui.separator();
                    if ui.button("形式を指定してリスト出力").clicked() { ui.close_menu(); }
                    ui.separator();
                    if ui.button("ショートカットキー").clicked() { 
                        self.state.show_shortcuts_dialog = true;
                        ui.close_menu(); 
                    }
                    if ui.button("ショートカットメニュー").clicked() { ui.close_menu(); }
                    ui.separator();
                    if ui.button("オプション").clicked() { ui.close_menu(); }
                    if ui.button("シェル拡張機能の設定").clicked() { ui.close_menu(); }
                });

                // ウィンドウメニュー
                ui.menu_button("ウィンドウ", |ui| {
                    if ui.button("最新の情報に更新").clicked() {
                        self.directory_cache.remove(&self.state.current_path);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("新しいウィンドウ").clicked() { ui.close_menu(); }
                    if ui.button("新しいタブ").clicked() { ui.close_menu(); }
                    if ui.button("最近開いたタブ").clicked() { ui.close_menu(); }
                    ui.separator();
                    if ui.button("タブグループ").clicked() { ui.close_menu(); }
                    if ui.button("タブを分離").clicked() { ui.close_menu(); }
                    ui.separator();
                    if ui.button("前の・次のタブ").clicked() { ui.close_menu(); }
                    if ui.button("他のタブを閉じる").clicked() { ui.close_menu(); }
                    ui.separator();
                    ui.menu_button("並べて表示・他", |ui| {
                        if ui.button("縦に並べて表示").clicked() { ui.close_menu(); }
                        if ui.button("横に並べて表示").clicked() { ui.close_menu(); }
                        if ui.button("重ねて表示").clicked() { ui.close_menu(); }
                        if ui.button("最小化").clicked() { ui.close_menu(); }
                    });
                    ui.separator();
                    ui.menu_button("全てのウィンドウ位置を保存・復帰", |ui| {
                        if ui.button("位置を保存").clicked() { ui.close_menu(); }
                        if ui.button("位置を復帰").clicked() { ui.close_menu(); }
                    });
                    ui.separator();
                    if ui.button("前の・次のウィンドウ").clicked() { ui.close_menu(); }
                });
            });
        });

        // ツールバー
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // ナビゲーションボタン
                let back_enabled = self.state.history_position > 0;
                let forward_enabled = self.state.history_position < self.state.navigation_history.len().saturating_sub(1);
                
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

        // サイドパネル（エクスプローラーツリー）
        egui::SidePanel::left("folder_tree")
            .default_width(self.state.sidebar_width)
            .resizable(true)
            .show(ctx, |ui| {
                let response = ExplorerTreeUI::show_explorer_tree(ui, self, ctx);
                
                // サイドペインクリック時にアクティブ化
                if response.clicked() {
                    self.state.active_pane = ActivePane::LeftSidebar;
                }
            });

        // ファイル閲覧・編集パネル（右側）
        if self.state.show_file_viewer {
            egui::SidePanel::right("file_viewer_panel")
                .resizable(true)
                .default_width(self.state.file_viewer_width)
                .width_range(300.0..=800.0)
                .show(ctx, |ui| {
                    FileViewerUI::show_file_viewer(ui, self);
                });
        }

        // メイン表示エリア（ファイルリスト）
        egui::CentralPanel::default().show(ctx, |ui| {
            // 表示するディレクトリを決定（左ペインの選択があればそれを使用、なければ現在のパス）
            let display_path = self.state.sidebar_selected_item
                .as_ref()
                .unwrap_or(&self.state.current_path)
                .clone();
            
            let search_query = self.state.search_query.clone();
            
            // entriesをクローンして所有権を取得し、借用の問題を回避
            let entries = match self.load_directory(&display_path) {
                Ok(entries) => entries.clone(),
                Err(error_msg) => {
                    ui.vertical_centered(|ui| {
                        ui.add_space(50.0);
                        ui.colored_label(egui::Color32::RED, "❌ ディレクトリアクセスエラー");
                        ui.label(&error_msg);
                        ui.add_space(10.0);
                        
                        ui.horizontal(|ui| {
                            if ui.button("再試行").clicked() {
                                self.directory_cache.remove(&display_path);
                            }
                            if ui.button("ホームに戻る").clicked() {
                                if let Ok(home_dir) = std::env::home_dir().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Home directory not found")) {
                                    self.navigate_to(home_dir);
                                }
                            }
                            if ui.button("Cドライブに移動").clicked() {
                                self.navigate_to(PathBuf::from("C:\\"));
                            }
                        });
                        
                        ui.add_space(10.0);
                        ui.colored_label(egui::Color32::GRAY, "💡 ヒント: パスが存在するか、アクセス権限があるか確認してください");
                    });
                    return;
                }
            };
            
            // 親ディレクトリエントリを作成（ルートでない場合）
            let mut all_entries = Vec::new();
            if display_path.parent().is_some() {
                let parent_entry = FileEntry {
                    name: "..".to_string(),
                    path: display_path.parent().unwrap().to_path_buf(),
                    size: 0,
                    is_dir: true,
                    created: chrono::DateTime::from(std::time::SystemTime::UNIX_EPOCH),
                    modified: chrono::DateTime::from(std::time::SystemTime::UNIX_EPOCH),
                    extension: None,
                };
                all_entries.push(parent_entry);
            }
            
            // エントリをソート：フォルダーを先に、その後ファイル
            let mut sorted_entries = entries.clone();
            sorted_entries.sort_by(|a, b| {
                match (a.is_dir, b.is_dir) {
                    (true, false) => std::cmp::Ordering::Less,   // フォルダーが先
                    (false, true) => std::cmp::Ordering::Greater, // ファイルが後
                    _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()), // 同じ種類なら名前順
                }
            });
            
            // ソートされたエントリを追加
            all_entries.extend(sorted_entries);
            
            let filtered_entries: Vec<&FileEntry> = all_entries
                .iter()
                .filter(|entry| {
                    search_query.is_empty() ||
                    entry.name.to_lowercase().contains(&search_query.to_lowercase())
                })
                .collect();

            // 借用チェッカー対応：必要な値をコピー
            let view_mode = self.state.view_mode.clone();
            let display_path_for_ui = display_path.clone();
            
            // ナビゲーション用の一時的な変数
            let mut navigation_target: Option<PathBuf> = None;
            let mut file_open_target: Option<PathBuf> = None;
            let mut activate_main_pane = false;
            let mut update_sidebar_selection: Option<PathBuf> = None;
            
            {
                let mut navigate_callback = |path: PathBuf| {
                    navigation_target = Some(path.clone());
                    update_sidebar_selection = Some(path);
                };
                
                let mut file_open_callback = |path: PathBuf| {
                    file_open_target = Some(path);
                };
                
                let mut pane_activate_callback = || {
                    activate_main_pane = true;
                };

                FileListUI::show_file_list(
                    ui,
                    &filtered_entries,
                    view_mode,
                    &display_path_for_ui,
                    &mut self.state.selected_items,
                    &mut self.state.last_selected_index,
                    &mut self.state.sort_column,
                    &mut self.state.sort_ascending,
                    &mut self.directory_cache,
                    &mut navigate_callback,
                    &mut file_open_callback,
                    &self.state.active_pane,
                    &mut pane_activate_callback,
                );
            }
            
            // サイドバー選択の更新
            if let Some(path) = update_sidebar_selection {
                self.state.sidebar_selected_item = Some(path);
            }
            
            // ペインアクティブ化
            if activate_main_pane {
                self.state.active_pane = ActivePane::MainList;
            }
            
            // ナビゲーションの実行
            if let Some(target) = navigation_target {
                self.navigate_to(target);
            }
            
            // ファイル閲覧の実行
            if let Some(target) = file_open_target {
                // 圧縮ファイルかどうかをチェック
                if crate::archive::ArchiveHandler::is_archive(&target) {
                    tracing::info!("圧縮ファイルビューアを表示: {:?}", target);
                    self.show_archive_viewer(target);
                } else {
                    FileViewerUI::open_file_for_viewing(self, target);
                }
            }
        });

        // ステータスバー
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("📁 {}", self.state.current_path.display()));
                ui.separator();
                
                // 借用チェッカー対応：パスをコピーしてentriesをクローン
                let current_path = self.state.current_path.clone();
                if let Ok(entries) = self.load_directory(&current_path) {
                    let entries = entries.clone();
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

        // 削除確認ダイアログ
        let mut delete_requested = false;
        let mut cancel_requested = false;
        
        {
            let mut delete_callback = || {
                delete_requested = true;
            };
            let mut cancel_callback = || {
                cancel_requested = true;
            };

            DialogsUI::show_delete_dialog(
                ctx,
                &mut self.state.show_delete_dialog,
                &self.state.delete_dialog_items,
                &self.state.current_path,
                &mut delete_callback,
                &mut cancel_callback,
            );
        }

        // ショートカットキー一覧ダイアログ
        DialogsUI::show_shortcuts_dialog(ctx, &mut self.state.show_shortcuts_dialog);

        // 新規ファイル作成ダイアログ
        let mut create_file_requested = false;
        let mut cancel_create_file_requested = false;
        let mut created_file_name = String::new();
        
        if self.state.show_create_file_dialog {
            let mut create_callback = |file_name: &str| {
                create_file_requested = true;
                created_file_name = file_name.to_string();
            };
            let mut cancel_callback = || {
                cancel_create_file_requested = true;
            };
            
            DialogsUI::show_create_file_dialog(
                ctx,
                &mut self.state.show_create_file_dialog,
                &mut self.state.new_file_name,
                &mut create_callback,
                &mut cancel_callback,
            );
        }

        // 新規フォルダ作成ダイアログ
        let mut create_folder_requested = false;
        let mut cancel_create_folder_requested = false;
        let mut created_folder_name = String::new();
        
        if self.state.show_create_folder_dialog {
            let mut create_callback = |folder_name: &str| {
                create_folder_requested = true;
                created_folder_name = folder_name.to_string();
            };
            let mut cancel_callback = || {
                cancel_create_folder_requested = true;
            };
            
            DialogsUI::show_create_folder_dialog(
                ctx,
                &mut self.state.show_create_folder_dialog,
                &mut self.state.new_folder_name,
                &mut create_callback,
                &mut cancel_callback,
            );
        }

        // 未保存変更確認ダイアログ
        let mut save_requested = false;
        let mut discard_requested = false;
        let mut cancel_requested_unsaved = false;
        
        if self.state.show_unsaved_dialog {
            let file_name = self.state.viewed_file_path
                .as_ref()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("不明なファイル");
                
            let mut save_callback = || { save_requested = true; };
            let mut discard_callback = || { discard_requested = true; };
            let mut cancel_callback = || { cancel_requested_unsaved = true; };
            
            DialogsUI::show_unsaved_changes_dialog(
                ctx,
                &mut self.state.show_unsaved_dialog,
                file_name,
                &mut save_callback,
                &mut discard_callback,
                &mut cancel_callback,
            );
        }

        // 圧縮ファイル関連ダイアログ
        DialogsUI::show_unpack_dialog(ctx, self);
        DialogsUI::show_pack_dialog(ctx, self);
        DialogsUI::show_archive_viewer(ctx, self);
        
        // リネームダイアログ
        DialogsUI::show_rename_dialog(ctx, self);
        
        // ファイル情報ダイアログ
        FileInfoDialog::show(ctx, self);
        
        // ダイアログアクションの実行
        if delete_requested {
            self.delete_selected_files();
        }
        if cancel_requested {
            self.state.show_delete_dialog = false;
            self.state.delete_dialog_items.clear();
        }
        
        // 未保存変更ダイアログのアクション実行
        if save_requested {
            FileViewerUI::save_and_close_file_viewer(self);
        }
        if discard_requested {
            FileViewerUI::force_close_file_viewer(self);
        }
        if cancel_requested_unsaved {
            self.state.show_unsaved_dialog = false;
            self.state.pending_close_action = false;
        }
        
        // 新規ファイル作成ダイアログのアクション実行
        if create_file_requested {
            self.create_new_file(&created_file_name);
        }
        if cancel_create_file_requested {
            self.state.show_create_file_dialog = false;
            self.state.new_file_name.clear();
        }
        
        // 新規フォルダ作成ダイアログのアクション実行
        if create_folder_requested {
            self.create_new_folder_dialog(&created_folder_name);
        }
        if cancel_create_folder_requested {
            self.state.show_create_folder_dialog = false;
            self.state.new_folder_name.clear();
        }

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

fn main() -> Result<(), eframe::Error> {
    eframe::run_native(
        "FVRS File Manager",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1200.0, 800.0])
                .with_min_inner_size([800.0, 600.0]),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(FileVisorApp::new(cc)))),
    )
} 