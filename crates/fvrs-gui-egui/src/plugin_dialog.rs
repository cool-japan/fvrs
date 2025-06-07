use native_windows_gui as nwg;
use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;
use crate::plugin_manager::{PluginManager, PluginConfig};
use fvrs_plugin_api::PluginInfo;

#[derive(Default, NwgUi)]
pub struct PluginDialog {
    #[nwg_control(title: "Plugin Settings", size: (400, 300), center: true, visible: false)]
    window: nwg::Window,
    
    #[nwg_control(parent: window, text: "Plugins", size: (380, 200), position: (10, 10))]
    plugin_list: nwg::ListView,
    
    #[nwg_control(parent: window, text: "Enabled", size: (100, 25), position: (10, 220))]
    enabled_checkbox: nwg::Checkbox,
    
    #[nwg_control(parent: window, text: "Settings", size: (100, 25), position: (120, 220))]
    settings_button: nwg::Button,
    
    #[nwg_control(parent: window, text: "Close", size: (100, 25), position: (290, 220))]
    close_button: nwg::Button,
}

impl PluginDialog {
    pub fn new() -> Result<Self> {
        let mut dialog = Self::default();
        dialog.build()?;
        
        // Set columns for plugin list
        dialog.plugin_list.insert_column(nwg::InsertListViewColumn {
            index: Some(0),
            column: nwg::ListViewColumn {
                text: "Name".into(),
                width: Some(150),
                ..Default::default()
            },
            ..Default::default()
        })?;
        
        dialog.plugin_list.insert_column(nwg::InsertListViewColumn {
            index: Some(1),
            column: nwg::ListViewColumn {
                text: "Type".into(),
                width: Some(100),
                ..Default::default()
            },
            ..Default::default()
        })?;
        
        dialog.plugin_list.insert_column(nwg::InsertListViewColumn {
            index: Some(2),
            column: nwg::ListViewColumn {
                text: "Version".into(),
                width: Some(100),
                ..Default::default()
            },
            ..Default::default()
        })?;
        
        Ok(dialog)
    }
    
    pub fn show(&self, plugin_manager: &PluginManager) -> Result<()> {
        self.update_plugin_list(plugin_manager)?;
        self.window.set_visible(true);
        Ok(())
    }
    
    fn update_plugin_list(&self, plugin_manager: &PluginManager) -> Result<()> {
        self.plugin_list.clear();
        
        for plugin in plugin_manager.list_plugins() {
            let config = plugin_manager.get_plugin_config(&plugin.name);
            let enabled = config.map_or(false, |c| c.enabled);
            
            self.plugin_list.insert_item(nwg::InsertListViewItem {
                index: Some(0),
                column_index: Some(0),
                text: Some(plugin.name.clone()),
                ..Default::default()
            })?;
            
            self.plugin_list.insert_item(nwg::InsertListViewItem {
                index: Some(0),
                column_index: Some(1),
                text: Some(plugin.plugin_type.to_string()),
                ..Default::default()
            })?;
            
            self.plugin_list.insert_item(nwg::InsertListViewItem {
                index: Some(0),
                column_index: Some(2),
                text: Some(plugin.version.clone()),
                ..Default::default()
            })?;
        }
        
        Ok(())
    }
    
