use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use chrono::{DateTime, Local};

/// 利用可能なマウントポイント（ドライブ・ボリューム）を実行時に列挙する
///
/// - Windows: `A:\` 〜 `Z:\` の存在確認による列挙（外部クレート不要）
/// - macOS: `/` と `/Volumes` 直下のディレクトリ
/// - その他 Unix (Linux 等): `/` と `/mnt`・`/media` 直下、
///   および `/run/media/<user>/` 配下のディレクトリ
pub fn available_mounts() -> Vec<PathBuf> {
    collect_mounts()
}

/// マウント一覧の時間ベースキャッシュ
///
/// マウント列挙はボリュームごとに stat を伴い、応答しないネットワーク
/// ボリュームがあると UI スレッドが固まるため、毎フレームの列挙を避けて
/// TTL 経過時のみ再列挙する。
pub struct MountCache {
    mounts: Vec<PathBuf>,
    last_refresh: Option<Instant>,
}

impl MountCache {
    /// キャッシュの有効期間
    const TTL: Duration = Duration::from_secs(3);

    /// 空のキャッシュを作成する（初回アクセス時に列挙される）
    pub fn new() -> Self {
        Self {
            mounts: Vec::new(),
            last_refresh: None,
        }
    }

    /// キャッシュ済みのマウント一覧を返す（TTL 経過時のみ再列挙）
    pub fn mounts(&mut self) -> &[PathBuf] {
        let stale = self
            .last_refresh
            .map(|refreshed| refreshed.elapsed() >= Self::TTL)
            .unwrap_or(true);
        if stale {
            self.mounts = collect_mounts();
            self.last_refresh = Some(Instant::now());
        }
        &self.mounts
    }
}

impl Default for MountCache {
    fn default() -> Self {
        Self::new()
    }
}

/// マウントポイントの表示名（ドライブ文字・ボリューム名）を返す
pub fn mount_label(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(windows)]
fn collect_mounts() -> Vec<PathBuf> {
    (b'A'..=b'Z')
        .map(|letter| PathBuf::from(format!("{}:\\", letter as char)))
        .filter(|path| path.exists())
        .collect()
}

#[cfg(target_os = "macos")]
fn collect_mounts() -> Vec<PathBuf> {
    let mut mounts = vec![PathBuf::from("/")];
    append_subdirectories(Path::new("/Volumes"), &mut mounts);
    mounts
}

#[cfg(all(unix, not(target_os = "macos")))]
fn collect_mounts() -> Vec<PathBuf> {
    let mut mounts = vec![PathBuf::from("/")];
    append_subdirectories(Path::new("/mnt"), &mut mounts);
    append_subdirectories(Path::new("/media"), &mut mounts);

    // udisks2 系のマウント規約: /run/media/<user>/<volume>
    let mut media_user_dirs = Vec::new();
    append_subdirectories(Path::new("/run/media"), &mut media_user_dirs);
    for user_dir in &media_user_dirs {
        append_subdirectories(user_dir, &mut mounts);
    }

    mounts
}

#[cfg(not(any(windows, unix)))]
fn collect_mounts() -> Vec<PathBuf> {
    vec![PathBuf::from(std::path::MAIN_SEPARATOR_STR)]
}

