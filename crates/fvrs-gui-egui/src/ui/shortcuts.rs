use crate::app::FileVisorApp;
use crate::state::ActivePane;
use egui::{Context, Key};

pub struct ShortcutHandler;

impl ShortcutHandler {
    /// ショートカットキーを処理する
    pub fn handle_shortcuts(app: &mut FileVisorApp, ctx: &Context) {
        // ダイアログが表示されている間はショートカットキーを無効にする
        if app.state.show_delete_dialog 
            || app.state.show_shortcuts_dialog 
            || app.state.show_create_file_dialog 
            || app.state.show_create_folder_dialog 
            || app.state.show_unsaved_dialog
            || app.state.show_unpack_dialog
            || app.state.show_pack_dialog
            || app.state.show_rename_dialog 
                 {
             return;
         }

        ctx.input(|i| {
            // 基本的なショートカット（ペインに関係なく動作）
            if i.key_pressed(Key::F5) {
                Self::refresh_directory(app);
            }
            if i.key_pressed(Key::Delete) && !app.state.selected_items.is_empty() {
                Self::delete_files(app);
            }
            if i.modifiers.alt && i.key_pressed(Key::ArrowLeft) {
                Self::go_back(app);
            }
            if i.modifiers.alt && i.key_pressed(Key::ArrowRight) {
                Self::go_forward(app);
            }
            
            // Tab キーでペイン切り替え
            if i.key_pressed(Key::Tab) && !i.modifiers.any() {
                Self::switch_pane(app);
            }
            
            // 矢印キーによるナビゲーション（アクティブペインでのみ）
            if app.state.active_pane == ActivePane::MainList {
                if i.key_pressed(Key::ArrowUp) {
                    Self::navigate_list_up(app);
                }
                if i.key_pressed(Key::ArrowDown) {
                    Self::navigate_list_down(app);
                }
            }

            // A～Zのワンタッチキー（修飾キーなしの場合のみ）
            if !i.modifiers.any() {
                // A: 属性変更
                if i.key_pressed(Key::A) {
                    Self::change_attributes(app);
                }
                // B: バイナリ編集
                if i.key_pressed(Key::B) {
                    Self::binary_edit(app);
                }
                // C: コピー
                if i.key_pressed(Key::C) {
                    Self::copy_files(app);
                }
                // D: 削除
                if i.key_pressed(Key::D) {
                    Self::delete_files(app);
                }
                // E: エディタで編集
                if i.key_pressed(Key::E) {
                    Self::edit_with_editor(app);
                }
                // F: 検索
                if i.key_pressed(Key::F) {
                    Self::find_files(app);
                }
                // G: 履歴
                if i.key_pressed(Key::G) {
                    Self::show_history(app);
                }
                // H: 連結
                if i.key_pressed(Key::H) {
                    Self::concatenate_files(app);
                }
                // I: ファイル情報
                if i.key_pressed(Key::I) {
                    Self::show_file_info(app);
                }
                // K: フォルダの作成
                if i.key_pressed(Key::K) {
                    Self::create_folder(app);
                }
                // L: フォルダを開く
                if i.key_pressed(Key::L) {
                    Self::open_folder(app);
                }
                // M: 移動
                if i.key_pressed(Key::M) {
                    Self::move_files(app);
                }
                // N: 新規ファイル作成
                if i.key_pressed(Key::N) {
                    Self::create_file(app);
                }
                // O: 開く
                if i.key_pressed(Key::O) {
                    Self::open_files(app);
                }
                // P: 圧縮書庫の作成
                if i.key_pressed(Key::P) {
                    Self::create_archive(app);
                }
                // Q: ホットキー
                if i.key_pressed(Key::Q) {
                    Self::show_hotkeys(app);
                }
                // R: 名前変更
                if i.key_pressed(Key::R) {
                    Self::rename_files(app);
                }
                // S: ソート条件の変更
                if i.key_pressed(Key::S) {
                    Self::change_sort(app);
                }
                // T: フルパス名をコピー
                if i.key_pressed(Key::T) {
                    Self::copy_full_path(app);
                }
                // U: 圧縮書庫の解凍
                if i.key_pressed(Key::U) {
                    Self::extract_archive(app);
                }
                // V: ファイル閲覧
                if i.key_pressed(Key::V) {
                    Self::view_files(app);
                }
                // W: フィルタ
                if i.key_pressed(Key::W) {
                    Self::set_filter(app);
                }
                // X: 実行
                if i.key_pressed(Key::X) {
                    Self::execute_files(app);
                }
                // Y: 関連付け
                if i.key_pressed(Key::Y) {
                    Self::show_associations(app);
                }
                // Z: 一括選択
                if i.key_pressed(Key::Z) {
                    Self::select_all(app);
                }
            }
        });

        if ctx.input(|i| i.key_pressed(Key::U)) {
            tracing::info!("解凍ダイアログを表示");
            app.show_unpack_dialog();
        }

        if ctx.input(|i| i.key_pressed(Key::P)) {
            tracing::info!("圧縮ダイアログを表示");
            app.show_pack_dialog();
        }

                    if ctx.input(|i| i.key_pressed(Key::V)) {
            if let Some(selected_path) = app.state.selected_items.first() {
                let full_path = selected_path.clone();
                if crate::archive::ArchiveHandler::is_archive(&full_path) {
                    tracing::info!("圧縮ファイルビューアを表示: {:?}", full_path);
                    app.show_archive_viewer(full_path);
                } else {
                    // app.open_file(full_path);
                }
            }
        }

        if ctx.input(|i| i.key_pressed(Key::R)) {
            tracing::info!("リネームダイアログを表示");
            app.show_rename_dialog();
        }
    }

