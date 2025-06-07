use std::path::{Path, PathBuf};
use std::fs::{self, Metadata};
use std::time::SystemTime;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

/// ファイル情報構造体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedFileInfo {
    // 基本情報
    pub name: String,
    pub full_path: PathBuf,
    pub size: u64,
    pub size_on_disk: u64,
    pub is_directory: bool,
    pub is_hidden: bool,
    pub is_readonly: bool,
    pub is_system: bool,
    
    // 時刻情報
    pub created: Option<DateTime<Local>>,
    pub modified: Option<DateTime<Local>>,
    pub accessed: Option<DateTime<Local>>,
    
    // ファイル種別情報
    pub file_type: String,
    pub file_extension: String,
    pub mime_type: String,
    
    // 権限・セキュリティ情報
    pub permissions: String,
    pub owner: String,
    
    // メタデータ情報
    pub version: Option<String>,
    pub company: Option<String>,
    pub description: Option<String>,
    pub copyright: Option<String>,
    
    // 関連付け情報
    pub associated_program: Option<String>,
    pub open_with_command: Option<String>,
    
    // システム情報
    pub disk_space: Option<DiskSpaceInfo>,
    pub computer_name: String,
}

/// ディスク容量情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskSpaceInfo {
    pub total_space: u64,
    pub free_space: u64,
    pub used_space: u64,
}

/// ファイル情報取得ユーティリティ
pub struct FileInfoCollector;

impl FileInfoCollector {
    /// 詳細なファイル情報を取得
    pub fn collect_detailed_info(path: &Path) -> Result<DetailedFileInfo, String> {
        let metadata = fs::metadata(path)
            .map_err(|e| format!("メタデータ取得エラー: {}", e))?;
        
        let name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("不明")
            .to_string();
        
        let full_path = path.to_path_buf();
        
        // 基本情報
        let size = metadata.len();
        let size_on_disk = Self::calculate_size_on_disk(&metadata);
        let is_directory = metadata.is_dir();
        let (is_hidden, is_readonly, is_system) = Self::get_file_attributes(&metadata);
        
        // 時刻情報
        let created = Self::system_time_to_local(metadata.created().ok());
        let modified = Self::system_time_to_local(metadata.modified().ok());
        let accessed = Self::system_time_to_local(metadata.accessed().ok());
        
        // ファイル種別情報
        let file_extension = path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_string();
        let file_type = Self::determine_file_type(path);
        let mime_type = Self::get_mime_type(path);
        
        // 権限・セキュリティ情報
        let permissions = Self::get_permissions_string(&metadata);
        let owner = Self::get_file_owner(path);
        
        // メタデータ情報（Windows PE ファイルの場合）
        let (version, company, description, copyright) = Self::get_pe_metadata(path);
        
        // 関連付け情報
        let (associated_program, open_with_command) = Self::get_file_associations(&file_extension);
        
        // システム情報
        let disk_space = Self::get_disk_space_info(path);
        let computer_name = Self::get_computer_name();
        
        Ok(DetailedFileInfo {
            name,
            full_path,
            size,
            size_on_disk,
            is_directory,
            is_hidden,
            is_readonly,
            is_system,
            created,
            modified,
            accessed,
            file_type,
            file_extension,
            mime_type,
            permissions,
            owner,
            version,
            company,
            description,
            copyright,
            associated_program,
            open_with_command,
            disk_space,
            computer_name,
        })
    }
    
    /// ディスク上のサイズを計算（クラスターサイズを考慮）
    fn calculate_size_on_disk(metadata: &Metadata) -> u64 {
        let size = metadata.len();
        if size == 0 {
            return 0;
        }
        
        // 簡略化：4KB クラスターサイズと仮定
        let cluster_size = 4096u64;
        ((size + cluster_size - 1) / cluster_size) * cluster_size
    }
    
    /// ファイル属性を取得
    fn get_file_attributes(metadata: &Metadata) -> (bool, bool, bool) {
        let is_readonly = metadata.permissions().readonly();
        
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            let attrs = metadata.file_attributes();
            let is_hidden = (attrs & 0x2) != 0; // FILE_ATTRIBUTE_HIDDEN
            let is_system = (attrs & 0x4) != 0; // FILE_ATTRIBUTE_SYSTEM
            (is_hidden, is_readonly, is_system)
        }
        
