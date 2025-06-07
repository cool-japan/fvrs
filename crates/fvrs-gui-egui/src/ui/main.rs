impl MainUI {
    pub fn show(ctx: &egui::Context, app: &mut crate::app::FileVisorApp) {
        // ... existing UI code ...

        // ダイアログ表示
        crate::ui::dialogs::DialogsUI::show_delete_dialog(ctx, app);
        crate::ui::dialogs::DialogsUI::show_shortcuts_dialog(ctx, app);
        crate::ui::dialogs::DialogsUI::show_create_file_dialog(ctx, app);
        crate::ui::dialogs::DialogsUI::show_create_folder_dialog(ctx, app);
        crate::ui::dialogs::DialogsUI::show_unsaved_dialog(ctx, app);
        
        // 圧縮ファイル関連ダイアログ
        crate::ui::dialogs::DialogsUI::show_unpack_dialog(ctx, app);
        crate::ui::dialogs::DialogsUI::show_pack_dialog(ctx, app);
        crate::ui::dialogs::DialogsUI::show_archive_viewer(ctx, app);

        // ... rest of existing code ...
    }
} 