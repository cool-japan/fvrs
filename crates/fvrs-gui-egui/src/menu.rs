use native_windows_gui as nwg;
use std::path::PathBuf;
use std::rc::Rc;
use std::cell::RefCell;
use crate::{MainWindow, Result};
use crate::file_ops::{self, FileOpError};
use fvrs_core::SortBy;

/// Context menu structure
#[derive(Default, NwgUi)]
pub struct ContextMenu {
    #[nwg_control(parent: window, popup: true)]
    menu: nwg::Menu,
    
    #[nwg_control(parent: menu, text: "Open", enabled: true)]
    open: nwg::MenuItem,
    
    #[nwg_control(parent: menu, text: "Open with...", enabled: true)]
    open_with: nwg::MenuItem,
    
    #[nwg_control(parent: menu, text: "", enabled: false)]
    separator1: nwg::MenuItem,
    
    #[nwg_control(parent: menu, text: "Copy\tCtrl+C", enabled: true)]
    copy: nwg::MenuItem,
    
    #[nwg_control(parent: menu, text: "Cut\tCtrl+X", enabled: true)]
    cut: nwg::MenuItem,
    
    #[nwg_control(parent: menu, text: "Paste\tCtrl+V", enabled: true)]
    paste: nwg::MenuItem,
    
    #[nwg_control(parent: menu, text: "", enabled: false)]
    separator2: nwg::MenuItem,
    
    #[nwg_control(parent: menu, text: "Delete\tDel", enabled: true)]
    delete: nwg::MenuItem,
    
    #[nwg_control(parent: menu, text: "Rename\tF2", enabled: true)]
    rename: nwg::MenuItem,
    
    #[nwg_control(parent: menu, text: "", enabled: false)]
    separator3: nwg::MenuItem,
    
    #[nwg_control(parent: menu, text: "Properties", enabled: true)]
    properties: nwg::MenuItem,
}

impl ContextMenu {
    pub fn new() -> Result<Self> {
        let mut menu = Self::default();
        menu.build()?;
        Ok(menu)
    }
    
    pub fn show(&self, x: i32, y: i32) {
        self.menu.show_popup_menu(x, y);
    }
}