/// `base` 直下のディレクトリをソートして `mounts` に追加する
#[cfg(unix)]
fn append_subdirectories(base: &Path, mounts: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(base) {
        let mut dirs: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        dirs.sort();
        mounts.extend(dirs);
    }
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

/// OSごとの日本語フォント候補パス一覧
fn japanese_font_candidates() -> &'static [&'static str] {
    #[cfg(target_os = "windows")]
    {
        &[
            "C:/Windows/Fonts/meiryo.ttc",       // メイリオ
            "C:/Windows/Fonts/msgothic.ttc",     // MSゴシック
            "C:/Windows/Fonts/YuGothM.ttc",      // 游ゴシック Medium
            "C:/Windows/Fonts/YuGothR.ttc",      // 游ゴシック Regular
            "C:/Windows/Fonts/NotoSansCJK-Regular.ttc", // Noto Sans CJK
            "C:/Windows/Fonts/calibri.ttf",      // Calibri (フォールバック)
        ]
    }
    #[cfg(target_os = "macos")]
    {
        &[
            "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc", // ヒラギノ角ゴシック W3
            "/System/Library/Fonts/ヒラギノ角ゴシック W4.ttc", // ヒラギノ角ゴシック W4
            "/System/Library/Fonts/ヒラギノ丸ゴ ProN W4.ttc",  // ヒラギノ丸ゴ ProN
            "/System/Library/Fonts/ヒラギノ明朝 ProN.ttc",     // ヒラギノ明朝 ProN
            "/System/Library/Fonts/Hiragino Sans GB.ttc",       // Hiragino Sans GB
            "/Library/Fonts/NotoSansCJK-Regular.ttc",           // Noto Sans CJK (手動導入)
            "/System/Library/Fonts/AquaKana.ttc",               // アクアかな (フォールバック)
        ]
    }
    #[cfg(target_os = "linux")]
    {
        &[
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", // Noto Sans CJK (Debian/Ubuntu)
            "/usr/share/fonts/opentype/noto/NotoSansCJKjp-Regular.otf", // Noto Sans CJK JP
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",      // Noto CJK (Arch)
            "/usr/share/fonts/TTF/NotoSansCJK-Regular.ttc",           // Noto CJK (Arch 旧配置)
            "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc", // Noto CJK (Fedora)
            "/usr/share/fonts/opentype/ipafont-gothic/ipag.ttf",      // IPAゴシック
            "/usr/share/fonts/truetype/fonts-japanese-gothic.ttf",    // 日本語ゴシック (alternatives)
            "/usr/share/fonts/truetype/takao-gothic/TakaoGothic.ttf", // Takaoゴシック
        ]
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        &[]
    }
}

/// 日本語フォントの設定
pub fn setup_japanese_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // OSごとの候補パスを順に試行
    let font_paths = japanese_font_candidates();

    for font_path in font_paths {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// マウント列挙は空にならず、返るパスはすべて実在すること
    /// （Windows は存在確認済みドライブ、Unix はルート `/` が必ず含まれる）
    #[test]
    fn available_mounts_returns_existing_directories() {
        let mounts = available_mounts();
        assert!(!mounts.is_empty(), "マウント一覧が空です");
        for mount in &mounts {
            assert!(mount.exists(), "存在しないマウント: {}", mount.display());
        }
    }

    /// mount_label はボリューム名またはルート表記を返すこと
    #[test]
    fn mount_label_uses_last_component_or_full_path() {
        assert_eq!(mount_label(Path::new("/Volumes/Data")), "Data");
        #[cfg(unix)]
        assert_eq!(mount_label(Path::new("/")), "/");
    }

    /// macOS ではシステム標準の日本語フォントが必ず存在するはずなので、
    /// 候補リストから実在するフォントが見つかり、非空バイト列として
    /// 読み込めることを検証する（ウィンドウ・ディスプレイ不要）。
    #[test]
    #[cfg(target_os = "macos")]
    fn japanese_font_candidates_find_loadable_font_on_macos() {
        let candidates = japanese_font_candidates();
        assert!(
            !candidates.is_empty(),
            "macOS の日本語フォント候補リストが空です"
        );

        let loaded = candidates.iter().find_map(|path| {
            std::fs::read(path).ok().map(|bytes| (*path, bytes))
        });

        match loaded {
            Some((path, bytes)) => {
                assert!(
                    !bytes.is_empty(),
                    "フォントファイルが空です: {}",
                    path
                );
            }
            None => panic!(
                "候補パスのいずれからも日本語フォントを読み込めませんでした: {:?}",
                candidates
            ),
        }
    }
}