    // ===== 基本操作 =====
    
    fn refresh_directory(app: &mut FileVisorApp) {
        app.directory_cache.remove(&app.state.current_path);
        tracing::info!("ディレクトリを更新しました");
    }

    fn go_back(app: &mut FileVisorApp) {
        app.go_back();
    }

    fn go_forward(app: &mut FileVisorApp) {
        app.go_forward();
    }
    
    fn switch_pane(app: &mut FileVisorApp) {
        app.state.active_pane = match app.state.active_pane {
            ActivePane::LeftSidebar => ActivePane::MainList,
            ActivePane::MainList => ActivePane::LeftSidebar,
        };
        tracing::info!("ペインを切り替えました: {:?}", app.state.active_pane);
    }
    
    fn navigate_list_up(app: &mut FileVisorApp) {
        if let Some(current_index) = app.state.last_selected_index {
            if current_index > 0 {
                app.state.last_selected_index = Some(current_index - 1);
                // 実際の選択を更新する処理が必要
                tracing::info!("リスト上へ移動: {}", current_index - 1);
            }
        } else if !app.state.selected_items.is_empty() {
            app.state.last_selected_index = Some(0);
        }
    }
    
    fn navigate_list_down(app: &mut FileVisorApp) {
        if let Some(current_index) = app.state.last_selected_index {
            app.state.last_selected_index = Some(current_index + 1);
            // 実際の選択を更新する処理が必要
            tracing::info!("リスト下へ移動: {}", current_index + 1);
        } else if app.state.selected_items.is_empty() {
            app.state.last_selected_index = Some(0);
        }
    }

    // ===== ファイル操作 =====

    fn copy_files(app: &mut FileVisorApp) {
        if !app.state.selected_items.is_empty() {
            // クリップボードに選択アイテムをコピー
            use crate::state::ClipboardOperation;
            app.state.clipboard = Some(ClipboardOperation::Copy(app.state.selected_items.clone()));
            tracing::info!("{}個のアイテムをコピーしました", app.state.selected_items.len());
        }
    }

    fn move_files(app: &mut FileVisorApp) {
        if !app.state.selected_items.is_empty() {
            // クリップボードに選択アイテムを切り取り
            use crate::state::ClipboardOperation;
            app.state.clipboard = Some(ClipboardOperation::Cut(app.state.selected_items.clone()));
            tracing::info!("{}個のアイテムを切り取りました", app.state.selected_items.len());
        }
    }

    fn delete_files(app: &mut FileVisorApp) {
        if !app.state.selected_items.is_empty() {
            app.show_delete_confirmation();
        }
    }

    fn rename_files(app: &mut FileVisorApp) {
        if app.state.selected_items.len() == 1 {
            tracing::info!("名前変更: {:?}", app.state.selected_items[0]);
            // TODO: 名前変更ダイアログを実装
        }
    }

    fn open_files(app: &mut FileVisorApp) {
        for path in &app.state.selected_items {
            if path.is_file() {
                if let Err(e) = open::that(path) {
                    tracing::error!("ファイルオープンエラー: {:?}", e);
                }
            } else if path.is_dir() {
                app.navigate_to(path.clone());
                break; // 最初のディレクトリのみ開く
            }
        }
    }

    fn execute_files(app: &mut FileVisorApp) {
        for path in &app.state.selected_items {
            if path.is_file() {
                if let Err(e) = open::that(path) {
                    tracing::error!("実行エラー: {:?}", e);
                }
            }
        }
    }

    // ===== 選択操作 =====

    fn select_all(app: &mut FileVisorApp) {
        let current_path = app.state.current_path.clone();
        if let Ok(entries) = app.load_directory(&current_path) {
            let entries = entries.clone(); // 借用の競合を避けるためにクローン
            app.state.selected_items.clear();
            for entry in entries {
                app.state.selected_items.push(entry.path.clone());
            }
            app.state.last_selected_index = None;
            tracing::info!("すべてのアイテムを選択しました");
        }
    }

    // ===== フォルダ操作 =====

    fn create_folder(app: &mut FileVisorApp) {
        app.state.show_create_folder_dialog = true;
        app.state.new_folder_name.clear();
        tracing::info!("新規フォルダ作成ダイアログを表示");
    }

    fn open_folder(app: &mut FileVisorApp) {
        if let Some(selected) = app.state.selected_items.first() {
            if selected.is_dir() {
                app.navigate_to(selected.clone());
            }
        }
    }