/// Initialize the context menu
pub fn init_context_menu(window: &MainWindow) -> Result<ContextMenu> {
    let menu = ContextMenu::new()?;
    let window = Rc::new(RefCell::new(window.clone()));
    
    // Open
    let window_clone = window.clone();
    nwg::bind_event_handler(&menu.open.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnMenuItemSelected {
            let window = window_clone.borrow();
            if let Some(path) = window.get_selected_items().first() {
                if let Err(e) = file_ops::open_file(path) {
                    file_ops::show_error_dialog("Error", &format!("Failed to open file: {}", e));
                }
            }
        }
    })?;
    
    // Open with...
    let window_clone = window.clone();
    nwg::bind_event_handler(&menu.open_with.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnMenuItemSelected {
            let window = window_clone.borrow();
            if let Some(path) = window.get_selected_items().first() {
                let mut dialog = nwg::FileDialog::default();
                dialog.title("Open With");
                dialog.options(nwg::FileDialogOptions::FILE_MUST_EXIST);
                
                if dialog.show() {
                    if let Some(app) = dialog.selected_path() {
                        if let Err(e) = file_ops::open_file_with(path, &app) {
                            file_ops::show_error_dialog("Error", &format!("Failed to open file: {}", e));
                        }
                    }
                }
            }
        }
    })?;
    
    // Copy
    let window_clone = window.clone();
    nwg::bind_event_handler(&menu.copy.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnMenuItemSelected {
            let window = window_clone.borrow();
            let selected = window.get_selected_items();
            if !selected.is_empty() {
                let mut dialog = nwg::FileDialog::default();
                dialog.title("Copy To");
                dialog.options(nwg::FileDialogOptions::PICK_FOLDERS);
                
                if dialog.show() {
                    if let Some(dest) = dialog.selected_path() {
                        for source in selected {
                            if let Err(e) = file_ops::copy_file(&source, &dest) {
                                file_ops::show_error_dialog("Error", &format!("Failed to copy {}: {}", source.display(), e));
                                break;
                            }
                        }
                        window.update_list_view(&dest).unwrap_or_else(|e| {
                            file_ops::show_error_dialog("Error", &format!("Failed to update view: {}", e));
                        });
                    }
                }
            }
        }
    })?;
    
    // Cut
    let window_clone = window.clone();
    nwg::bind_event_handler(&menu.cut.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnMenuItemSelected {
            let window = window_clone.borrow();
            let selected = window.get_selected_items();
            if !selected.is_empty() {
                let mut dialog = nwg::FileDialog::default();
                dialog.title("Move To");
                dialog.options(nwg::FileDialogOptions::PICK_FOLDERS);
                
                if dialog.show() {
                    if let Some(dest) = dialog.selected_path() {
                        for source in selected {
                            if let Err(e) = file_ops::move_file(&source, &dest) {
                                file_ops::show_error_dialog("Error", &format!("Failed to move {}: {}", source.display(), e));
                                break;
                            }
                        }
                        window.update_list_view(&dest).unwrap_or_else(|e| {
                            file_ops::show_error_dialog("Error", &format!("Failed to update view: {}", e));
                        });
                    }
                }
            }
        }
    })?;
    
    // Paste
    let window_clone = window.clone();
    nwg::bind_event_handler(&menu.paste.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnMenuItemSelected {
            let window = window_clone.borrow();
            let current_path = window.current_path.borrow();
            if let Err(e) = file_ops::paste_files(&current_path) {
                file_ops::show_error_dialog("Error", &format!("Failed to paste files: {}", e));
            } else {
                window.update_list_view(&current_path).unwrap_or_else(|e| {
                    file_ops::show_error_dialog("Error", &format!("Failed to update view: {}", e));
                });
            }
        }
    })?;
    
    // Delete
    let window_clone = window.clone();
    nwg::bind_event_handler(&menu.delete.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnMenuItemSelected {
            let window = window_clone.borrow();
            let selected = window.get_selected_items();
            if !selected.is_empty() {
                let count = selected.len();
                if file_ops::show_confirm_dialog(
                    "Confirm Delete",
                    &format!("Are you sure you want to delete {} item{}?", count, if count == 1 { "" } else { "s" })
                ) {
                    for path in selected {
                        if let Err(e) = file_ops::delete_file(&path) {
                            file_ops::show_error_dialog("Error", &format!("Failed to delete {}: {}", path.display(), e));
                            break;
                        }
                    }
                    window.update_list_view(&window.current_path.borrow()).unwrap_or_else(|e| {
                        file_ops::show_error_dialog("Error", &format!("Failed to update view: {}", e));
                    });
                }
            }
        }
    })?;
    
    // Rename
    let window_clone = window.clone();
    nwg::bind_event_handler(&menu.rename.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnMenuItemSelected {
            let window = window_clone.borrow();
            if let Some(path) = window.get_selected_items().first() {
                if let Err(e) = file_ops::rename_file(path) {
                    file_ops::show_error_dialog("Error", &format!("Failed to rename file: {}", e));
                } else {
                    window.update_list_view(&window.current_path.borrow()).unwrap_or_else(|e| {
                        file_ops::show_error_dialog("Error", &format!("Failed to update view: {}", e));
                    });
                }
            }
        }
    })?;
    
    // Properties
    let window_clone = window.clone();
    nwg::bind_event_handler(&menu.properties.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnMenuItemSelected {
            let window = window_clone.borrow();
            if let Some(path) = window.get_selected_items().first() {
                if let Err(e) = file_ops::show_properties(path) {
                    file_ops::show_error_dialog("Error", &format!("Failed to show properties: {}", e));
                }
            }
        }
    })?;
    
    Ok(menu)
}

