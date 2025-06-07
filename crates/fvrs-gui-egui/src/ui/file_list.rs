use std::path::Path;
use egui::{Color32, Layout, Align};
use egui_extras::{TableBuilder, Column};
use fvrs_core::core::FileEntry;
use crate::state::{ViewMode, SortColumn};
use crate::utils::{format_file_size, format_time};

pub struct FileListUI;

impl FileListUI {
    /// ファイルリスト表示のメイン関数
    pub fn show_file_list(
        ui: &mut egui::Ui,
        entries: &[&FileEntry],
        view_mode: ViewMode,
        current_path: &Path,
        selected_items: &mut Vec<std::path::PathBuf>,
        last_selected_index: &mut Option<usize>,
        sort_column: &mut SortColumn,
        sort_ascending: &mut bool,
        directory_cache: &mut std::collections::HashMap<std::path::PathBuf, Vec<FileEntry>>,
        navigate_callback: &mut dyn FnMut(std::path::PathBuf),
    ) {
        match view_mode {
            ViewMode::Details => Self::show_details_view(
                ui, entries, current_path, selected_items, last_selected_index,
                sort_column, sort_ascending, directory_cache, navigate_callback
            ),
            ViewMode::List => Self::show_list_view(
                ui, entries, current_path, selected_items, last_selected_index, navigate_callback
            ),
            ViewMode::Grid => Self::show_grid_view(
                ui, entries, current_path, selected_items, last_selected_index, navigate_callback
            ),
        }
    }

    /// 詳細ビュー
    fn show_details_view(
        ui: &mut egui::Ui,
        entries: &[&FileEntry],
        current_path: &Path,
        selected_items: &mut Vec<std::path::PathBuf>,
        last_selected_index: &mut Option<usize>,
        sort_column: &mut SortColumn,
        sort_ascending: &mut bool,
        directory_cache: &mut std::collections::HashMap<std::path::PathBuf, Vec<FileEntry>>,
        navigate_callback: &mut dyn FnMut(std::path::PathBuf),
    ) {
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
                    if ui.button(if *sort_column == SortColumn::Name {
                        if *sort_ascending { "名前 ▲" } else { "名前 ▼" }
                    } else { "名前" }).clicked() {
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
                    if ui.button(if *sort_column == SortColumn::Size {
                        if *sort_ascending { "サイズ ▲" } else { "サイズ ▼" }
                    } else { "サイズ" }).clicked() {
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
                    if ui.button(if *sort_column == SortColumn::Modified {
                        if *sort_ascending { "更新日時 ▲" } else { "更新日時 ▼" }
                    } else { "更新日時" }).clicked() {
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
                    if ui.button(if *sort_column == SortColumn::Type {
                        if *sort_ascending { "種類 ▲" } else { "種類 ▼" }
                    } else { "種類" }).clicked() {
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
                        let name_response = ui.selectable_label(is_selected, &entry.name);
                        if name_response.double_clicked() {
                            if entry.is_dir {
                                navigate_callback(entry_path.clone());
                            } else {
                                // ファイルを開く
                                if let Err(e) = open::that(&entry_path) {
                                    tracing::error!("ファイルオープンエラー: {:?}", e);
                                }
                            }
                        }
                        if name_response.clicked() {
                            let modifiers = ui.input(|i| i.modifiers.clone());
                            
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
    fn show_list_view(
        ui: &mut egui::Ui,
        entries: &[&FileEntry],
        current_path: &Path,
        selected_items: &mut Vec<std::path::PathBuf>,
        last_selected_index: &mut Option<usize>,
        navigate_callback: &mut dyn FnMut(std::path::PathBuf),
    ) {
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
                            if let Err(e) = open::that(&entry_path) {
                                tracing::error!("ファイルオープンエラー: {:?}", e);
                            }
                        }
                    }
                    
                    if response.clicked() {
                        let modifiers = ui.input(|i| i.modifiers.clone());
                        
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
    fn show_grid_view(
        ui: &mut egui::Ui,
        entries: &[&FileEntry],
        current_path: &Path,
        selected_items: &mut Vec<std::path::PathBuf>,
        last_selected_index: &mut Option<usize>,
        navigate_callback: &mut dyn FnMut(std::path::PathBuf),
    ) {
        const ITEM_SIZE: f32 = 80.0;
        const SPACING: f32 = 10.0;
        
        egui::ScrollArea::vertical().show(ui, |ui| {
            let available_width = ui.available_width();
            let items_per_row = ((available_width + SPACING) / (ITEM_SIZE + SPACING)).max(1.0) as usize;
            
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
                                let name_response = ui.add(
                                    egui::Label::new(&entry.name)
                                        .selectable(false)
                                        .wrap()
                                );
                                
                                if is_selected {
                                    let rect = icon_response.rect.union(name_response.rect);
                                    ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(2.0, Color32::BLUE), egui::StrokeKind::Outside);
                                }
                                
                                if icon_response.double_clicked() || name_response.double_clicked() {
                                    if entry.is_dir {
                                        navigate_callback(entry_path.clone());
                                    } else {
                                        if let Err(e) = open::that(&entry_path) {
                                            tracing::error!("ファイルオープンエラー: {:?}", e);
                                        }
                                    }
                                }
                                
                                if icon_response.clicked() || name_response.clicked() {
                                    let modifiers = ui.input(|i| i.modifiers.clone());
                                    
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
                            }
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