//! Native Windows GUI implementation for FVRS
//!
//! This crate provides a native Windows GUI using the native-windows-gui library.

mod menu;
mod file_ops;
mod preview;
mod filter;
mod clipboard;
mod drag_drop;
mod plugin_manager;
mod plugin_dialog;

use native_windows_gui as nwg;
use native_windows_derive as nwd;
use std::path::PathBuf;
use thiserror::Error;
use std::sync::mpsc;
use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashSet;
use std::fs;
use std::thread;
use std::time::Duration;
use std::default::Default;

use fvrs_core::{Config, Result as CoreResult, SortBy};
use fvrs_plugin_api::Plugin;
use file_ops::{FileEntry, get_sorted_entries, DragOp, format_size, format_time, format_date};

use menu::{init_file_menu, init_edit_menu, init_view_menu, init_context_menu, ContextMenu};
use preview::PreviewPanel;
use filter::FilterPanel;
use clipboard::{Clipboard, ClipboardOp};
use drag_drop::{DragDrop, init_drag_drop};
use plugin_manager::PluginManager;
use plugin_dialog::{PluginDialog, init_plugin_dialog};

/// Error type for GUI operations
#[derive(Error, Debug)]
pub enum GuiError {
    #[error("NWG error: {0}")]
    Nwg(#[from] nwg::NwgError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
}

/// Result type for GUI operations
pub type GuiResult<T> = Result<T, GuiError>;

/// File system event
#[derive(Debug, Clone)]
pub enum FsEvent {
    DirectoryChanged(PathBuf),
    FileSelected(PathBuf),
    SortChanged(SortBy),
}

/// Main window structure
pub struct MainWindow {
    /// Main window handle
    window: nwg::Window,
    /// File list view
    file_list: nwg::ListView,
    /// Status bar
    status_bar: nwg::StatusBar,
    /// Current directory
    current_dir: PathBuf,
}

impl MainWindow {
    /// Create a new main window
    pub fn new() -> GuiResult<Self> {
        let mut window = Default::default();
        let mut file_list = Default::default();
        let mut status_bar = Default::default();

        nwg::Window::builder()
            .title("FVRS")
            .size((800, 600))
            .build(&mut window)?;

        nwg::ListView::builder()
            .parent(&window)
            .build(&mut file_list)?;

        nwg::StatusBar::builder()
            .parent(&window)
            .build(&mut status_bar)?;

        Ok(Self {
            window,
            file_list,
            status_bar,
            current_dir: PathBuf::from("."),
        })
    }

    /// Initialize the window
    pub fn init(&self) -> GuiResult<()> {
        // Set up event handlers
        let file_list = self.file_list.clone();
        self.window.on_window_close(move |_| {
            nwg::stop_thread_dispatch();
        });

        // Set up file list columns
        self.file_list.insert_column(nwg::InsertListViewColumn {
            index: Some(0),
            text: "Name",
            width: Some(200),
            ..Default::default()
        })?;

        self.file_list.insert_column(nwg::InsertListViewColumn {
            index: Some(1),
            text: "Size",
            width: Some(100),
            ..Default::default()
        })?;

        self.file_list.insert_column(nwg::InsertListViewColumn {
            index: Some(2),
            text: "Type",
            width: Some(100),
            ..Default::default()
        })?;

        self.file_list.insert_column(nwg::InsertListViewColumn {
            index: Some(3),
            text: "Modified",
            width: Some(150),
            ..Default::default()
        })?;

        Ok(())
    }

    /// Update the file list
    pub fn update_file_list(&self) -> GuiResult<()> {
        // Clear current items
        self.file_list.clear()?;

        // TODO: Implement file listing logic
        // This will be implemented when we integrate with fvrs-core

        Ok(())
    }

    /// Show the window
    pub fn show(&self) {
        self.window.set_visible(true);
    }
}

/// Application structure
pub struct Application {
    /// Main window
    main_window: MainWindow,
}

impl Application {
    /// Create a new application instance
    pub fn new() -> GuiResult<Self> {
        Ok(Self {
            main_window: MainWindow::new()?,
        })
    }

    /// Initialize the application
    pub fn init(&self) -> GuiResult<()> {
        self.main_window.init()?;
        Ok(())
    }