    // ===== ソート・表示 =====

    fn change_sort(app: &mut FileVisorApp) {
        use crate::state::SortColumn;
        app.state.sort_column = match app.state.sort_column {
            SortColumn::Name => SortColumn::Size,
            SortColumn::Size => SortColumn::Modified,
            SortColumn::Modified => SortColumn::Type,
            SortColumn::Type => SortColumn::Name,
        };
        app.directory_cache.remove(&app.state.current_path);
        tracing::info!("ソート条件を変更しました: {:?}", app.state.sort_column);
    }

    // ===== パス・情報 =====

    fn copy_full_path(app: &mut FileVisorApp) {
        if !app.state.selected_items.is_empty() {
            let paths: Vec<String> = app.state.selected_items
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect();
            let full_paths = paths.join("\n");
            
            // システムクリップボードにコピー
            #[cfg(feature = "clipboard")]
            {
                use arboard::Clipboard;
                if let Ok(mut clipboard) = Clipboard::new() {
                    let _ = clipboard.set_text(&full_paths);
                }
            }
            
            tracing::info!("フルパスをコピーしました: {}", paths.len());
        }
    }

    fn show_file_info(app: &mut FileVisorApp) {
        if let Some(selected) = app.state.selected_items.first() {
            app.state.file_info_target = Some(selected.clone());
            app.state.show_file_info_dialog = true;
            tracing::info!("ファイル情報ダイアログを表示: {:?}", selected);
        } else {
            tracing::warn!("ファイル情報を表示するファイルが選択されていません");
        }
    }

    // ===== まだ実装されていない機能（ログのみ） =====

    fn change_attributes(_app: &mut FileVisorApp) {
        tracing::info!("属性変更機能（未実装）");
    }

    fn binary_edit(_app: &mut FileVisorApp) {
        tracing::info!("バイナリ編集機能（未実装）");
    }

    fn edit_with_editor(app: &mut FileVisorApp) {
        if let Some(selected_file) = app.state.selected_items.first().cloned() {
            if selected_file.is_file() {
                use crate::ui::FileViewerUI;
                FileViewerUI::open_file_for_editing(app, selected_file.clone());
                tracing::info!("ファイル編集を開始: {:?}", selected_file);
            } else {
                tracing::warn!("選択されたアイテムはファイルではありません");
            }
        } else {
            tracing::warn!("編集するファイルが選択されていません");
        }
    }

    fn find_files(_app: &mut FileVisorApp) {
        tracing::info!("検索機能（未実装）");
    }

    fn show_history(_app: &mut FileVisorApp) {
        tracing::info!("履歴機能（未実装）");
    }

    fn concatenate_files(_app: &mut FileVisorApp) {
        tracing::info!("連結機能（未実装）");  
    }

    fn create_file(app: &mut FileVisorApp) {
        app.state.show_create_file_dialog = true;
        app.state.new_file_name.clear();
        tracing::info!("新規ファイル作成ダイアログを表示");
    }

    fn create_archive(_app: &mut FileVisorApp) {
        tracing::info!("圧縮書庫作成機能（未実装）");
    }

    fn extract_archive(_app: &mut FileVisorApp) {
        tracing::info!("圧縮書庫解凍機能（未実装）");
    }

    fn view_files(app: &mut FileVisorApp) {
        if let Some(selected_file) = app.state.selected_items.first().cloned() {
            if selected_file.is_file() {
                use crate::ui::FileViewerUI;
                FileViewerUI::open_file_for_viewing(app, selected_file.clone());
                tracing::info!("ファイル閲覧を開始: {:?}", selected_file);
            } else {
                tracing::warn!("選択されたアイテムはファイルではありません");
            }
        } else {
            tracing::warn!("閲覧するファイルが選択されていません");
        }
    }

    fn set_filter(_app: &mut FileVisorApp) {
        tracing::info!("フィルタ機能（未実装）");
    }

    fn show_associations(_app: &mut FileVisorApp) {
        tracing::info!("関連付け機能（未実装）");
    }

    fn show_hotkeys(app: &mut FileVisorApp) {
        app.state.show_shortcuts_dialog = true;
        tracing::info!("ショートカットキー一覧を表示");
    }

    pub fn get_shortcut_description() -> Vec<(&'static str, &'static str)> {
        vec![
            ("A-Z", "頭文字検索"),
            ("Enter", "開く"),
            ("Backspace", "親ディレクトリ"),
            ("Del", "削除"),
            ("F2", "名前変更"),
            ("F5", "更新"),
            ("H", "ヘルプ"),
            ("N", "新規ファイル作成"),
            ("K", "新規フォルダ作成"),
            ("U", "解凍 (Unpack)"),
            ("P", "圧縮 (Pack)"),
            ("V", "表示/圧縮ファイル中身"),
            ("R", "名前変更 (Rename)"),
            ("C", "複製"),
            ("M", "移動"),
            ("S", "圧縮書庫作成"),
        ]
    }
} 