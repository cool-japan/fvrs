use crate::state::{ActivePane, SortColumn, ViewMode};
use crate::utils::{format_file_size, format_time};
use egui::{Align, Color32, Layout, Stroke};
use egui_extras::{Column, TableBuilder};
use fvrs_core::core::FileEntry;
use std::path::Path;

pub struct FileListUI;

/// `show_file_list` に渡すパラメータ一式(引数過多を避けるための束ね構造体)
pub struct FileListParams<'a> {
    /// 表示対象のエントリ(フィルタ済み)
    pub entries: &'a [&'a FileEntry],
    /// 現在のビューモード
    pub view_mode: ViewMode,
    /// 現在表示中のディレクトリ
    pub current_path: &'a Path,
    /// 選択中アイテムのパス集合
    pub selected_items: &'a mut Vec<std::path::PathBuf>,
    /// 直近に選択した行インデックス(Shift 範囲選択用)
    pub last_selected_index: &'a mut Option<usize>,
    /// ソート対象カラム
    pub sort_column: &'a mut SortColumn,
    /// 昇順ソートかどうか
    pub sort_ascending: &'a mut bool,
    /// ディレクトリ一覧キャッシュ(ソート変更時に無効化する)
    pub directory_cache: &'a mut std::collections::HashMap<std::path::PathBuf, Vec<FileEntry>>,
    /// ディレクトリ移動コールバック
    pub navigate_callback: &'a mut dyn FnMut(std::path::PathBuf),
    /// ファイルオープンコールバック
    pub file_open_callback: &'a mut dyn FnMut(std::path::PathBuf),
    /// 現在アクティブなペイン
    pub active_pane: &'a ActivePane,
    /// メインペインのアクティブ化コールバック
    pub pane_activate_callback: &'a mut dyn FnMut(),
}

/// 各ビュー(詳細/リスト/グリッド)共通の描画パラメータ
struct ViewParams<'a> {
    entries: &'a [&'a FileEntry],
    current_path: &'a Path,
    selected_items: &'a mut Vec<std::path::PathBuf>,
    last_selected_index: &'a mut Option<usize>,
    navigate_callback: &'a mut dyn FnMut(std::path::PathBuf),
    file_open_callback: &'a mut dyn FnMut(std::path::PathBuf),
}

impl FileListUI {
    /// ファイルリスト表示のメイン関数
    pub fn show_file_list(ui: &mut egui::Ui, params: FileListParams<'_>) {
        let FileListParams {
            entries,
            view_mode,
            current_path,
            selected_items,
            last_selected_index,
            sort_column,
            sort_ascending,
            directory_cache,
            navigate_callback,
            file_open_callback,
            active_pane,
            pane_activate_callback,
        } = params;

        let is_active = *active_pane == ActivePane::MainList;

        // ペイン全体にフレームを適用してアクティブ状態を視覚化
        let frame = egui::Frame::default().stroke(if is_active {
            Stroke::new(2.0, Color32::from_rgb(0, 120, 215)) // 青い枠
        } else {
            Stroke::new(1.0, Color32::GRAY) // グレーの枠
        });

        let view = ViewParams {
            entries,
            current_path,
            selected_items,
            last_selected_index,
            navigate_callback,
            file_open_callback,
        };

        let response = frame.show(ui, |ui| match view_mode {
            ViewMode::Details => {
                Self::show_details_view(ui, view, sort_column, sort_ascending, directory_cache)
            }
            ViewMode::List => Self::show_list_view(ui, view),
            ViewMode::Grid => Self::show_grid_view(ui, view),
        });

        // フレームがクリックされたらペインをアクティブ化
        if response.response.clicked() {
            pane_activate_callback();
        }
    }

