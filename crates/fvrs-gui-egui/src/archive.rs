use std::path::{Path, PathBuf};
use std::fs::File;
use std::io::BufReader;

/// サポートする圧縮ファイル形式
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ArchiveType {
    Zip,
    Lzh,
    Tar,
    TarGz,
    TarBz2,
    Gz,
    SevenZ,
    Rar,
    Cab,
    Unknown,
}

/// アーカイブ内のエントリ情報
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArchiveEntry {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub compressed_size: u64,
    pub is_dir: bool,
    pub modified: Option<chrono::DateTime<chrono::Utc>>,
}

/// 圧縮ファイル処理ユーティリティ
pub struct ArchiveHandler;

impl ArchiveHandler {
    /// ファイル拡張子から圧縮形式を判定
    pub fn detect_archive_type(file_path: &Path) -> ArchiveType {
        let extension = file_path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_lowercase();

        match extension.as_str() {
            "zip" | "jar" | "war" | "ear" => ArchiveType::Zip,
            "lzh" | "lha" => ArchiveType::Lzh,
            "tar" => ArchiveType::Tar,
            "tgz" | "tar.gz" => ArchiveType::TarGz,
            "tbz2" | "tar.bz2" => ArchiveType::TarBz2,
            "gz" => ArchiveType::Gz,
            "7z" => ArchiveType::SevenZ,
            "rar" => ArchiveType::Rar,
            "cab" => ArchiveType::Cab,
            _ => {
                // ファイル名全体をチェック（.tar.gz などの複合拡張子）
                let file_name = file_path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                
                if file_name.ends_with(".tar.gz") {
                    ArchiveType::TarGz
                } else if file_name.ends_with(".tar.bz2") {
                    ArchiveType::TarBz2
                } else {
                    ArchiveType::Unknown
                }
            }
        }
    }

    /// 圧縮ファイルかどうかを判定
    pub fn is_archive(file_path: &Path) -> bool {
        !matches!(Self::detect_archive_type(file_path), ArchiveType::Unknown)
    }

    /// 圧縮ファイルの内容を一覧表示
    pub fn list_archive_contents(file_path: &Path) -> Result<Vec<ArchiveEntry>, String> {
        let archive_type = Self::detect_archive_type(file_path);
        
        match archive_type {
            ArchiveType::Zip => Self::list_zip_contents(file_path),
            ArchiveType::Lzh => Self::list_lzh_contents(file_path),
            ArchiveType::Tar => Self::list_tar_contents(file_path),
            ArchiveType::TarGz => Self::list_tar_gz_contents(file_path),
            ArchiveType::TarBz2 => Self::list_tar_bz2_contents(file_path),
            ArchiveType::Gz => Self::list_gz_contents(file_path),
            ArchiveType::SevenZ => Self::list_7z_contents(file_path),
            ArchiveType::Rar => Self::list_rar_contents(file_path),
            ArchiveType::Cab => Self::list_cab_contents(file_path),
            _ => Err(format!("未対応の圧縮形式: {:?}", archive_type)),
        }
    }

