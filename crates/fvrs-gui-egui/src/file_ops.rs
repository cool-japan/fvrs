use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// File operation error types
#[derive(Debug)]
pub enum FileOpError {
    Io(io::Error),
    SourceNotFound(PathBuf),
    DestinationExists(PathBuf),
    Cancelled,
}

impl From<io::Error> for FileOpError {
    fn from(err: io::Error) -> Self {
        FileOpError::Io(err)
    }
}

impl std::fmt::Display for FileOpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileOpError::Io(e) => write!(f, "IO error: {}", e),
            FileOpError::SourceNotFound(path) => write!(f, "Source file not found: {}", path.display()),
            FileOpError::DestinationExists(path) => write!(f, "Destination already exists: {}", path.display()),
            FileOpError::Cancelled => write!(f, "Operation cancelled by user"),
        }
    }
}

impl std::error::Error for FileOpError {}

type FileOpResult<T> = std::result::Result<T, FileOpError>;

/// File entry information
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: SystemTime,
}

impl FileEntry {
    pub fn new(path: PathBuf) -> io::Result<Self> {
        let metadata = fs::metadata(&path)?;
        let name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        
        Ok(Self {
            path,
            name,
            is_dir: metadata.is_dir(),
            size: metadata.len(),
            modified: metadata.modified()?,
        })
    }
}

/// 高速なファイル一覧取得（表示数制限付き）
pub fn get_entries_fast(path: &Path, show_hidden: bool) -> FileOpResult<Vec<FileEntry>> {
    const MAX_ENTRIES: usize = 1000; // 最大表示数を制限してパフォーマンス向上
    
    let mut entries = Vec::new();
    
    // 親ディレクトリエントリを追加
    if path.parent().is_some() {
        entries.push(FileEntry {
            path: path.to_path_buf(),
            name: "..".to_string(),
            is_dir: true,
            size: 0,
            modified: SystemTime::now(),
        });
    }
    
    // ディレクトリエントリを効率的に読み込み
    let dir_entries: Vec<_> = fs::read_dir(path)?
        .filter_map(|entry| entry.ok())
        .take(MAX_ENTRIES)
        .collect();
    
    // 隠しファイル/フォルダのフィルタリング
    let filtered_entries: Vec<_> = dir_entries
        .into_iter()
        .filter(|entry| {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();
            show_hidden || !name.starts_with('.')
        })
        .collect();
    
    // 並列処理的にメタデータ取得（実際の並列処理はせずに効率化）
    for entry in filtered_entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        
        // 軽量なメタデータ取得
        if let Ok(metadata) = entry.metadata() {
            let size = if metadata.is_file() { metadata.len() } else { 0 };
            let modified = metadata.modified().unwrap_or(SystemTime::now());
            
            entries.push(FileEntry {
                path,
                name,
                is_dir: metadata.is_dir(),
                size,
                modified,
            });
        }
    }
    
    // 効率的なソート：ディレクトリ優先、その後名前順
    entries.sort_unstable_by(|a, b| {
        // .. エントリは常に最初
        if a.name == ".." {
            return std::cmp::Ordering::Less;
        }
        if b.name == ".." {
            return std::cmp::Ordering::Greater;
        }
        
        // ディレクトリを先に
        match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });
    
    Ok(entries)
}

/// Format a file size for display
pub fn format_size(size: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut size = size as f64;
    let mut unit_index = 0;
    
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    
    format!("{:.1} {}", size, UNITS[unit_index])
}

/// Format a timestamp for display
pub fn format_time(time: SystemTime) -> String {
    if let Ok(duration) = time.duration_since(SystemTime::UNIX_EPOCH) {
        let secs = duration.as_secs();
        let datetime = chrono::DateTime::from_timestamp(secs as i64, 0)
            .unwrap_or_default();
        datetime.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        "Unknown".to_string()
    }
} 