    /// 詳細ビュー
    fn show_details_view(
        ui: &mut egui::Ui,
        view: ViewParams<'_>,
        sort_column: &mut SortColumn,
        sort_ascending: &mut bool,
        directory_cache: &mut std::collections::HashMap<std::path::PathBuf, Vec<FileEntry>>,
    ) {
        let ViewParams {
            entries,
            current_path,
            selected_items,
            last_selected_index,
            navigate_callback,
            file_open_callback,
        } = view;

        let table = TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(Layout::left_to_right(Align::Center))
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
                    if ui
                        .button(if *sort_column == SortColumn::Name {
                            if *sort_ascending {
                                "名前 ▲"
                            } else {
                                "名前 ▼"
                            }
                        } else {
                            "名前"
                        })
                        .clicked()
                    {
                        if *sort_column == SortColumn::Name {
                            *sort_ascending = !*sort_ascending;
                        } else {
                            *sort_column = SortColumn::Name;
                            *sort_ascending = true;
                        }
                        directory_cache.remove(&current_path.to_path_buf());
                    }
                });
                header.col(|ui| {
                    if ui
                        .button(if *sort_column == SortColumn::Size {
                            if *sort_ascending {
                                "サイズ ▲"
                            } else {
                                "サイズ ▼"
                            }
                        } else {
                            "サイズ"
                        })
                        .clicked()
                    {
                        if *sort_column == SortColumn::Size {
                            *sort_ascending = !*sort_ascending;
                        } else {
                            *sort_column = SortColumn::Size;
                            *sort_ascending = true;
                        }
                        directory_cache.remove(&current_path.to_path_buf());
                    }
                });
                header.col(|ui| {
                    if ui
                        .button(if *sort_column == SortColumn::Modified {
                            if *sort_ascending {
                                "更新日時 ▲"
                            } else {
                                "更新日時 ▼"
                            }
                        } else {
                            "更新日時"
                        })
                        .clicked()
                    {
                        if *sort_column == SortColumn::Modified {
                            *sort_ascending = !*sort_ascending;
                        } else {
                            *sort_column = SortColumn::Modified;
                            *sort_ascending = true;
                        }
                        directory_cache.remove(&current_path.to_path_buf());
                    }
                });
                header.col(|ui| {
                    if ui
                        .button(if *sort_column == SortColumn::Type {
                            if *sort_ascending {
                                "種類 ▲"
                            } else {
                                "種類 ▼"
                            }
                        } else {
                            "種類"
                        })
                        .clicked()
                    {
                        if *sort_column == SortColumn::Type {
                            *sort_ascending = !*sort_ascending;
                        } else {
                            *sort_column = SortColumn::Type;
                            *sort_ascending = true;
                        }
                        directory_cache.remove(&current_path.to_path_buf());
                    }
                });
            })
            .body(|body| {
                body.rows(20.0, entries.len(), |mut row| {
                    let row_index = row.index();
                    let entry = entries[row_index];
                    let entry_path = current_path.join(&entry.name);
                    let is_selected = selected_items.contains(&entry_path);

                    row.col(|ui| {
                        ui.label(if entry.is_dir { "📁" } else { "📄" });
                    });

                    row.col(|ui| {
                        // 「..」エントリの特別な表示
                        let display_name = if entry.name == ".." {
                            "📁 上へ".to_string()
                        } else {
                            entry.name.clone()
                        };

                        let name_response = ui.selectable_label(is_selected, display_name);
                        if name_response.double_clicked() {
                            if entry.name == ".." {
                                // 親ディレクトリに移動
                                if let Some(parent) = current_path.parent() {
                                    navigate_callback(parent.to_path_buf());
                                }
                            } else if entry.is_dir {
                                navigate_callback(entry_path.clone());
                            } else {
                                // ファイルを閲覧モードで開く
                                file_open_callback(entry_path.clone());
                            }
                        }
                        if name_response.clicked() {
                            let modifiers = ui.input(|i| i.modifiers);

                            if modifiers.shift {
                                // Shift+クリック: 範囲選択
                                if let Some(last_idx) = *last_selected_index {
                                    let start_idx = last_idx.min(row_index);
                                    let end_idx = last_idx.max(row_index);

                                    selected_items.clear();
                                    for idx in start_idx..=end_idx {
                                        if idx < entries.len() {
                                            let target_entry = entries[idx];
                                            let target_path = current_path.join(&target_entry.name);
                                            selected_items.push(target_path);
                                        }
                                    }
                                } else {
                                    // 最初の選択
                                    selected_items.clear();
                                    selected_items.push(entry_path.clone());
                                    *last_selected_index = Some(row_index);
                                }
                            } else if modifiers.ctrl {
                                // Ctrl+クリック: 個別選択
                                if is_selected {
                                    selected_items.retain(|p| p != &entry_path);
                                } else {
                                    selected_items.push(entry_path.clone());
                                }
                                *last_selected_index = Some(row_index);
                            } else {
                                // 通常クリック: 単一選択
                                selected_items.clear();
                                selected_items.push(entry_path.clone());
                                *last_selected_index = Some(row_index);
                            }
                        }
                    });

                    row.col(|ui| {
                        if entry.is_dir {
                            ui.label("―");
                        } else {
                            ui.label(format_file_size(entry.size));
                        }
                    });

                    row.col(|ui| {
                        ui.label(format_time(entry.modified));
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

    /// リストビュー
    fn show_list_view(ui: &mut egui::Ui, view: ViewParams<'_>) {
        let ViewParams {
            entries,
            current_path,
            selected_items,
            last_selected_index,
            navigate_callback,
            file_open_callback,
        } = view;

        egui::ScrollArea::vertical().show(ui, |ui| {
            for (row_index, entry) in entries.iter().enumerate() {
                let entry_path = current_path.join(&entry.name);
                let is_selected = selected_items.contains(&entry_path);

                ui.horizontal(|ui| {
                    ui.label(if entry.is_dir { "📁" } else { "📄" });

                    let response = ui.selectable_label(is_selected, &entry.name);

                    if response.double_clicked() {
                        if entry.is_dir {
                            navigate_callback(entry_path.clone());
                        } else {
                            file_open_callback(entry_path.clone());
                        }
                    }

                    if response.clicked() {
                        let modifiers = ui.input(|i| i.modifiers);

                        if modifiers.shift {
                            // Shift+クリック: 範囲選択
                            if let Some(last_idx) = *last_selected_index {
                                let start_idx = last_idx.min(row_index);
                                let end_idx = last_idx.max(row_index);

                                selected_items.clear();
                                for idx in start_idx..=end_idx {
                                    if idx < entries.len() {
                                        let target_entry = entries[idx];
                                        let target_path = current_path.join(&target_entry.name);
                                        selected_items.push(target_path);
                                    }
                                }
                            } else {
                                // 最初の選択
                                selected_items.clear();
                                selected_items.push(entry_path.clone());
                                *last_selected_index = Some(row_index);
                            }
                        } else if modifiers.ctrl {
                            // Ctrl+クリック: 個別選択
                            if is_selected {
                                selected_items.retain(|p| p != &entry_path);
                            } else {
                                selected_items.push(entry_path.clone());
                            }
                            *last_selected_index = Some(row_index);
                        } else {
                            // 通常クリック: 単一選択
                            selected_items.clear();
                            selected_items.push(entry_path.clone());
                            *last_selected_index = Some(row_index);
                        }
                    }
                });
            }
        });
    }

    /// グリッドビュー
    fn show_grid_view(ui: &mut egui::Ui, view: ViewParams<'_>) {
        let ViewParams {
            entries,
            current_path,
            selected_items,
            last_selected_index,
            navigate_callback,
            file_open_callback,
        } = view;

        const ITEM_SIZE: f32 = 80.0;
        const SPACING: f32 = 10.0;

        egui::ScrollArea::vertical().show(ui, |ui| {
            let available_width = ui.available_width();
            let items_per_row =
                ((available_width + SPACING) / (ITEM_SIZE + SPACING)).max(1.0) as usize;

            let mut current_index = 0;
            for chunk in entries.chunks(items_per_row) {
                ui.horizontal(|ui| {
                    for (chunk_idx, entry) in chunk.iter().enumerate() {
                        let row_index = current_index + chunk_idx;
                        let entry_path = current_path.join(&entry.name);
                        let is_selected = selected_items.contains(&entry_path);

                        ui.allocate_ui_with_layout(
                            [ITEM_SIZE, ITEM_SIZE].into(),
                            Layout::top_down(Align::Center),
                            |ui| {
                                let icon = if entry.is_dir { "📁" } else { "📄" };

                                let icon_response = ui.button(icon);
                                let name_response =
                                    ui.add(egui::Label::new(&entry.name).selectable(false).wrap());

                                if is_selected {
                                    let rect = icon_response.rect.union(name_response.rect);
                                    ui.painter().rect_stroke(
                                        rect,
                                        2.0,
                                        egui::Stroke::new(2.0, Color32::BLUE),
                                        egui::StrokeKind::Outside,
                                    );
                                }

                                if icon_response.double_clicked() || name_response.double_clicked()
                                {
                                    if entry.is_dir {
                                        navigate_callback(entry_path.clone());
                                    } else {
                                        file_open_callback(entry_path.clone());
                                    }
                                }

                                if icon_response.clicked() || name_response.clicked() {
                                    let modifiers = ui.input(|i| i.modifiers);

                                    if modifiers.shift {
                                        // Shift+クリック: 範囲選択
                                        if let Some(last_idx) = *last_selected_index {
                                            let start_idx = last_idx.min(row_index);
                                            let end_idx = last_idx.max(row_index);

                                            selected_items.clear();
                                            for idx in start_idx..=end_idx {
                                                if idx < entries.len() {
                                                    let target_entry = entries[idx];
                                                    let target_path =
                                                        current_path.join(&target_entry.name);
                                                    selected_items.push(target_path);
                                                }
                                            }
                                        } else {
                                            // 最初の選択
                                            selected_items.clear();
                                            selected_items.push(entry_path.clone());
                                            *last_selected_index = Some(row_index);
                                        }
                                    } else if modifiers.ctrl {
                                        // Ctrl+クリック: 個別選択
                                        if is_selected {
                                            selected_items.retain(|p| p != &entry_path);
                                        } else {
                                            selected_items.push(entry_path.clone());
                                        }
                                        *last_selected_index = Some(row_index);
                                    } else {
                                        // 通常クリック: 単一選択
                                        selected_items.clear();
                                        selected_items.push(entry_path.clone());
                                        *last_selected_index = Some(row_index);
                                    }
                                }
                            },
                        );

                        ui.add_space(SPACING);
                    }
                });
                current_index += chunk.len();
                ui.add_space(SPACING);
            }
        });
    }
}