/// Initialize the file menu
pub fn init_file_menu(window: &MainWindow) -> Result<()> {
    let file_menu = &window.file_menu;
    let window = Rc::new(RefCell::new(window.clone()));
    
    // New window
    let new_window = nwg::MenuItem::new(file_menu, "New Window", true, None)?;
    let window_clone = window.clone();
    nwg::bind_event_handler(&new_window.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnMenuItemSelected {
            // TODO: Implement new window
        }
    })?;
    
    // Open
    let open = nwg::MenuItem::new(file_menu, "Open", true, None)?;
    let window_clone = window.clone();
    nwg::bind_event_handler(&open.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnMenuItemSelected {
            let mut dialog = nwg::FileDialog::default();
            dialog.title("Open File");
            dialog.options(nwg::FileDialogOptions::FILE_MUST_EXIST);
            
            if dialog.show() {
                if let Some(path) = dialog.selected_path() {
                    let window = window_clone.borrow();
                    window.update_list_view(&path).unwrap_or_else(|e| {
                        file_ops::show_error_dialog("Error", &format!("Failed to open directory: {}", e));
                    });
                }
            }
        }
    })?;
    
    // Save
    let save = nwg::MenuItem::new(file_menu, "Save", true, None)?;
    let window_clone = window.clone();
    nwg::bind_event_handler(&save.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnMenuItemSelected {
            let mut dialog = nwg::FileDialog::default();
            dialog.title("Save File");
            dialog.options(nwg::FileDialogOptions::OVERWRITE_PROMPT);
            
            if dialog.show() {
                if let Some(path) = dialog.selected_path() {
                    // TODO: Implement save functionality
                }
            }
        }
    })?;
    
    // Separator
    nwg::MenuItem::new(file_menu, "", false, None)?;
    
    // Exit
    let exit = nwg::MenuItem::new(file_menu, "Exit", true, None)?;
    nwg::bind_event_handler(&exit.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnMenuItemSelected {
            nwg::stop_thread_dispatch();
        }
    })?;
    
    Ok(())
}