    /// Run the application
    pub fn run(&self) {
        self.main_window.show();
        nwg::dispatch_thread_events();
    }
}

/// Initialize the GUI
pub fn init() -> Result<()> {
    nwg::init().map_err(GuiError::Nwg)?;
    Ok(())
}

/// Run the GUI main loop
pub fn run() -> Result<()> {
    let mut window = MainWindow::new()?;
    
    // メニュー初期化
    init_file_menu(&window)?;
    init_edit_menu(&window)?;
    init_view_menu(&window)?;
    let context_menu = init_context_menu(&window)?;
    *window.context_menu.borrow_mut() = Some(context_menu);
    
    // 初期ディレクトリで初期化
    let current_dir = std::env::current_dir()?;
    window.init_tree_view(&current_dir)?;
    window.update_list_view(&current_dir)?;
    
    nwg::dispatch_thread_events();
    Ok(())
}

/// Initialize the edit menu
pub fn init_edit_menu(window: &MainWindow) -> Result<()> {
    let edit_menu = &window.edit_menu;
    let window = Rc::new(RefCell::new(window.clone()));
    
    // Copy
    let copy = nwg::MenuItem::new(edit_menu, "Copy\tCtrl+C", true, None)?;
    let window_clone = window.clone();
    nwg::bind_event_handler(&copy.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnMenuItemSelected {
            let window = window_clone.borrow();
            if let Err(e) = window.copy_selected() {
                file_ops::show_error_dialog("Error", &format!("Failed to copy: {}", e));
            }
        }
    })?;
    
    // Cut
    let cut = nwg::MenuItem::new(edit_menu, "Cut\tCtrl+X", true, None)?;
    let window_clone = window.clone();
    nwg::bind_event_handler(&cut.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnMenuItemSelected {
            let window = window_clone.borrow();
            if let Err(e) = window.cut_selected() {
                file_ops::show_error_dialog("Error", &format!("Failed to cut: {}", e));
            }
        }
    })?;
    
    // Paste
    let paste = nwg::MenuItem::new(edit_menu, "Paste\tCtrl+V", true, None)?;
    let window_clone = window.clone();
    nwg::bind_event_handler(&paste.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnMenuItemSelected {
            let window = window_clone.borrow();
            if let Err(e) = window.paste_from_clipboard() {
                file_ops::show_error_dialog("Error", &format!("Failed to paste: {}", e));
            }
        }
    })?;
    
    Ok(())
}

/// Initialize the search panel
pub fn init_search_panel(window: &MainWindow) -> Result<()> {
    let search_panel = &window.search_panel;
    let window = Rc::new(RefCell::new(window.clone()));
    
    // Start button
    let window_clone = window.clone();
    nwg::bind_event_handler(&search_panel.start_button.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnButtonClick {
            let window = window_clone.borrow();
            if let Err(e) = search_panel.start_search() {
                file_ops::show_error_dialog("Error", &format!("Failed to start search: {}", e));
            }
        }
    })?;
    
    // Stop button
    let window_clone = window.clone();
    nwg::bind_event_handler(&search_panel.stop_button.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnButtonClick {
            let window = window_clone.borrow();
            if let Err(e) = search_panel.stop_search() {
                file_ops::show_error_dialog("Error", &format!("Failed to stop search: {}", e));
            }
        }
    })?;
    
    // Search input
    let window_clone = window.clone();
    nwg::bind_event_handler(&search_panel.search_input.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnTextInput {
            let window = window_clone.borrow();
            if !search_panel.is_running() {
                if let Err(e) = search_panel.start_search() {
                    file_ops::show_error_dialog("Error", &format!("Failed to start search: {}", e));
                }
            }
        }
    })?;
    
    Ok(())
}

pub fn init_menu(window: &MainWindow) -> Result<()> {
    // ... existing code ...
    
    let tools_menu = nwg::Menu::new(&window.menu)?;
    nwg::MenuItem::new(&window.menu, "Tools", true, Some(&tools_menu))?;
    
    let plugin_settings = nwg::MenuItem::new(&tools_menu, "Plugin Settings", true, None)?;
    let window = Rc::new(RefCell::new(window.clone()));
    nwg::bind_event_handler(&plugin_settings.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnMenuItemSelected {
            let window = window.borrow();
            if let Err(e) = window.show_plugin_dialog() {
                file_ops::show_error_dialog("Error", &format!("Failed to show plugin dialog: {}", e));
            }
        }
    })?;
    
    Ok(())
} 