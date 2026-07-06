use std::fs::File;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use oxiarc_archive::{
    CabReader, GzipReader, TarHeader, TarReader, TarWriter, ZipReader, ZipWriter,
};
use oxiarc_core::Entry;

mod bzip2_compat;
mod lzh;
#[doc(hidden)]
pub mod lzhuf1;
mod lzma_compat;
mod name_codec;
mod sevenz;
mod zip_names;

/// TAR の ustar name フィールドに安全に収まる最大バイト数
/// （oxiarc の `write_string` は NUL 終端用に 1 バイト確保するため 100 ではなく 99）
const TAR_NAME_MAX: usize = 99;

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
        let extension = file_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_lowercase();

        // ファイル名全体（.tar.gz などの複合拡張子は extension() では "gz" になるため、
        // 全体名でも判定する）
        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_lowercase();

        match extension.as_str() {
            "zip" | "jar" | "war" | "ear" => ArchiveType::Zip,
            "lzh" | "lha" => ArchiveType::Lzh,
            "tar" => ArchiveType::Tar,
            "tgz" => ArchiveType::TarGz,
            "tbz2" => ArchiveType::TarBz2,
            "gz" if file_name.ends_with(".tar.gz") => ArchiveType::TarGz,
            "gz" => ArchiveType::Gz,
            "bz2" if file_name.ends_with(".tar.bz2") => ArchiveType::TarBz2,
            "7z" => ArchiveType::SevenZ,
            "rar" => ArchiveType::Rar,
            "cab" => ArchiveType::Cab,
            _ => ArchiveType::Unknown,
        }
    }

    /// 圧縮ファイルかどうかを判定
    pub fn is_archive(file_path: &Path) -> bool {
        !matches!(Self::detect_archive_type(file_path), ArchiveType::Unknown)
    }

    /// oxiarc のエントリを FVRS のエントリ情報に変換
    fn to_archive_entry(entry: &Entry) -> ArchiveEntry {
        ArchiveEntry {
            name: entry.name.clone(),
            path: PathBuf::from(&entry.name),
            size: entry.size,
            compressed_size: entry.compressed_size,
            is_dir: entry.is_dir(),
            modified: entry.modified.map(chrono::DateTime::<chrono::Utc>::from),
        }
    }

    /// エントリ名を無害化し、解凍先ディレクトリ内に収まる安全な出力パスを求める
    /// （`../` などによるパストラバーサル対策）
    fn safe_output_path(extract_to: &Path, entry: &Entry) -> Option<PathBuf> {
        let sanitized = entry.sanitized_name();
        if sanitized.is_empty() {
            tracing::warn!("不正なエントリパスをスキップ: {}", entry.name);
            None
        } else {
            Some(extract_to.join(sanitized))
        }
    }

    /// エントリ名（文字列）を無害化した出力パスを求める（パストラバーサル対策）
    fn safe_output_path_for_name(extract_to: &Path, name: &str) -> Option<PathBuf> {
        let mut sanitized = PathBuf::new();
        for component in Path::new(name).components() {
            if let std::path::Component::Normal(part) = component {
                sanitized.push(part);
            }
        }
        if sanitized.as_os_str().is_empty() {
            tracing::warn!("不正なエントリパスをスキップ: {}", name);
            None
        } else {
            Some(extract_to.join(sanitized))
        }
    }

    /// 出力パスの親ディレクトリを作成
    fn ensure_parent_dir(output_path: &Path) -> Result<(), String> {
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("親ディレクトリ作成エラー: {}", e))?;
        }
        Ok(())
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
        let zip = ZipReader::new(file).map_err(|e| format!("ZIP読み込みエラー: {}", e))?;

        // oxiarc は Shift_JIS 名を lossy 復号して潰すため、生バイト列から復元する
        let mut entries = zip.entries().to_vec();
        zip_names::refine_entry_names(file_path, &mut entries);

        Ok(entries.iter().map(Self::to_archive_entry).collect())
    }

    /// LZH ファイルの内容を一覧表示
    fn list_lzh_contents(file_path: &Path) -> Result<Vec<ArchiveEntry>, String> {
        let mut file =
            File::open(file_path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
        let entries = lzh::read_entries(&mut file)?;

        Ok(entries
            .iter()
            .map(|entry| ArchiveEntry {
                name: entry.name.clone(),
                path: PathBuf::from(&entry.name),
                size: entry.size,
                compressed_size: entry.compressed_size,
                is_dir: entry.is_dir,
                modified: entry
                    .mtime_unix
                    .and_then(|secs| chrono::DateTime::from_timestamp(secs, 0)),
            })
            .collect())
    }

    /// TAR ファイルの内容を一覧表示
    fn list_tar_contents(file_path: &Path) -> Result<Vec<ArchiveEntry>, String> {
        let file = File::open(file_path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
        let tar = TarReader::new(file).map_err(|e| format!("TAR読み込みエラー: {}", e))?;

        Ok(tar.entries().iter().map(Self::to_archive_entry).collect())
    }

    /// TAR.GZ ファイルの内容を一覧表示
    fn list_tar_gz_contents(file_path: &Path) -> Result<Vec<ArchiveEntry>, String> {
        let mut file =
            File::open(file_path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
        let data = oxiarc_archive::gzip::decompress(&mut file)
            .map_err(|e| format!("TAR.GZ読み込みエラー: {}", e))?;
        let tar = TarReader::new(Cursor::new(data))
            .map_err(|e| format!("TAR.GZ読み込みエラー: {}", e))?;

        Ok(tar.entries().iter().map(Self::to_archive_entry).collect())
    }

    /// TAR.BZ2 ファイルの内容を一覧表示
    ///
    /// bzip2 の復号は自前実装（`bzip2_compat`）を使用する。oxiarc-bzip2 0.3.3 は
    /// 実 libbz2 ストリームと非互換で、実在の .tar.bz2 を一切読めないため。
    fn list_tar_bz2_contents(file_path: &Path) -> Result<Vec<ArchiveEntry>, String> {
        let compressed =
            std::fs::read(file_path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
        let data = bzip2_compat::decompress(&compressed)
            .map_err(|e| format!("TAR.BZ2読み込みエラー: {}", e))?;
        let tar = TarReader::new(Cursor::new(data))
            .map_err(|e| format!("TAR.BZ2読み込みエラー: {}", e))?;

        Ok(tar.entries().iter().map(Self::to_archive_entry).collect())
    }

    /// GZ ファイルの内容を一覧表示
    fn list_gz_contents(file_path: &Path) -> Result<Vec<ArchiveEntry>, String> {
        let compressed_size = std::fs::metadata(file_path).map(|m| m.len()).unwrap_or(0);

        let file = File::open(file_path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
        let gz = GzipReader::new(file).map_err(|e| format!("GZ読み込みエラー: {}", e))?;

        // gzip ヘッダーの元ファイル名を優先し、なければ .gz を除いた名前を使用
        let file_name = Self::gz_output_name(gz.header().filename.as_deref(), file_path);

        let modified = if gz.header().mtime > 0 {
            chrono::DateTime::from_timestamp(gz.header().mtime as i64, 0)
        } else {
            None
        };

        Ok(vec![ArchiveEntry {
            name: file_name.clone(),
            path: PathBuf::from(&file_name),
            size: 0, // 解凍後サイズは不明
            compressed_size,
            is_dir: false,
            modified,
        }])
    }

    /// 7Z ファイルの内容を一覧表示
    fn list_7z_contents(file_path: &Path) -> Result<Vec<ArchiveEntry>, String> {
        let file = File::open(file_path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
        let sevenz =
            sevenz::SevenZArchive::new(file).map_err(|e| format!("7Z読み込みエラー: {}", e))?;

        Ok(sevenz
            .entries()
            .iter()
            .map(|entry| ArchiveEntry {
                name: entry.name.clone(),
                path: PathBuf::from(&entry.name),
                size: entry.size,
                compressed_size: entry.packed_size,
                is_dir: entry.is_dir,
                modified: entry.mtime.map(chrono::DateTime::<chrono::Utc>::from),
            })
            .collect())
    }

    /// RAR ファイルの内容を一覧表示
    #[cfg(feature = "rar")]
    fn list_rar_contents(file_path: &Path) -> Result<Vec<ArchiveEntry>, String> {
        use unrar::Archive as UnrarArchive;

        let mut entries = Vec::new();

        // unrarライブラリを使用してRARファイルを開いて一覧表示
        let archive = UnrarArchive::new(file_path)
            .open_for_listing()
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
                        size,
                        compressed_size,
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

    /// RAR ファイルの内容を一覧表示（RAR機能無効時）
    #[cfg(not(feature = "rar"))]
    fn list_rar_contents(_file_path: &Path) -> Result<Vec<ArchiveEntry>, String> {
        Err(Self::rar_disabled_message())
    }

    /// RAR 機能が無効な場合のエラーメッセージ
    #[cfg(not(feature = "rar"))]
    fn rar_disabled_message() -> String {
        "RAR形式はこのビルドでは無効化されています。`rar` フィーチャーを有効にしてビルドしてください（例: cargo build --features rar）。".to_string()
    }

    /// CAB ファイルの内容を一覧表示
    fn list_cab_contents(file_path: &Path) -> Result<Vec<ArchiveEntry>, String> {
        let file = File::open(file_path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
        let cab = CabReader::new(file).map_err(|e| format!("CAB読み込みエラー: {}", e))?;

        Ok(cab
            .entries()
            .iter()
            .map(|entry| {
                let mut archive_entry = Self::to_archive_entry(entry);
                // oxiarc の CAB は個別の圧縮後サイズを 0 で返すため、
                // 旧実装と同じく非圧縮サイズで近似する（一覧の 0 表示回避）
                if archive_entry.compressed_size == 0 && !archive_entry.is_dir {
                    archive_entry.compressed_size = archive_entry.size;
                }
                archive_entry
            })
            .collect())
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
        let file =
            File::open(archive_path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
        let mut zip = ZipReader::new(file).map_err(|e| format!("ZIP読み込みエラー: {}", e))?;

        // Shift_JIS 名の衝突（U+FFFD 潰れによる上書き消失）を防ぐため名前を復元する。
        // 抽出自体は Entry::offset ベースのため名前の置き換えは抽出に影響しない。
        let mut entries = zip.entries().to_vec();
        zip_names::refine_entry_names(archive_path, &mut entries);

        for entry in &entries {
            let Some(outpath) = Self::safe_output_path(extract_to, entry) else {
                continue;
            };

            if entry.is_dir() {
                std::fs::create_dir_all(&outpath)
                    .map_err(|e| format!("ディレクトリ作成エラー: {}", e))?;
            } else {
                Self::ensure_parent_dir(&outpath)?;

                let data = zip
                    .extract(entry)
                    .map_err(|e| format!("ZIP エントリ取得エラー: {}", e))?;
                std::fs::write(&outpath, data)
                    .map_err(|e| format!("ファイル書き込みエラー: {}", e))?;
            }
        }

        Ok(())
    }

    /// LZH ファイルを解凍
    fn extract_lzh(archive_path: &Path, extract_to: &Path) -> Result<(), String> {
        let mut file =
            File::open(archive_path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
        let entries = lzh::read_entries(&mut file)?;

        for entry in &entries {
            let Some(outpath) = Self::safe_output_path_for_name(extract_to, &entry.name) else {
                continue;
            };

            if entry.is_dir {
                std::fs::create_dir_all(&outpath)
                    .map_err(|e| format!("ディレクトリ作成エラー: {}", e))?;
            } else {
                // CRC-16 検証は read_entry_data 内部で実施される
                match lzh::read_entry_data(&mut file, entry)? {
                    lzh::LzhData::Ok(data) => {
                        Self::ensure_parent_dir(&outpath)?;
                        std::fs::write(&outpath, data)
                            .map_err(|e| format!("ファイル書き込みエラー: {}", e))?;
                    }
                    lzh::LzhData::Unsupported(method) => {
                        tracing::warn!(
                            "未対応の圧縮方式 ({}) のファイルをスキップ: {}",
                            method,
                            entry.name
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// TAR リーダーから全エントリを解凍（TAR / TAR.GZ / TAR.BZ2 共通）
    fn extract_tar_entries<R: std::io::Read + std::io::Seek>(
        tar: &mut TarReader<R>,
        extract_to: &Path,
    ) -> Result<(), String> {
        let entries = tar.entries().to_vec();
        for entry in &entries {
            let Some(outpath) = Self::safe_output_path(extract_to, entry) else {
                continue;
            };

            if entry.is_dir() {
                std::fs::create_dir_all(&outpath)
                    .map_err(|e| format!("ディレクトリ作成エラー: {}", e))?;
            } else if entry.is_file() {
                Self::ensure_parent_dir(&outpath)?;

                let mut outfile =
                    File::create(&outpath).map_err(|e| format!("ファイル作成エラー: {}", e))?;
                tar.extract(entry, &mut outfile)
                    .map_err(|e| format!("TAR解凍エラー: {}", e))?;
            } else {
                // シンボリックリンク等は安全のため作成しない
                tracing::warn!("未対応のTARエントリ種別をスキップ: {}", entry.name);
            }
        }

        Ok(())
    }

    /// TAR ファイルを解凍
    fn extract_tar(archive_path: &Path, extract_to: &Path) -> Result<(), String> {
        let file =
            File::open(archive_path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
        let mut tar = TarReader::new(file).map_err(|e| format!("TAR読み込みエラー: {}", e))?;

        Self::extract_tar_entries(&mut tar, extract_to)
    }

    /// TAR.GZ ファイルを解凍
    fn extract_tar_gz(archive_path: &Path, extract_to: &Path) -> Result<(), String> {
        let mut file =
            File::open(archive_path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
        let data = oxiarc_archive::gzip::decompress(&mut file)
            .map_err(|e| format!("TAR.GZ読み込みエラー: {}", e))?;
        let mut tar = TarReader::new(Cursor::new(data))
            .map_err(|e| format!("TAR.GZ読み込みエラー: {}", e))?;

        Self::extract_tar_entries(&mut tar, extract_to)
    }

    /// TAR.BZ2 ファイルを解凍
    fn extract_tar_bz2(archive_path: &Path, extract_to: &Path) -> Result<(), String> {
        let compressed =
            std::fs::read(archive_path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
        let data = bzip2_compat::decompress(&compressed)
            .map_err(|e| format!("TAR.BZ2読み込みエラー: {}", e))?;
        let mut tar = TarReader::new(Cursor::new(data))
            .map_err(|e| format!("TAR.BZ2読み込みエラー: {}", e))?;

        Self::extract_tar_entries(&mut tar, extract_to)
    }

    /// GZ の出力ファイル名を決定（ヘッダーの元ファイル名を優先、.gz を除いた名前で代替）
    fn gz_output_name(header_filename: Option<&str>, archive_path: &Path) -> String {
        header_filename
            .and_then(|name| Path::new(name).file_name())
            .and_then(|name| name.to_str())
            .map(str::to_string)
            .or_else(|| {
                archive_path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "decompressed".to_string())
    }

    /// GZ ファイルを解凍
    fn extract_gz(archive_path: &Path, extract_to: &Path) -> Result<(), String> {
        let file =
            File::open(archive_path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
        let mut gz = GzipReader::new(file).map_err(|e| format!("GZ読み込みエラー: {}", e))?;

        // 出力ファイル名を決定（gzip ヘッダーの元ファイル名を優先）
        let output_filename = Self::gz_output_name(gz.header().filename.as_deref(), archive_path);
        let output_path = extract_to.join(&output_filename);

        let data = gz
            .decompress()
            .map_err(|e| format!("GZ解凍エラー: {}", e))?;

        std::fs::write(&output_path, data).map_err(|e| format!("出力ファイル作成エラー: {}", e))?;

        Ok(())
    }

    /// 7Z ファイルを解凍
    fn extract_7z(archive_path: &Path, extract_to: &Path) -> Result<(), String> {
        let file =
            File::open(archive_path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
        let mut sevenz =
            sevenz::SevenZArchive::new(file).map_err(|e| format!("7Z読み込みエラー: {}", e))?;

        for index in 0..sevenz.entries().len() {
            let (name, is_dir, is_anti) = {
                let entry = &sevenz.entries()[index];
                (entry.name.clone(), entry.is_dir, entry.is_anti)
            };

            // アンチファイル（削除マーカー）は作成しない
            if is_anti {
                continue;
            }

            let Some(outpath) = Self::safe_output_path_for_name(extract_to, &name) else {
                continue;
            };

            if is_dir {
                std::fs::create_dir_all(&outpath)
                    .map_err(|e| format!("ディレクトリ作成エラー: {}", e))?;
            } else {
                Self::ensure_parent_dir(&outpath)?;

                // 空ファイルを含め CRC 検証済みのデータが返る
                let data = sevenz
                    .read_file_data(index)
                    .map_err(|e| format!("7Z解凍エラー: {}", e))?;
                std::fs::write(&outpath, data)
                    .map_err(|e| format!("ファイル書き込みエラー: {}", e))?;
            }
        }

        Ok(())
    }

    /// RAR エントリのパスを無害化して相対パスに変換（パストラバーサル対策）
    #[cfg(feature = "rar")]
    fn sanitize_relative_path(name: &Path) -> Option<PathBuf> {
        let mut sanitized = PathBuf::new();
        for component in name.components() {
            if let std::path::Component::Normal(part) = component {
                sanitized.push(part);
            }
        }

        if sanitized.as_os_str().is_empty() {
            None
        } else {
            Some(sanitized)
        }
    }

    /// RAR ファイルを解凍
    #[cfg(feature = "rar")]
    fn extract_rar(archive_path: &Path, extract_to: &Path) -> Result<(), String> {
        use std::fs;
        use unrar::Archive as UnrarArchive;

        // 解凍先ディレクトリを作成
        fs::create_dir_all(extract_to).map_err(|e| format!("ディレクトリ作成エラー: {}", e))?;

        // unrarライブラリを使用してRARファイルを開いて解凍
        let archive = UnrarArchive::new(archive_path)
            .open_for_processing()
            .map_err(|e| format!("RAR読み込みエラー: {:?}", e))?;

        let mut current_archive = Some(archive);

        while let Some(archive) = current_archive {
            current_archive = match archive.read_header() {
                Ok(Some(archive_with_header)) => {
                    let entry = archive_with_header.entry();

                    // パストラバーサル対策: 不正なパスのエントリはスキップ
                    match Self::sanitize_relative_path(&entry.filename) {
                        None => {
                            tracing::warn!("不正なRARエントリパスをスキップ: {:?}", entry.filename);
                            Some(
                                archive_with_header
                                    .skip()
                                    .map_err(|e| format!("RARスキップエラー: {:?}", e))?,
                            )
                        }
                        Some(relative_path) => {
                            let target_path = extract_to.join(relative_path);

                            // ディレクトリの場合は作成
                            if entry.is_directory() {
                                fs::create_dir_all(&target_path)
                                    .map_err(|e| format!("ディレクトリ作成エラー: {}", e))?;
                                Some(
                                    archive_with_header
                                        .skip()
                                        .map_err(|e| format!("RARスキップエラー: {:?}", e))?,
                                )
                            } else {
                                // ファイルの場合は解凍
                                if let Some(parent) = target_path.parent() {
                                    fs::create_dir_all(parent)
                                        .map_err(|e| format!("親ディレクトリ作成エラー: {}", e))?;
                                }

                                let next_archive = archive_with_header
                                    .extract_to(&target_path)
                                    .map_err(|e| format!("RAR解凍エラー: {:?}", e))?;
                                Some(next_archive)
                            }
                        }
                    }
                }
                Ok(None) => None,
                Err(e) => return Err(format!("RARヘッダ読み込みエラー: {:?}", e)),
            };
        }

        Ok(())
    }

    /// RAR ファイルを解凍（RAR機能無効時）
    #[cfg(not(feature = "rar"))]
    fn extract_rar(_archive_path: &Path, _extract_to: &Path) -> Result<(), String> {
        Err(Self::rar_disabled_message())
    }

    /// CAB ファイルを解凍
    fn extract_cab(archive_path: &Path, extract_to: &Path) -> Result<(), String> {
        let file =
            File::open(archive_path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
        let mut cab = CabReader::new(file).map_err(|e| format!("CAB読み込みエラー: {}", e))?;

        let entries = cab.entries().to_vec();
        for entry in &entries {
            let Some(outpath) = Self::safe_output_path(extract_to, entry) else {
                continue;
            };

            if entry.is_dir() {
                std::fs::create_dir_all(&outpath)
                    .map_err(|e| format!("ディレクトリ作成エラー: {}", e))?;
            } else {
                Self::ensure_parent_dir(&outpath)?;

                let data = cab
                    .extract(entry)
                    .map_err(|e| format!("CABファイル読み込みエラー: {}", e))?;
                std::fs::write(&outpath, data)
                    .map_err(|e| format!("ファイル書き込みエラー: {}", e))?;
            }
        }

        Ok(())
    }

    /// ファイル・フォルダを圧縮
    pub fn create_archive(
        source_paths: &[PathBuf],
        archive_path: &Path,
        archive_type: ArchiveType,
    ) -> Result<(), String> {
        match archive_type {
            ArchiveType::Zip => Self::create_zip(source_paths, archive_path),
            ArchiveType::Tar => Self::create_tar(source_paths, archive_path),
            ArchiveType::TarGz => Self::create_tar_gz(source_paths, archive_path),
            ArchiveType::TarBz2 => Self::create_tar_bz2(source_paths, archive_path),
            ArchiveType::Lzh => Self::create_lzh(source_paths, archive_path),
            ArchiveType::Rar => Err(
                "RAR形式の作成はライセンス制限により対応していません。解凍のみサポートしています。"
                    .to_string(),
            ),
            ArchiveType::Cab => Err(
                "CAB形式の作成は現在サポートされていません。解凍のみ対応しています。".to_string(),
            ),
            _ => Err(format!("作成未対応の圧縮形式: {:?}", archive_type)),
        }
    }

    /// アーカイブ内のエントリ名を決定
    fn archive_entry_name(source_path: &Path, base_path: &str) -> Result<String, String> {
        let file_name = source_path
            .file_name()
            .ok_or_else(|| "ファイル名を取得できません".to_string())?
            .to_string_lossy();

        if base_path.is_empty() {
            Ok(file_name.to_string())
        } else {
            Ok(format!("{}/{}", base_path, file_name))
        }
    }

    /// ファイル・フォルダを再帰的に走査してアーカイブへ追加
    /// （`add_entry(エントリ名, 元パス, ディレクトリか)` を各エントリに対して呼び出す）
    fn add_sources_recursively<F>(
        source_path: &Path,
        base_path: &str,
        add_entry: &mut F,
    ) -> Result<(), String>
    where
        F: FnMut(&str, &Path, bool) -> Result<(), String>,
    {
        let entry_name = Self::archive_entry_name(source_path, base_path)?;

        if source_path.is_file() {
            add_entry(&entry_name, source_path, false)?;
        } else if source_path.is_dir() {
            add_entry(&entry_name, source_path, true)?;

            for dir_entry in std::fs::read_dir(source_path)
                .map_err(|e| format!("ディレクトリ読み込みエラー: {}", e))?
            {
                let dir_entry = dir_entry.map_err(|e| format!("エントリ読み込みエラー: {}", e))?;
                Self::add_sources_recursively(&dir_entry.path(), &entry_name, add_entry)?;
            }
        }

        Ok(())
    }

    /// 元ファイルのパーミッションを取得（Unix 以外は既定値）
    #[cfg(unix)]
    fn source_mode(source_path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;

        std::fs::metadata(source_path)
            .map(|m| m.permissions().mode() & 0o7777)
            .unwrap_or(0o644)
    }

    /// 元ファイルのパーミッションを取得（Unix 以外は既定値）
    #[cfg(not(unix))]
    fn source_mode(_source_path: &Path) -> u32 {
        0o644
    }

    /// 元ファイルの更新時刻（UNIXエポック秒）を取得
    fn source_mtime_secs(source_path: &Path) -> Option<u64> {
        std::fs::metadata(source_path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
    }

    /// ZIP ファイルを作成
    fn create_zip(source_paths: &[PathBuf], archive_path: &Path) -> Result<(), String> {
        let file = File::create(archive_path).map_err(|e| format!("ファイル作成エラー: {}", e))?;
        let mut zip = ZipWriter::new(file);

        for source_path in source_paths {
            Self::add_sources_recursively(source_path, "", &mut |name, path, is_dir| {
                if is_dir {
                    zip.add_directory(name)
                        .map_err(|e| format!("ZIPディレクトリ追加エラー: {}", e))
                } else {
                    let data = std::fs::read(path)
                        .map_err(|e| format!("ファイルオープンエラー: {}", e))?;
                    zip.add_file(name, &data)
                        .map_err(|e| format!("ZIPファイル追加エラー: {}", e))
                }
            })?;
        }

        zip.finish().map_err(|e| format!("ZIP完了エラー: {}", e))?;

        Ok(())
    }

    /// LZH ファイルを作成
    ///
    /// oxiarc-archive 0.3.3 の `LzhWriter` は名前を UTF-8 生バイトで書き込み、
    /// LZH の慣習 (Shift_JIS) と非互換で日本語名が再オープン時に化けるため、
    /// 自前ライター（レベル 2 ヘッダー・Shift_JIS 名・lh0 格納）を使用する。
    /// lh0 格納なのは oxiarc の lh5 エンコーダーに 8KB 超データの往復破損が
    /// あるためで、上流修正までデータの完全性を優先する。
    fn create_lzh(source_paths: &[PathBuf], archive_path: &Path) -> Result<(), String> {
        let file = File::create(archive_path).map_err(|e| format!("ファイル作成エラー: {}", e))?;
        let mut lzh = lzh::LzhWriter2::new(file);

        for source_path in source_paths {
            Self::add_sources_recursively(source_path, "", &mut |name, path, is_dir| {
                let mtime = Self::source_mtime_secs(path)
                    .and_then(|secs| u32::try_from(secs).ok())
                    .unwrap_or(0);
                if is_dir {
                    lzh.add_directory(name, mtime)
                        .map_err(|e| format!("LZHディレクトリ追加エラー: {}", e))
                } else {
                    let data = std::fs::read(path)
                        .map_err(|e| format!("ファイルオープンエラー: {}", e))?;
                    lzh.add_file(name, &data, mtime)
                        .map_err(|e| format!("LZHファイル追加エラー: {}", e))
                }
            })?;
        }

        lzh.finish().map_err(|e| format!("LZH完了エラー: {}", e))?;

        Ok(())
    }

    /// PAX 拡張ヘッダーの path レコードを構築する（"長さ path=値\n" 形式）
    fn pax_path_record(path: &str) -> Vec<u8> {
        // レコード長は自身の桁数を含むため不動点を求める
        let base = 1 + "path=".len() + path.len() + 1; // 空白 + キー + 値 + 改行
        let mut total = base + 1;
        loop {
            let digits = total.to_string().len();
            if digits + base == total {
                break;
            }
            total = digits + base;
        }
        format!("{} path={}\n", total, path).into_bytes()
    }

    /// ustar name フィールド用のフォールバック名（末尾を文字境界で切り詰める）
    fn tar_fallback_name(name: &str) -> String {
        let had_trailing_slash = name.ends_with('/');
        let trimmed = name.trim_end_matches('/');
        let budget = if had_trailing_slash {
            TAR_NAME_MAX - 1
        } else {
            TAR_NAME_MAX
        };
        let mut start = trimmed.len().saturating_sub(budget);
        while start < trimmed.len() && !trimmed.is_char_boundary(start) {
            start += 1;
        }
        let mut fallback = trimmed[start..].to_string();
        if had_trailing_slash {
            fallback.push('/');
        }
        if fallback.trim_end_matches('/').is_empty() {
            fallback = "long_name".to_string();
        }
        fallback
    }

    /// 長い名前を PAX 拡張ヘッダーで安全に処理しつつ TAR エントリを書き込む
    ///
    /// oxiarc の `TarHeader::to_block` は名前が 100 バイトを超えると
    /// `name[..155]` のスライスで文字境界を無視してパニックする
    /// （日本語ファイル名で容易に発生）ため、100 バイト超は自前で
    /// PAX path レコードを先行させ、ヘッダー側には文字境界で切り詰めた
    /// フォールバック名を渡す。PAX 対応リーダー（oxiarc 自身・GNU tar・
    /// bsdtar）は完全な名前を復元する。
    fn add_tar_entry_safe<W: std::io::Write>(
        tar: &mut TarWriter<W>,
        mut header: TarHeader,
        data: &[u8],
        label: &str,
    ) -> Result<(), String> {
        if header.name.len() > TAR_NAME_MAX {
            let pax_data = Self::pax_path_record(&header.name);
            let mut pax_header = TarHeader::new_file("PaxHeader", pax_data.len() as u64, 0o644);
            pax_header.typeflag = b'x';
            pax_header.mtime = header.mtime;
            tar.add_entry_from_header(&pax_header, &pax_data)
                .map_err(|e| format!("{}PAXヘッダー追加エラー: {}", label, e))?;
            header.name = Self::tar_fallback_name(&header.name);
        }

        tar.add_entry_from_header(&header, data)
            .map_err(|e| format!("{}エントリ追加エラー: {}", label, e))
    }

    /// TAR ライターへ全ソースを追加（TAR / TAR.GZ / TAR.BZ2 共通）
    fn add_sources_to_tar<W: std::io::Write>(
        tar: &mut TarWriter<W>,
        source_paths: &[PathBuf],
        label: &str,
    ) -> Result<(), String> {
        for source_path in source_paths {
            Self::add_sources_recursively(source_path, "", &mut |name, path, is_dir| {
                if is_dir {
                    let dir_name = if name.ends_with('/') {
                        name.to_string()
                    } else {
                        format!("{}/", name)
                    };
                    let mut header = TarHeader::new_directory(&dir_name, 0o755);
                    if let Some(mtime) = Self::source_mtime_secs(path) {
                        header.mtime = mtime;
                    }
                    Self::add_tar_entry_safe(tar, header, &[], label)
                } else {
                    let data = std::fs::read(path)
                        .map_err(|e| format!("ファイルオープンエラー: {}", e))?;

                    // 元ファイルのパーミッションと更新時刻を保持
                    let mut header =
                        TarHeader::new_file(name, data.len() as u64, Self::source_mode(path));
                    if let Some(mtime) = Self::source_mtime_secs(path) {
                        header.mtime = mtime;
                    }

                    Self::add_tar_entry_safe(tar, header, &data, label)
                }
            })?;
        }

        Ok(())
    }

    /// TAR ファイルを作成
    fn create_tar(source_paths: &[PathBuf], archive_path: &Path) -> Result<(), String> {
        let file = File::create(archive_path).map_err(|e| format!("ファイル作成エラー: {}", e))?;
        let mut tar = TarWriter::new(file);

        Self::add_sources_to_tar(&mut tar, source_paths, "TAR")?;

        tar.finish().map_err(|e| format!("TAR完了エラー: {}", e))?;

        Ok(())
    }

    /// TAR.GZ ファイルを作成
    fn create_tar_gz(source_paths: &[PathBuf], archive_path: &Path) -> Result<(), String> {
        let file = File::create(archive_path).map_err(|e| format!("ファイル作成エラー: {}", e))?;
        // レベル6 = flate2::Compression::default() 相当
        let encoder = oxiarc_deflate::GzipStreamEncoder::new(file, 6);
        let mut tar = TarWriter::new(encoder);

        Self::add_sources_to_tar(&mut tar, source_paths, "TAR.GZ")?;

        tar.finish()
            .map_err(|e| format!("TAR.GZ完了エラー: {}", e))?;
        let encoder = tar
            .into_inner()
            .map_err(|e| format!("TAR.GZ完了エラー: {}", e))?;
        encoder
            .finish()
            .map_err(|e| format!("GZ圧縮完了エラー: {}", e))?;

        Ok(())
    }

    /// TAR.BZ2 ファイルを作成
    ///
    /// bzip2 の符号化は自前実装（`bzip2_compat`）を使用する。oxiarc-bzip2 0.3.3 の
    /// 出力は libbz2 / bsdtar 等の標準ツールで解凍できない非互換形式のため。
    fn create_tar_bz2(source_paths: &[PathBuf], archive_path: &Path) -> Result<(), String> {
        let mut tar = TarWriter::new(Vec::new());

        Self::add_sources_to_tar(&mut tar, source_paths, "TAR.BZ2")?;

        tar.finish()
            .map_err(|e| format!("TAR.BZ2完了エラー: {}", e))?;
        let tar_bytes = tar
            .into_inner()
            .map_err(|e| format!("TAR.BZ2完了エラー: {}", e))?;

        // レベル6 = bzip2::Compression::default() 相当
        let compressed =
            bzip2_compat::compress(&tar_bytes, 6).map_err(|e| format!("BZ2圧縮エラー: {}", e))?;

        std::fs::write(archive_path, compressed)
            .map_err(|e| format!("ファイル作成エラー: {}", e))?;

        Ok(())
    }
}