/// Initialize the edit menu
pub fn init_edit_menu(window: &MainWindow) -> Result<()> {
    let edit_menu = &window.edit_menu;
    let window = Rc::new(RefCell::new(window.clone()));
    
    // Select All
    let select_all = nwg::MenuItem::new(edit_menu, "Select All\tCtrl+A", true, None)?;
    let window_clone = window.clone();
    nwg::bind_event_handler(&select_all.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnMenuItemSelected {
            let window = window_clone.borrow();
            window.select_all().unwrap_or_else(|e| {
                file_ops::show_error_dialog("Error", &format!("Failed to select all: {}", e));
            });
        }
    })?;
    
    // Invert Selection
    let invert_selection = nwg::MenuItem::new(edit_menu, "Invert Selection\tCtrl+I", true, None)?;
    let window_clone = window.clone();
    nwg::bind_event_handler(&invert_selection.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnMenuItemSelected {
            let window = window_clone.borrow();
            window.invert_selection().unwrap_or_else(|e| {
                file_ops::show_error_dialog("Error", &format!("Failed to invert selection: {}", e));
            });
        }
    })?;
    
    // Separator
    nwg::MenuItem::new(edit_menu, "", false, None)?;
    
    // Copy
    let copy = nwg::MenuItem::new(edit_menu, "Copy\tCtrl+C", true, None)?;
    let window_clone = window.clone();
    nwg::bind_event_handler(&copy.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnMenuItemSelected {
            let window = window_clone.borrow();
            let selected = window.get_selected_items();
            if !selected.is_empty() {
                let mut dialog = nwg::FileDialog::default();
                dialog.title("Copy To");
                dialog.options(nwg::FileDialogOptions::PICK_FOLDERS);
                
                if dialog.show() {
                    if let Some(dest) = dialog.selected_path() {
                        for source in selected {
                            match file_ops::copy_file(&source, &dest) {
                                Ok(()) => continue,
                                Err(e) => {
                                    file_ops::show_error_dialog("Error", &format!("Failed to copy {}: {}", source.display(), e));
                                    break;
                                }
                            }
                        }
                        window.update_list_view(&dest).unwrap_or_else(|e| {
                            file_ops::show_error_dialog("Error", &format!("Failed to update view: {}", e));
                        });
                    }
                }
            }
        }
    })?;
    
    // Cut
    let cut = nwg::MenuItem::new(edit_menu, "Cut\tCtrl+X", true, None)?;
    let window_clone = window.clone();
    nwg::bind_event_handler(&cut.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnMenuItemSelected {
            let window = window_clone.borrow();
            let selected = window.get_selected_items();
            if !selected.is_empty() {
                let mut dialog = nwg::FileDialog::default();
                dialog.title("Move To");
                dialog.options(nwg::FileDialogOptions::PICK_FOLDERS);
                
                if dialog.show() {
                    if let Some(dest) = dialog.selected_path() {
                        for source in selected {
                            match file_ops::move_file(&source, &dest) {
                                Ok(()) => continue,
                                Err(e) => {
                                    file_ops::show_error_dialog("Error", &format!("Failed to move {}: {}", source.display(), e));
                                    break;
                                }
                            }
                        }
                        window.update_list_view(&dest).unwrap_or_else(|e| {
                            file_ops::show_error_dialog("Error", &format!("Failed to update view: {}", e));
                        });
                    }
                }
            }
        }
    })?;
    
    // Delete
    let delete = nwg::MenuItem::new(edit_menu, "Delete\tDel", true, None)?;
    let window_clone = window.clone();
    nwg::bind_event_handler(&delete.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnMenuItemSelected {
            let window = window_clone.borrow();
            let selected = window.get_selected_items();
            if !selected.is_empty() {
                let count = selected.len();
                if file_ops::show_confirm_dialog(
                    "Confirm Delete",
                    &format!("Are you sure you want to delete {} item{}?", count, if count == 1 { "" } else { "s" })
                ) {
                    for path in selected {
                        if let Err(e) = file_ops::delete_file(&path) {
                            file_ops::show_error_dialog("Error", &format!("Failed to delete {}: {}", path.display(), e));
                            break;
                        }
                    }
                    window.update_list_view(&window.current_path.borrow()).unwrap_or_else(|e| {
                        file_ops::show_error_dialog("Error", &format!("Failed to update view: {}", e));
                    });
                }
            }
        }
    })?;
    
    Ok(())
}

/// Initialize the view menu
pub fn init_view_menu(window: &MainWindow) -> Result<()> {
    let view_menu = &window.view_menu;
    let window = Rc::new(RefCell::new(window.clone()));
    
    // Sort by name
    let sort_name = nwg::MenuItem::new(view_menu, "Sort by Name", true, None)?;
    let window_clone = window.clone();
    nwg::bind_event_handler(&sort_name.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnMenuItemSelected {
            let window = window_clone.borrow();
            window.set_sort_order(SortBy::Name).unwrap_or_else(|e| {
                file_ops::show_error_dialog("Error", &format!("Failed to sort: {}", e));
            });
        }
    })?;
    
    // Sort by size
    let sort_size = nwg::MenuItem::new(view_menu, "Sort by Size", true, None)?;
    let window_clone = window.clone();
    nwg::bind_event_handler(&sort_size.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnMenuItemSelected {
            let window = window_clone.borrow();
            window.set_sort_order(SortBy::Size).unwrap_or_else(|e| {
                file_ops::show_error_dialog("Error", &format!("Failed to sort: {}", e));
            });
        }
    })?;
    
    // Sort by date
    let sort_date = nwg::MenuItem::new(view_menu, "Sort by Date", true, None)?;
    let window_clone = window.clone();
    nwg::bind_event_handler(&sort_date.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnMenuItemSelected {
            let window = window_clone.borrow();
            window.set_sort_order(SortBy::Modified).unwrap_or_else(|e| {
                file_ops::show_error_dialog("Error", &format!("Failed to sort: {}", e));
            });
        }
    })?;
    
    Ok(())
} 