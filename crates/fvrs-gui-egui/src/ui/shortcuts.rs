use crate::app::FileVisorApp;
use egui::{Context, Key};

pub struct ShortcutHandler;

impl ShortcutHandler {
    /// ショートカットキーを処理する
    pub fn handle_shortcuts(app: &mut FileVisorApp, ctx: &Context) {
        ctx.input(|i| {
            // 既存のショートカット
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
                // N: 分割
                if i.key_pressed(Key::N) {
                    Self::split_files(app);
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
        app.create_new_folder("新しいフォルダー");
        tracing::info!("新しいフォルダーを作成しました");
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
            tracing::info!("ファイル情報: {:?}", selected);
            // TODO: ファイル情報ダイアログを実装
        }
    }

    // ===== まだ実装されていない機能（ログのみ） =====

    fn change_attributes(_app: &mut FileVisorApp) {
        tracing::info!("属性変更機能（未実装）");
    }

    fn binary_edit(_app: &mut FileVisorApp) {
        tracing::info!("バイナリ編集機能（未実装）");
    }

    fn edit_with_editor(_app: &mut FileVisorApp) {
        tracing::info!("エディタで編集機能（未実装）");
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

    fn split_files(_app: &mut FileVisorApp) {
        tracing::info!("分割機能（未実装）");
    }

    fn create_archive(_app: &mut FileVisorApp) {
        tracing::info!("圧縮書庫作成機能（未実装）");
    }

    fn extract_archive(_app: &mut FileVisorApp) {
        tracing::info!("圧縮書庫解凍機能（未実装）");
    }

    fn view_files(_app: &mut FileVisorApp) {
        tracing::info!("ファイル閲覧機能（未実装）");
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
} 