    fn show_settings_dialog(&self, plugin_info: &PluginInfo, config: &PluginConfig) -> Result<()> {
        let mut dialog = nwg::ModalWindow::default();
        dialog.build(nwg::WindowBuilder::default()
            .title(format!("Settings - {}", plugin_info.name))
            .size((300, 200))
            .center())?;
        
        let mut settings_grid = nwg::GridLayout::default();
        settings_grid.build(nwg::GridLayoutBuilder::default()
            .parent(&dialog)
            .spacing(5)
            .child_margin(5))?;
        
        let mut row = 0;
        for (key, value) in &config.settings {
            let label = nwg::Label::default();
            label.build(nwg::LabelBuilder::default()
                .text(key)
                .parent(&dialog)
                .build())?;
            
            let input = nwg::TextInput::default();
            input.build(nwg::TextInputBuilder::default()
                .text(value)
                .parent(&dialog)
                .build())?;
            
            settings_grid.add_child(row, 0, &label);
            settings_grid.add_child(row, 1, &input);
            row += 1;
        }
        
        let mut ok_button = nwg::Button::default();
        ok_button.build(nwg::ButtonBuilder::default()
            .text("OK")
            .parent(&dialog)
            .build())?;
        
        let mut cancel_button = nwg::Button::default();
        cancel_button.build(nwg::ButtonBuilder::default()
            .text("Cancel")
            .parent(&dialog)
            .build())?;
        
        settings_grid.add_child(row, 0, &ok_button);
        settings_grid.add_child(row, 1, &cancel_button);
        
        dialog.set_visible(true);
        Ok(())
    }
}

/// Initialize plugin settings dialog
pub fn init_plugin_dialog(dialog: &PluginDialog, plugin_manager: Rc<RefCell<PluginManager>>) -> Result<()> {
    let dialog = Rc::new(RefCell::new(dialog.clone()));
    
    // Plugin list selection change
    let dialog_clone = dialog.clone();
    let plugin_manager_clone = plugin_manager.clone();
    nwg::bind_event_handler(&dialog.borrow().plugin_list.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnListViewSelect {
            let dialog = dialog_clone.borrow();
            let plugin_manager = plugin_manager_clone.borrow();
            
            if let Some(index) = dialog.plugin_list.get_selected_index() {
                if let Some(name) = dialog.plugin_list.get_item_text(index, 0) {
                    if let Some(config) = plugin_manager.get_plugin_config(&name) {
                        dialog.enabled_checkbox.set_checked(config.enabled);
                    }
                }
            }
        }
    })?;
    
    // Enable checkbox
    let dialog_clone = dialog.clone();
    let plugin_manager_clone = plugin_manager.clone();
    nwg::bind_event_handler(&dialog.borrow().enabled_checkbox.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnCheckboxClick {
            let dialog = dialog_clone.borrow();
            let mut plugin_manager = plugin_manager_clone.borrow_mut();
            
            if let Some(index) = dialog.plugin_list.get_selected_index() {
                if let Some(name) = dialog.plugin_list.get_item_text(index, 0) {
                    let enabled = dialog.enabled_checkbox.checked();
                    if let Err(e) = plugin_manager.set_plugin_enabled(&name, enabled) {
                        file_ops::show_error_dialog("Error", &format!("Failed to update plugin: {}", e));
                    }
                }
            }
        }
    })?;
    
    // Settings button
    let dialog_clone = dialog.clone();
    let plugin_manager_clone = plugin_manager.clone();
    nwg::bind_event_handler(&dialog.borrow().settings_button.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnButtonClick {
            let dialog = dialog_clone.borrow();
            let plugin_manager = plugin_manager_clone.borrow();
            
            if let Some(index) = dialog.plugin_list.get_selected_index() {
                if let Some(name) = dialog.plugin_list.get_item_text(index, 0) {
                    if let Some(plugin) = plugin_manager.plugins.get(&name) {
                        if let Some(config) = plugin_manager.get_plugin_config(&name) {
                            if let Err(e) = dialog.show_settings_dialog(&plugin.info(), config) {
                                file_ops::show_error_dialog("Error", &format!("Failed to show settings: {}", e));
                            }
                        }
                    }
                }
            }
        }
    })?;
    
    // Close button
    let dialog_clone = dialog.clone();
    nwg::bind_event_handler(&dialog.borrow().close_button.handle, move |evt, _evt_data, _handle| {
        if evt == nwg::Event::OnButtonClick {
            dialog_clone.borrow().window.set_visible(false);
        }
    })?;
    
    Ok(())
} 