    /// ZIP ファイルの内容を一覧表示
    fn list_zip_contents(file_path: &Path) -> Result<Vec<ArchiveEntry>, String> {
        let file = File::open(file_path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
        let reader = BufReader::new(file);
        let mut zip = zip::ZipArchive::new(reader).map_err(|e| format!("ZIP読み込みエラー: {}", e))?;
        
        let mut entries = Vec::new();
        
        for i in 0..zip.len() {
            match zip.by_index(i) {
                Ok(file) => {
                    let name = file.name().to_string();
                    let path = PathBuf::from(&name);
                    let size = file.size();
                    let compressed_size = file.compressed_size();
                    let is_dir = file.is_dir();
                    
                    let modified = file.last_modified()
                        .and_then(|dt| {
                            chrono::NaiveDate::from_ymd_opt(dt.year() as i32, dt.month() as u32, dt.day() as u32)
                                .and_then(|date| {
                                    date.and_hms_opt(dt.hour() as u32, dt.minute() as u32, dt.second() as u32)
                                })
                                .map(|naive_dt| chrono::DateTime::from_naive_utc_and_offset(naive_dt, chrono::Utc))
                        });
                    
                    entries.push(ArchiveEntry {
                        name,
                        path,
                        size,
                        compressed_size,
                        is_dir,
                        modified,
                    });
                }
                Err(e) => {
                    tracing::warn!("ZIP エントリ読み込みエラー {}: {}", i, e);
                }
            }
        }
        
        Ok(entries)
    }

    /// LZH ファイルの内容を一覧表示
    fn list_lzh_contents(file_path: &Path) -> Result<Vec<ArchiveEntry>, String> {
        let mut lha_reader = delharc::parse_file(file_path)
            .map_err(|e| format!("LZH読み込みエラー: {}", e))?;
        
        let mut entries = Vec::new();
        
        loop {
            let header = lha_reader.header();
            let name = header.parse_pathname().to_string_lossy().to_string();
            let path = PathBuf::from(&name);
            let size = header.original_size as u64;
            let compressed_size = header.compressed_size as u64;
            let is_dir = header.is_directory();
            
            let modified = None; // LZH タイムスタンプは簡略化
            
            entries.push(ArchiveEntry {
                name,
                path,
                size,
                compressed_size,
                is_dir,
                modified,
            });
            
            if !lha_reader.next_file().map_err(|e| format!("LZH次ファイルエラー: {}", e))? {
                break;
            }
        }
        
        Ok(entries)
    }

    /// TAR ファイルの内容を一覧表示
    fn list_tar_contents(file_path: &Path) -> Result<Vec<ArchiveEntry>, String> {
        let file = File::open(file_path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
        let mut tar = tar::Archive::new(file);
        
        let mut entries = Vec::new();
        
        for entry_result in tar.entries().map_err(|e| format!("TAR読み込みエラー: {}", e))? {
            match entry_result {
                Ok(mut entry) => {
                    let header = entry.header();
                    let path = entry.path().map_err(|e| format!("パス取得エラー: {}", e))?;
                    let name = path.to_string_lossy().to_string();
                    let size = header.size().unwrap_or(0);
                    let is_dir = header.entry_type().is_dir();
                    
                    let modified = header.mtime().ok()
                        .map(|ts| chrono::DateTime::from_timestamp(ts as i64, 0))
                        .flatten();
                    
                    entries.push(ArchiveEntry {
                        name,
                        path: path.into_owned(),
                        size,
                        compressed_size: size, // TAR は非圧縮
                        is_dir,
                        modified,
                    });
                    
                    // エントリの内容を完全に消費してブロック境界の問題を回避
                    if !is_dir {
                        let _ = std::io::copy(&mut entry, &mut std::io::sink());
                    }
                }
                Err(e) => {
                    tracing::warn!("TAR エントリ読み込みエラー: {}", e);
                    // エラーの場合も処理を続行
                    break;
                }
            }
        }
        
        Ok(entries)
    }

    /// TAR.GZ ファイルの内容を一覧表示
    fn list_tar_gz_contents(file_path: &Path) -> Result<Vec<ArchiveEntry>, String> {
        let file = File::open(file_path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
        let gz_decoder = flate2::read::GzDecoder::new(file);
        let mut tar = tar::Archive::new(gz_decoder);
        
        let mut entries = Vec::new();
        
        for entry_result in tar.entries().map_err(|e| format!("TAR.GZ読み込みエラー: {}", e))? {
            match entry_result {
                Ok(mut entry) => {
                    let header = entry.header();
                    let path = entry.path().map_err(|e| format!("パス取得エラー: {}", e))?;
                    let name = path.to_string_lossy().to_string();
                    let size = header.size().unwrap_or(0);
                    let is_dir = header.entry_type().is_dir();
                    
                    let modified = header.mtime().ok()
                        .map(|ts| chrono::DateTime::from_timestamp(ts as i64, 0))
                        .flatten();
                    
                    entries.push(ArchiveEntry {
                        name,
                        path: path.into_owned(),
                        size,
                        compressed_size: size, // 圧縮後サイズは取得困難
                        is_dir,
                        modified,
                    });
                    
                    // エントリの内容を完全に消費してブロック境界の問題を回避
                    if !is_dir {
                        let _ = std::io::copy(&mut entry, &mut std::io::sink());
                    }
                }
                Err(e) => {
                    tracing::warn!("TAR.GZ エントリ読み込みエラー: {}", e);
                    break;
                }
            }
        }
        
        Ok(entries)
    }

    /// TAR.BZ2 ファイルの内容を一覧表示
    fn list_tar_bz2_contents(file_path: &Path) -> Result<Vec<ArchiveEntry>, String> {
        let file = File::open(file_path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
        let bz2_decoder = bzip2::read::BzDecoder::new(file);
        let mut tar = tar::Archive::new(bz2_decoder);
        
        let mut entries = Vec::new();
        
        for entry_result in tar.entries().map_err(|e| format!("TAR.BZ2読み込みエラー: {}", e))? {
            match entry_result {
                Ok(mut entry) => {
                    let header = entry.header();
                    let path = entry.path().map_err(|e| format!("パス取得エラー: {}", e))?;
                    let name = path.to_string_lossy().to_string();
                    let size = header.size().unwrap_or(0);
                    let is_dir = header.entry_type().is_dir();
                    
                    let modified = header.mtime().ok()
                        .map(|ts| chrono::DateTime::from_timestamp(ts as i64, 0))
                        .flatten();
                    
                    entries.push(ArchiveEntry {
                        name,
                        path: path.into_owned(),
                        size,
                        compressed_size: size, // 圧縮後サイズは取得困難
                        is_dir,
                        modified,
                    });
                    
                    // エントリの内容を完全に消費してブロック境界の問題を回避
                    if !is_dir {
                        let _ = std::io::copy(&mut entry, &mut std::io::sink());
                    }
                }
                Err(e) => {
                    tracing::warn!("TAR.BZ2 エントリ読み込みエラー: {}", e);
                    break;
                }
            }
        }
        
        Ok(entries)
    }

    /// GZ ファイルの内容を一覧表示
    fn list_gz_contents(file_path: &Path) -> Result<Vec<ArchiveEntry>, String> {
        // .gz単体ファイルは1つのファイルが圧縮されている
        let file_name = file_path.file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("decompressed")
            .to_string();
        
        let file_size = std::fs::metadata(file_path)
            .map(|m| m.len())
            .unwrap_or(0);
        
        Ok(vec![ArchiveEntry {
            name: file_name.clone(),
            path: PathBuf::from(&file_name),
            size: 0, // 解凍後サイズは不明
            compressed_size: file_size,
            is_dir: false,
            modified: None,
        }])
    }

    /// 7Z ファイルの内容を一覧表示
    fn list_7z_contents(file_path: &Path) -> Result<Vec<ArchiveEntry>, String> {
        // sevenz-rust は現在読み込み専用実装のため、基本的な情報のみ取得
        match sevenz_rust::decompress_file(file_path, std::env::temp_dir()) {
            Ok(_) => {
                // 7Z の詳細なエントリ情報取得は複雑なため、
                // とりあえずファイルが読めることを確認
                Ok(vec![ArchiveEntry {
                    name: "7Z Archive Contents".to_string(),
                    path: PathBuf::from("7z_contents"),
                    size: 0,
                    compressed_size: 0,
                    is_dir: true,
                    modified: None,
                }])
            }
            Err(e) => Err(format!("7Z読み込みエラー: {}", e)),
        }
    }

    /// RAR ファイルの内容を一覧表示
    fn list_rar_contents(file_path: &Path) -> Result<Vec<ArchiveEntry>, String> {
        use unrar::Archive as UnrarArchive;
        
        let mut entries = Vec::new();
        
        // unrarライブラリを使用してRARファイルを開いて一覧表示
        let archive = UnrarArchive::new(file_path).open_for_listing()
            .map_err(|e| format!("RAR読み込みエラー: {:?}", e))?;
        
        for entry_result in archive {
            match entry_result {
                Ok(entry) => {
                    let name = entry.filename.to_string_lossy().to_string();
                    let path = entry.filename.clone();
                    let size = entry.unpacked_size;
                    let compressed_size = entry.unpacked_size; // RARではcompressed_sizeは取得困難
                    let is_dir = entry.is_directory();
                    
                    // unrarのファイル時刻はFileTimeで提供される
                    let modified = None; // RARのfile_timeは単純な数値のため、解析が複雑
                    
                    entries.push(ArchiveEntry {
                        name,
                        path,
                        size: size.into(),
                        compressed_size: compressed_size.into(),
                        is_dir,
                        modified,
                    });
                }
                Err(e) => {
                    tracing::warn!("RAR エントリ読み込みエラー: {:?}", e);
                }
            }
        }
        
        Ok(entries)
    }

    /// CAB ファイルの内容を一覧表示
    fn list_cab_contents(file_path: &Path) -> Result<Vec<ArchiveEntry>, String> {
        use cab::Cabinet;
        
        let mut entries = Vec::new();
        
        let file = File::open(file_path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
        let cabinet = Cabinet::new(file).map_err(|e| format!("CAB読み込みエラー: {:?}", e))?;
        
        // CABファイル内のフォルダとファイルを列挙
        for folder in cabinet.folder_entries() {
            for file_entry in folder.file_entries() {
                let name = file_entry.name().to_string();
                let path = PathBuf::from(&name);
                let size = file_entry.uncompressed_size();
                let compressed_size = size; // CABでは圧縮サイズの正確な取得が困難
                let is_dir = false; // CABではディレクトリの概念が異なる
                
                // CABファイルの時刻情報を取得（簡略化）
                let modified = file_entry.datetime()
                    .and_then(|dt| {
                        Some(chrono::NaiveDateTime::new(
                            chrono::NaiveDate::from_ymd_opt(dt.year() as i32, dt.month() as u32, dt.day() as u32)?,
                            chrono::NaiveTime::from_hms_opt(dt.hour() as u32, dt.minute() as u32, dt.second() as u32)?
                        ))
                    })
                    .map(|ndt| chrono::DateTime::from_naive_utc_and_offset(ndt, chrono::Utc));
                
                entries.push(ArchiveEntry {
                    name: name.clone(),
                    path,
                    size: size.into(),
                    compressed_size: compressed_size.into(),
                    is_dir,
                    modified,
                });
            }
        }
        
        Ok(entries)
    }

    /// 圧縮ファイルを指定ディレクトリに解凍
    pub fn extract_archive(archive_path: &Path, extract_to: &Path) -> Result<(), String> {
        let archive_type = Self::detect_archive_type(archive_path);
        
        std::fs::create_dir_all(extract_to)
            .map_err(|e| format!("解凍先ディレクトリ作成エラー: {}", e))?;
        
        match archive_type {
            ArchiveType::Zip => Self::extract_zip(archive_path, extract_to),
            ArchiveType::Lzh => Self::extract_lzh(archive_path, extract_to),
            ArchiveType::Tar => Self::extract_tar(archive_path, extract_to),
            ArchiveType::TarGz => Self::extract_tar_gz(archive_path, extract_to),
            ArchiveType::TarBz2 => Self::extract_tar_bz2(archive_path, extract_to),
            ArchiveType::Gz => Self::extract_gz(archive_path, extract_to),
            ArchiveType::SevenZ => Self::extract_7z(archive_path, extract_to),
            ArchiveType::Rar => Self::extract_rar(archive_path, extract_to),
            ArchiveType::Cab => Self::extract_cab(archive_path, extract_to),
            _ => Err(format!("未対応の圧縮形式: {:?}", archive_type)),
        }
    }

    /// ZIP ファイルを解凍
    fn extract_zip(archive_path: &Path, extract_to: &Path) -> Result<(), String> {
        let file = File::open(archive_path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
        let reader = BufReader::new(file);
        let mut zip = zip::ZipArchive::new(reader).map_err(|e| format!("ZIP読み込みエラー: {}", e))?;
        
        for i in 0..zip.len() {
            let mut file = zip.by_index(i).map_err(|e| format!("ZIP エントリ取得エラー: {}", e))?;
            let outpath = extract_to.join(file.name());
            
            if file.is_dir() {
                std::fs::create_dir_all(&outpath)
                    .map_err(|e| format!("ディレクトリ作成エラー: {}", e))?;
            } else {
                if let Some(parent) = outpath.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("親ディレクトリ作成エラー: {}", e))?;
                }
                
                let mut outfile = File::create(&outpath)
                    .map_err(|e| format!("ファイル作成エラー: {}", e))?;
                std::io::copy(&mut file, &mut outfile)
                    .map_err(|e| format!("ファイル書き込みエラー: {}", e))?;
            }
        }
        
        Ok(())
    }

    /// LZH ファイルを解凍
    fn extract_lzh(archive_path: &Path, extract_to: &Path) -> Result<(), String> {
        let mut lha_reader = delharc::parse_file(archive_path)
            .map_err(|e| format!("LZH読み込みエラー: {}", e))?;
        
        loop {
            let header = lha_reader.header();
            let filename = header.parse_pathname();
            let outpath = extract_to.join(&*filename);
            
            if header.is_directory() {
                std::fs::create_dir_all(&outpath)
                    .map_err(|e| format!("ディレクトリ作成エラー: {}", e))?;
            } else {
                if let Some(parent) = outpath.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("親ディレクトリ作成エラー: {}", e))?;
                }
                
                if lha_reader.is_decoder_supported() {
                    let mut outfile = File::create(&outpath)
                        .map_err(|e| format!("ファイル作成エラー: {}", e))?;
                    std::io::copy(&mut lha_reader, &mut outfile)
                        .map_err(|e| format!("ファイル書き込みエラー: {}", e))?;
                    
                    lha_reader.crc_check()
                        .map_err(|e| format!("CRCチェックエラー: {}", e))?;
                } else {
                    tracing::warn!("未対応の圧縮方式のファイルをスキップ: {:?}", filename);
                }
            }
            
            if !lha_reader.next_file().map_err(|e| format!("LZH次ファイルエラー: {}", e))? {
                break;
            }
        }
        
        Ok(())
    }

    /// TAR ファイルを解凍
    fn extract_tar(archive_path: &Path, extract_to: &Path) -> Result<(), String> {
        let file = File::open(archive_path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
        let mut tar = tar::Archive::new(file);
        
        tar.unpack(extract_to).map_err(|e| format!("TAR解凍エラー: {}", e))?;
        
        Ok(())
    }

    /// TAR.GZ ファイルを解凍
    fn extract_tar_gz(archive_path: &Path, extract_to: &Path) -> Result<(), String> {
        let file = File::open(archive_path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
        let gz_decoder = flate2::read::GzDecoder::new(file);
        let mut tar = tar::Archive::new(gz_decoder);
        
        tar.unpack(extract_to).map_err(|e| format!("TAR.GZ解凍エラー: {}", e))?;
        
        Ok(())
    }

    /// TAR.BZ2 ファイルを解凍
    fn extract_tar_bz2(archive_path: &Path, extract_to: &Path) -> Result<(), String> {
        let file = File::open(archive_path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
        let bz2_decoder = bzip2::read::BzDecoder::new(file);
        let mut tar = tar::Archive::new(bz2_decoder);
        
        tar.unpack(extract_to).map_err(|e| format!("TAR.BZ2解凍エラー: {}", e))?;
        
        Ok(())
    }

    /// GZ ファイルを解凍
    fn extract_gz(archive_path: &Path, extract_to: &Path) -> Result<(), String> {
        let file = File::open(archive_path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
        let mut gz_decoder = flate2::read::GzDecoder::new(file);
        
        // 出力ファイル名を決定（.gz拡張子を除去）
        let output_filename = archive_path.file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("decompressed");
        let output_path = extract_to.join(output_filename);
        
        let mut output_file = File::create(&output_path)
            .map_err(|e| format!("出力ファイル作成エラー: {}", e))?;
        
        std::io::copy(&mut gz_decoder, &mut output_file)
            .map_err(|e| format!("GZ解凍エラー: {}", e))?;
        
        Ok(())
    }

    /// 7Z ファイルを解凍
    fn extract_7z(archive_path: &Path, extract_to: &Path) -> Result<(), String> {
        sevenz_rust::decompress_file(archive_path, extract_to)
            .map_err(|e| format!("7Z解凍エラー: {}", e))?;
        
        Ok(())
    }

    /// RAR ファイルを解凍
    fn extract_rar(archive_path: &Path, extract_to: &Path) -> Result<(), String> {
        use unrar::Archive as UnrarArchive;
        use std::fs;
        
        // 解凍先ディレクトリを作成
        fs::create_dir_all(extract_to).map_err(|e| format!("ディレクトリ作成エラー: {}", e))?;
        
        // unrarライブラリを使用してRARファイルを開いて解凍
        let archive = UnrarArchive::new(archive_path).open_for_processing()
            .map_err(|e| format!("RAR読み込みエラー: {:?}", e))?;
        
        let mut current_archive = Some(archive);
        
        while let Some(archive) = current_archive {
            current_archive = match archive.read_header() {
                Ok(Some(archive_with_header)) => {
                    let entry = archive_with_header.entry();
                    let target_path = extract_to.join(&entry.filename);
                    
                    // ディレクトリの場合は作成
                    if entry.is_directory() {
                        fs::create_dir_all(&target_path)
                            .map_err(|e| format!("ディレクトリ作成エラー: {}", e))?;
                        Some(archive_with_header.skip().map_err(|e| format!("RARスキップエラー: {:?}", e))?)
                    } else {
                        // ファイルの場合は解凍
                        if let Some(parent) = target_path.parent() {
                            fs::create_dir_all(parent)
                                .map_err(|e| format!("親ディレクトリ作成エラー: {}", e))?;
                        }
                        
                        let next_archive = archive_with_header.extract_to(&target_path)
                            .map_err(|e| format!("RAR解凍エラー: {:?}", e))?;
                        Some(next_archive)
                    }
                }
                Ok(None) => None,
                Err(e) => return Err(format!("RARヘッダ読み込みエラー: {:?}", e)),
            };
        }
        
        Ok(())
    }

    /// CAB ファイルを解凍
    fn extract_cab(archive_path: &Path, extract_to: &Path) -> Result<(), String> {
        use cab::Cabinet;
        use std::fs;
        
        // 解凍先ディレクトリを作成
        fs::create_dir_all(extract_to).map_err(|e| format!("ディレクトリ作成エラー: {}", e))?;
        
        let file = File::open(archive_path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
        let cabinet = Cabinet::new(file).map_err(|e| format!("CAB読み込みエラー: {:?}", e))?;
        
        // まずファイル名のリストを作成
        let mut file_list = Vec::new();
        for folder in cabinet.folder_entries() {
            for file_entry in folder.file_entries() {
                file_list.push(file_entry.name().to_string());
            }
        }
        
        // 借用問題を解決するために新しいcabinetインスタンスを作成
        let file2 = File::open(archive_path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
        let mut cabinet2 = Cabinet::new(file2).map_err(|e| format!("CAB読み込みエラー: {:?}", e))?;
        
        // ファイルを解凍
        for file_name in file_list {
            let target_path = extract_to.join(&file_name);
            
            // 親ディレクトリを作成
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("親ディレクトリ作成エラー: {}", e))?;
            }
            
            // ファイルを読み込んで書き出し
            let mut reader = cabinet2.read_file(&file_name)
                .map_err(|e| format!("CABファイル読み込みエラー: {:?}", e))?;
            let mut output_file = fs::File::create(&target_path)
                .map_err(|e| format!("出力ファイル作成エラー: {}", e))?;
            
            std::io::copy(&mut reader, &mut output_file)
                .map_err(|e| format!("ファイル書き込みエラー: {}", e))?;
        }
        
        Ok(())
    }

    /// ファイル・フォルダを圧縮
    pub fn create_archive(
        source_paths: &[PathBuf], 
        archive_path: &Path, 
        archive_type: ArchiveType
    ) -> Result<(), String> {
        match archive_type {
            ArchiveType::Zip => Self::create_zip(source_paths, archive_path),
            ArchiveType::Tar => Self::create_tar(source_paths, archive_path),
            ArchiveType::TarGz => Self::create_tar_gz(source_paths, archive_path),
            ArchiveType::TarBz2 => Self::create_tar_bz2(source_paths, archive_path),
            ArchiveType::Lzh => Err("LHA/LZH形式の作成は現在サポートされていません。解凍のみ対応しています。ZIP、TAR、またはTAR.GZ形式をご利用ください。".to_string()),
            ArchiveType::Rar => Err("RAR形式の作成はライセンス制限により対応していません。解凍のみサポートしています。".to_string()),
            ArchiveType::Cab => Err("CAB形式の作成は現在サポートされていません。解凍のみ対応しています。".to_string()),
            _ => Err(format!("作成未対応の圧縮形式: {:?}", archive_type)),
        }
    }

    /// ZIP ファイルを作成
    fn create_zip(source_paths: &[PathBuf], archive_path: &Path) -> Result<(), String> {
        let file = File::create(archive_path).map_err(|e| format!("ファイル作成エラー: {}", e))?;
        let mut zip = zip::ZipWriter::new(file);
        
        for source_path in source_paths {
            Self::add_to_zip(&mut zip, source_path, "")?;
        }
        
        zip.finish().map_err(|e| format!("ZIP完了エラー: {}", e))?;
        
        Ok(())
    }

    /// ZIP に再帰的にファイル・フォルダを追加
    fn add_to_zip<W: std::io::Write + std::io::Seek>(
        zip: &mut zip::ZipWriter<W>,
        source_path: &Path,
        base_path: &str,
    ) -> Result<(), String> {
        if source_path.is_file() {
            let name = if base_path.is_empty() {
                source_path.file_name().unwrap().to_string_lossy().to_string()
            } else {
                format!("{}/{}", base_path, source_path.file_name().unwrap().to_string_lossy())
            };
            
            zip.start_file(&name, zip::write::FileOptions::<()>::default())
                .map_err(|e| format!("ZIPファイル開始エラー: {}", e))?;
            
            let mut file = File::open(source_path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
            std::io::copy(&mut file, zip).map_err(|e| format!("ファイル書き込みエラー: {}", e))?;
        } else if source_path.is_dir() {
            let dir_name = if base_path.is_empty() {
                source_path.file_name().unwrap().to_string_lossy().to_string()
            } else {
                format!("{}/{}", base_path, source_path.file_name().unwrap().to_string_lossy())
            };
            
            for entry in std::fs::read_dir(source_path).map_err(|e| format!("ディレクトリ読み込みエラー: {}", e))? {
                let entry = entry.map_err(|e| format!("エントリ読み込みエラー: {}", e))?;
                Self::add_to_zip(zip, &entry.path(), &dir_name)?;
            }
        }
        
        Ok(())
    }

    /// TAR ファイルを作成
    fn create_tar(source_paths: &[PathBuf], archive_path: &Path) -> Result<(), String> {
        let file = File::create(archive_path).map_err(|e| format!("ファイル作成エラー: {}", e))?;
        let mut tar = tar::Builder::new(file);
        
        for source_path in source_paths {
            if source_path.is_file() {
                // ファイル名のみを使用して相対パスで追加
                let file_name = source_path.file_name()
                    .ok_or_else(|| "ファイル名を取得できません".to_string())?
                    .to_string_lossy();
                tar.append_path_with_name(source_path, &*file_name)
                    .map_err(|e| format!("TARファイル追加エラー: {}", e))?;
            } else if source_path.is_dir() {
                // ディレクトリの場合、ディレクトリ名を使用して相対パスで追加
                let dir_name = source_path.file_name()
                    .ok_or_else(|| "ディレクトリ名を取得できません".to_string())?
                    .to_string_lossy();
                tar.append_dir_all(&*dir_name, source_path)
                    .map_err(|e| format!("TARディレクトリ追加エラー: {}", e))?;
            }
        }
        
        tar.finish().map_err(|e| format!("TAR完了エラー: {}", e))?;
        
        Ok(())
    }

    /// TAR.GZ ファイルを作成
    fn create_tar_gz(source_paths: &[PathBuf], archive_path: &Path) -> Result<(), String> {
        let file = File::create(archive_path).map_err(|e| format!("ファイル作成エラー: {}", e))?;
        let gz_encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut tar = tar::Builder::new(gz_encoder);
        
        for source_path in source_paths {
            if source_path.is_file() {
                // ファイル名のみを使用して相対パスで追加
                let file_name = source_path.file_name()
                    .ok_or_else(|| "ファイル名を取得できません".to_string())?
                    .to_string_lossy();
                tar.append_path_with_name(source_path, &*file_name)
                    .map_err(|e| format!("TAR.GZファイル追加エラー: {}", e))?;
            } else if source_path.is_dir() {
                // ディレクトリの場合、ディレクトリ名を使用して相対パスで追加
                let dir_name = source_path.file_name()
                    .ok_or_else(|| "ディレクトリ名を取得できません".to_string())?
                    .to_string_lossy();
                tar.append_dir_all(&*dir_name, source_path)
                    .map_err(|e| format!("TAR.GZディレクトリ追加エラー: {}", e))?;
            }
        }
        
        tar.finish().map_err(|e| format!("TAR.GZ完了エラー: {}", e))?;
        
        Ok(())
    }

    /// TAR.BZ2 ファイルを作成
    fn create_tar_bz2(source_paths: &[PathBuf], archive_path: &Path) -> Result<(), String> {
        let file = File::create(archive_path).map_err(|e| format!("ファイル作成エラー: {}", e))?;
        let bz2_encoder = bzip2::write::BzEncoder::new(file, bzip2::Compression::default());
        let mut tar = tar::Builder::new(bz2_encoder);
        
        for source_path in source_paths {
            if source_path.is_file() {
                // ファイル名のみを使用して相対パスで追加
                let file_name = source_path.file_name()
                    .ok_or_else(|| "ファイル名を取得できません".to_string())?
                    .to_string_lossy();
                tar.append_path_with_name(source_path, &*file_name)
                    .map_err(|e| format!("TAR.BZ2ファイル追加エラー: {}", e))?;
            } else if source_path.is_dir() {
                // ディレクトリの場合、ディレクトリ名を使用して相対パスで追加
                let dir_name = source_path.file_name()
                    .ok_or_else(|| "ディレクトリ名を取得できません".to_string())?
                    .to_string_lossy();
                tar.append_dir_all(&*dir_name, source_path)
                    .map_err(|e| format!("TAR.BZ2ディレクトリ追加エラー: {}", e))?;
            }
        }
        
        tar.finish().map_err(|e| format!("TAR.BZ2完了エラー: {}", e))?;
        
        Ok(())
    }
} 