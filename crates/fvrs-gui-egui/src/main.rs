//! FVRS GUI 起動バイナリ
//!
//! モジュール本体はライブラリクレート `fvrs_gui_egui` 側に一元化されている
//! （eframe フレーム実装は `fvrs_gui_egui::ui::app_shell`）。

use fvrs_gui_egui::app::FileVisorApp;

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
