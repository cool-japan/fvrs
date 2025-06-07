use std::path::Path;
use chrono::{DateTime, Local};

/// ファイル名取得でエラーハンドリング
pub fn get_display_name(path: &Path) -> String {
    path.file_name()
        .and_then(|os_str| os_str.to_str())
        .unwrap_or("?")
        .to_string()
}

/// ファイルサイズのフォーマット
pub fn format_file_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = size as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", size as u64, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}

/// 時刻フォーマット
pub fn format_time(time: DateTime<Local>) -> String {
    time.format("%Y/%m/%d %H:%M").to_string()
}

/// 日本語フォントの設定
pub fn setup_japanese_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    
    // より多くのWindowsフォントパスを試行
    let font_paths = [
        "C:/Windows/Fonts/meiryo.ttc",       // メイリオ
        "C:/Windows/Fonts/msgothic.ttc",     // MSゴシック
        "C:/Windows/Fonts/YuGothM.ttc",      // 游ゴシック Medium
        "C:/Windows/Fonts/YuGothR.ttc",      // 游ゴシック Regular
        "C:/Windows/Fonts/NotoSansCJK-Regular.ttc", // Noto Sans CJK
        "C:/Windows/Fonts/calibri.ttf",      // Calibri (フォールバック)
    ];

    for font_path in &font_paths {
        if let Ok(font_data) = std::fs::read(font_path) {
            fonts.font_data.insert(
                "japanese_font".to_owned(),
                egui::FontData::from_owned(font_data).into(),
            );

            // プロポーショナルフォントファミリーに日本語フォントを最優先で追加
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "japanese_font".to_owned());

            // モノスペースフォントファミリーにも追加
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .insert(0, "japanese_font".to_owned());

            tracing::info!("日本語フォントを読み込みました: {}", font_path);
            break;
        }
    }

    ctx.set_fonts(fonts);
} 