        #[cfg(not(windows))]
        {
            // Unix系では隠しファイルは名前が '.' で始まる
            (false, is_readonly, false)
        }
    }
    
    /// SystemTimeをLocal DateTimeに変換
    fn system_time_to_local(time: Option<SystemTime>) -> Option<DateTime<Local>> {
        time.and_then(|t| {
            let datetime: DateTime<chrono::Utc> = t.into();
            Some(datetime.with_timezone(&Local))
        })
    }
    
    /// ファイル種別を判定
    fn determine_file_type(path: &Path) -> String {
        if path.is_dir() {
            return "フォルダー".to_string();
        }
        
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        
        match ext.as_str() {
            "txt" => "テキストファイル".to_string(),
            "doc" | "docx" => "Microsoft Word 文書".to_string(),
            "xls" | "xlsx" => "Microsoft Excel ワークシート".to_string(),
            "ppt" | "pptx" => "Microsoft PowerPoint プレゼンテーション".to_string(),
            "pdf" => "Adobe Acrobat Document".to_string(),
            "jpg" | "jpeg" => "JPEG画像".to_string(),
            "png" => "PNG画像".to_string(),
            "gif" => "GIF画像".to_string(),
            "mp4" => "MP4動画ファイル".to_string(),
            "mp3" => "MP3音声ファイル".to_string(),
            "zip" => "ZIP圧縮ファイル".to_string(),
            "rar" => "RAR圧縮ファイル".to_string(),
            "exe" => "アプリケーション".to_string(),
            "dll" => "アプリケーション拡張".to_string(),
            "rs" => "Rustソースファイル".to_string(),
            "py" => "Pythonスクリプト".to_string(),
            "js" => "JavaScriptファイル".to_string(),
            "html" => "HTMLファイル".to_string(),
            "css" => "CSSスタイルシート".to_string(),
            "json" => "JSONファイル".to_string(),
            "xml" => "XMLファイル".to_string(),
            "yaml" | "yml" => "YAMLファイル".to_string(),
            "toml" => "TOMLファイル".to_string(),
            _ => {
                if ext.is_empty() {
                    "ファイル".to_string()
                } else {
                    format!("{}ファイル", ext.to_uppercase())
                }
            }
        }
    }
    
    /// MIMEタイプを取得
    fn get_mime_type(path: &Path) -> String {
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        
        match ext.as_str() {
            "txt" => "text/plain",
            "html" => "text/html",
            "css" => "text/css",
            "js" => "application/javascript",
            "json" => "application/json",
            "xml" => "application/xml",
            "pdf" => "application/pdf",
            "jpg" | "jpeg" => "image/jpeg",
            "png" => "image/png",
            "gif" => "image/gif",
            "mp4" => "video/mp4",
            "mp3" => "audio/mpeg",
            "zip" => "application/zip",
            _ => "application/octet-stream",
        }.to_string()
    }
    
    /// 権限情報を文字列として取得
    fn get_permissions_string(metadata: &Metadata) -> String {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = metadata.permissions().mode();
            format!("{:o}", mode & 0o777)
        }
        
        #[cfg(windows)]
        {
            if metadata.permissions().readonly() {
                "読み取り専用".to_string()
            } else {
                "読み取り/書き込み".to_string()
            }
        }
        
        #[cfg(not(any(unix, windows)))]
        {
            "不明".to_string()
        }
    }
    
    /// ファイル所有者を取得
    fn get_file_owner(_path: &Path) -> String {
        #[cfg(windows)]
        {
            // Windows実装は複雑なので簡略化
            "管理者".to_string()
        }
        
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if let Ok(metadata) = std::fs::metadata(_path) {
                let uid = metadata.uid();
                format!("UID: {}", uid)
            } else {
                "不明".to_string()
            }
        }
        
        #[cfg(not(any(unix, windows)))]
        {
            "不明".to_string()
        }
    }
    
    /// PE（実行可能）ファイルのメタデータを取得
    fn get_pe_metadata(path: &Path) -> (Option<String>, Option<String>, Option<String>, Option<String>) {
        // 実装は複雑なので、拡張子による簡易判定
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        
        match ext.as_str() {
            "exe" | "dll" => {
                // 実際の実装では PE ヘッダーを解析
                (
                    Some("1.0.0".to_string()),
                    Some("Unknown Company".to_string()),
                    Some("Windows Application".to_string()),
                    Some("Copyright © 2024".to_string())
                )
            }
            _ => (None, None, None, None)
        }
    }
    
    /// ファイル関連付け情報を取得
    fn get_file_associations(extension: &str) -> (Option<String>, Option<String>) {
        if extension.is_empty() {
            return (None, None);
        }
        
        // 簡略化：一般的な関連付けのハードコード
        let (program, command) = match extension.to_lowercase().as_str() {
            "txt" => ("メモ帳", "notepad.exe"),
            "rs" => ("Visual Studio Code", "code.exe"),
            "py" => ("Python", "python.exe"),
            "html" => ("既定のブラウザー", "browser.exe"),
            "jpg" | "png" | "gif" => ("フォト", "photos.exe"),
            "mp4" => ("映画&テレビ", "movies.exe"),
            "mp3" => ("Groove ミュージック", "music.exe"),
            "pdf" => ("Adobe Acrobat Reader", "acrobat.exe"),
            "zip" => ("エクスプローラー", "explorer.exe"),
            _ => return (None, None),
        };
        
        (Some(program.to_string()), Some(command.to_string()))
    }
    
    /// ディスク容量情報を取得
    fn get_disk_space_info(_path: &Path) -> Option<DiskSpaceInfo> {
        // 実装は複雑なのでプレースホルダー
        Some(DiskSpaceInfo {
            total_space: 1_000_000_000_000, // 1TB
            free_space: 500_000_000_000,    // 500GB
            used_space: 500_000_000_000,    // 500GB
        })
    }
    
    /// コンピューター名を取得
    fn get_computer_name() -> String {
        std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| "Unknown".to_string())
    }
}

/// ファイルサイズフォーマット
pub fn format_size(size: u64) -> String {
    const UNITS: &[&str] = &["バイト", "KB", "MB", "GB", "TB"];
    let mut size = size as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", size as u64, UNITS[unit_index])
    } else {
        format!("{:.1} {} ({} バイト)", size, UNITS[unit_index], size as u64 * (1024_u64.pow(unit_index as u32)))
    }
} 