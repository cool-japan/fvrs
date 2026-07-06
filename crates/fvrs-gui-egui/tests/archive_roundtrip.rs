//! ArchiveHandler (oxiarc バックエンド) の回帰テスト
//!
//! - 作成可能な形式 (ZIP / TAR / TAR.GZ / TAR.BZ2 / LZH) のラウンドトリップ
//!   （日本語名の厳密一致を含む。LZH は lh5 で実際に圧縮されることも検証）
//! - 解凍専用形式 (GZ / 7Z) の一覧・解凍（内容のバイト一致を含む）
//! - 7Z 一覧表示が一時ディレクトリを汚染しないこと（旧実装の回帰確認）
//! - 7Z のサブストリーム・空ファイル・ディレクトリの取り扱い
//! - Shift_JIS 名の ZIP（EFS フラグ無し）で名前が衝突せず全ファイル解凍されること
//! - 100 バイト超の日本語ファイル名を含む TAR / TAR.GZ の作成がパニックしないこと
//! - `-lhd-` / `-lh1-` を含む LZH の一覧・解凍
//! - Zip Slip（パストラバーサル）対策
//! - RAR フィーチャー無効時のエラー処理
//!
//! すべてのファイル I/O は `std::env::temp_dir()` 配下の専用サブディレクトリで行い、
//! テスト終了時（パニック時も含む）に削除する。

use std::fs;
use std::path::{Path, PathBuf};

use fvrs_gui_egui::archive::{ArchiveHandler, ArchiveType};

/// テスト用一時ディレクトリ（Drop で自動削除）
struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(label: &str) -> Result<Self, String> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| format!("システム時刻取得エラー: {}", e))?
            .subsec_nanos();
        let path = std::env::temp_dir().join(format!(
            "fvrs_archive_test_{}_{}_{}",
            label,
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&path).map_err(|e| format!("一時ディレクトリ作成エラー: {}", e))?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// 決定論的な擬似ランダムバイナリデータを生成（xorshift32）
fn make_binary_data(len: usize) -> Vec<u8> {
    let mut state: u32 = 0x2545_f491;
    let mut out = Vec::with_capacity(len + 4);
    while out.len() < len {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        out.extend_from_slice(&state.to_le_bytes());
    }
    out.truncate(len);
    out
}

/// ラウンドトリップ対象のソースツリーを構築し、(相対パス, 内容) の一覧を返す
///
/// 内容: ネストしたディレクトリ、日本語ファイル名、空ファイル、約100KBのバイナリ、空ディレクトリ
fn build_source_tree(root: &Path) -> Result<Vec<(PathBuf, Vec<u8>)>, String> {
    let files: Vec<(PathBuf, Vec<u8>)> = vec![
        (
            PathBuf::from("日本語ファイル.txt"),
            "こんにちは、世界！日本語の内容です。".as_bytes().to_vec(),
        ),
        (PathBuf::from("empty.txt"), Vec::new()),
        (PathBuf::from("binary.dat"), make_binary_data(100 * 1024)),
        (
            PathBuf::from("nested/inner.txt"),
            b"nested file content".to_vec(),
        ),
        (
            PathBuf::from("nested/deep/leaf.bin"),
            make_binary_data(4096),
        ),
    ];

    for (rel, data) in &files {
        let full = root.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("ソースディレクトリ作成エラー: {}", e))?;
        }
        fs::write(&full, data).map_err(|e| format!("ソースファイル書き込みエラー: {}", e))?;
    }

    // 空ディレクトリ（明示的なディレクトリエントリの保存を確認するため）
    fs::create_dir_all(root.join("empty_dir"))
        .map_err(|e| format!("空ディレクトリ作成エラー: {}", e))?;

    Ok(files)
}

/// ディレクトリ配下の全ファイルの相対パスを収集
fn collect_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
        for dir_entry in fs::read_dir(dir).map_err(|e| format!("ディレクトリ走査エラー: {}", e))?
        {
            let dir_entry = dir_entry.map_err(|e| format!("エントリ読み込みエラー: {}", e))?;
            let path = dir_entry.path();
            if path.is_dir() {
                walk(&path, root, out)?;
            } else {
                let rel = path
                    .strip_prefix(root)
                    .map_err(|e| format!("相対パス変換エラー: {}", e))?
                    .to_path_buf();
                out.push(rel);
            }
        }
        Ok(())
    }

    let mut out = Vec::new();
    walk(root, root, &mut out)?;
    out.sort();
    Ok(out)
}

/// 作成 → 一覧 → 解凍 → バイト比較 のラウンドトリップ検証
///
/// LZH を含む全形式で日本語ファイル名も厳密に一致することを要求する
/// （LZH は自前ライター/リーダーが Shift_JIS で対称に符号化する）。
fn roundtrip(
    label: &str,
    archive_file_name: &str,
    archive_type: ArchiveType,
) -> Result<(), String> {
    let dir = TestDir::new(label)?;
    let src_root = dir.path().join("src_root");
    let files = build_source_tree(&src_root)?;

    // 作成
    let archive_path = dir.path().join(archive_file_name);
    ArchiveHandler::create_archive(std::slice::from_ref(&src_root), &archive_path, archive_type)?;
    let archive_len = fs::metadata(&archive_path)
        .map_err(|e| format!("アーカイブメタデータ取得エラー: {}", e))?
        .len();
    assert!(
        archive_len > 0,
        "作成されたアーカイブが空です: {}",
        archive_file_name
    );

    // 一覧（名前とサイズを検証）
    let entries = ArchiveHandler::list_archive_contents(&archive_path)?;
    for (rel, data) in &files {
        let expected_name = format!("src_root/{}", rel.to_string_lossy());
        let entry = entries
            .iter()
            .find(|e| e.name.trim_end_matches('/') == expected_name)
            .ok_or_else(|| format!("一覧にエントリがありません: {}", expected_name))?;
        assert!(
            !entry.is_dir,
            "ファイルがディレクトリ扱いです: {}",
            expected_name
        );
        assert_eq!(
            entry.size,
            data.len() as u64,
            "一覧のサイズが不正です: {}",
            expected_name
        );
    }
    let file_entry_count = entries.iter().filter(|e| !e.is_dir).count();
    assert_eq!(
        file_entry_count,
        files.len(),
        "一覧のファイルエントリ数が不正です"
    );
    for dir_name in [
        "src_root",
        "src_root/nested",
        "src_root/nested/deep",
        "src_root/empty_dir",
    ] {
        let entry = entries
            .iter()
            .find(|e| e.name.trim_end_matches('/') == dir_name)
            .ok_or_else(|| format!("一覧にディレクトリエントリがありません: {}", dir_name))?;
        assert!(entry.is_dir, "ディレクトリがファイル扱いです: {}", dir_name);
    }

    // 解凍
    let out_dir = dir.path().join("out");
    ArchiveHandler::extract_archive(&archive_path, &out_dir)?;

    // 全ファイルをバイト比較（日本語名も厳密一致）
    for (rel, data) in &files {
        let extracted = out_dir.join("src_root").join(rel);
        let got = fs::read(&extracted).map_err(|e| {
            format!(
                "解凍ファイル読み込みエラー ({}): {}",
                extracted.display(),
                e
            )
        })?;
        assert_eq!(
            got.len(),
            data.len(),
            "解凍後のサイズが不一致です: {}",
            rel.display()
        );
        assert!(got == *data, "解凍後の内容が不一致です: {}", rel.display());
    }

    // 余分なファイルが作成されていないこと
    let actual = collect_files(&out_dir)?;
    assert_eq!(
        actual.len(),
        files.len(),
        "解凍後のファイル集合が期待と異なります: {:?}",
        actual
    );

    // 空ディレクトリが保存されていること
    assert!(
        out_dir.join("src_root/empty_dir").is_dir(),
        "空ディレクトリが復元されていません"
    );

    Ok(())
}

#[test]
fn zip_roundtrip() -> Result<(), String> {
    roundtrip("zip", "test.zip", ArchiveType::Zip)
}

#[test]
fn tar_roundtrip() -> Result<(), String> {
    roundtrip("tar", "test.tar", ArchiveType::Tar)
}

#[test]
fn tar_gz_roundtrip() -> Result<(), String> {
    // 回帰: "*.tar.gz" が拡張子 "gz" 判定により単独 GZ として誤検出されないこと
    roundtrip("targz", "test.tar.gz", ArchiveType::TarGz)
}

#[test]
fn tar_bz2_roundtrip() -> Result<(), String> {
    roundtrip("tarbz2", "test.tar.bz2", ArchiveType::TarBz2)
}

#[test]
fn lzh_roundtrip() -> Result<(), String> {
    // oxiarc 0.3.4 はレベル 2 ヘッダーの Shift_JIS 名を対称に符号化するため
    // 日本語名も厳密一致（名前・サイズ・内容）でラウンドトリップする
    roundtrip("lzh", "test.lzh", ArchiveType::Lzh)
}

/// 回帰: LZH 作成が実際に圧縮する（lh5、Store フォールバックではない）こと
///
/// oxiarc 0.3.3 の lh5 エンコーダーは破損データを生成していたため
/// fvrs は一時的に全エントリを lh0（無圧縮）で格納していた。
/// 0.3.4 で修正されたため、圧縮可能な入力ではアーカイブが元データより
/// 小さくなることを検証し、あわせて内容のバイト一致も確認する。
#[test]
fn lzh_compresses_compressible_input() -> Result<(), String> {
    let dir = TestDir::new("lzh_compress")?;
    let src_root = dir.path().join("src_root");
    fs::create_dir_all(&src_root).map_err(|e| format!("ソースディレクトリ作成エラー: {}", e))?;

    // 高冗長な圧縮可能データ（約 128KB）
    let payload = "圧縮可能な繰り返しテキスト。compressible repeated text. "
        .repeat(1600)
        .into_bytes();
    fs::write(src_root.join("compressible.txt"), &payload)
        .map_err(|e| format!("ソースファイル書き込みエラー: {}", e))?;

    let archive_path = dir.path().join("compress.lzh");
    ArchiveHandler::create_archive(
        std::slice::from_ref(&src_root),
        &archive_path,
        ArchiveType::Lzh,
    )?;

    let archive_len = fs::metadata(&archive_path)
        .map_err(|e| format!("アーカイブメタデータ取得エラー: {}", e))?
        .len();
    assert!(
        archive_len < payload.len() as u64,
        "LZH が圧縮されていません (アーカイブ {} バイト >= 元データ {} バイト): \
         lh5 ではなく無圧縮格納にフォールバックしている可能性があります",
        archive_len,
        payload.len()
    );

    // 一覧のサイズが元サイズであること（lh5 でも size は解凍後サイズ）
    let entries = ArchiveHandler::list_archive_contents(&archive_path)?;
    let entry = entries
        .iter()
        .find(|e| e.name.trim_end_matches('/') == "src_root/compressible.txt")
        .ok_or("一覧に compressible.txt がありません")?;
    assert_eq!(entry.size, payload.len() as u64, "一覧のサイズが不正です");

    // 解凍して内容がバイト一致すること
    let out_dir = dir.path().join("out");
    ArchiveHandler::extract_archive(&archive_path, &out_dir)?;
    let got = fs::read(out_dir.join("src_root/compressible.txt"))
        .map_err(|e| format!("解凍ファイル読み込みエラー: {}", e))?;
    assert!(got == payload, "lh5 解凍後の内容が不一致です");

    Ok(())
}

#[test]
fn gz_list_and_extract_honors_header_filename() -> Result<(), String> {
    let dir = TestDir::new("gz_named")?;
    let original = "GZ 形式の内容テスト。Original content for gzip.".repeat(64);

    let gz_bytes =
        oxiarc_archive::gzip::compress_with_filename(original.as_bytes(), "original_name.txt", 6)
            .map_err(|e| format!("GZ フィクスチャ作成エラー: {}", e))?;
    let gz_path = dir.path().join("renamed_on_disk.gz");
    fs::write(&gz_path, &gz_bytes).map_err(|e| format!("GZ 書き込みエラー: {}", e))?;

    // 一覧: gzip ヘッダーの元ファイル名が優先されること
    let entries = ArchiveHandler::list_archive_contents(&gz_path)?;
    assert_eq!(entries.len(), 1, "GZ の一覧は 1 エントリのはずです");
    assert_eq!(entries[0].name, "original_name.txt");
    assert!(!entries[0].is_dir);

    // 解凍: ヘッダーの元ファイル名で出力されること
    let out_dir = dir.path().join("out");
    ArchiveHandler::extract_archive(&gz_path, &out_dir)?;
    let extracted = out_dir.join("original_name.txt");
    let got = fs::read(&extracted).map_err(|e| format!("解凍ファイル読み込みエラー: {}", e))?;
    assert!(got == original.as_bytes(), "GZ 解凍後の内容が不一致です");

    Ok(())
}

#[test]
fn gz_without_header_filename_uses_stem() -> Result<(), String> {
    let dir = TestDir::new("gz_stem")?;
    let original = b"gzip without embedded filename".to_vec();

    let gz_bytes = oxiarc_archive::gzip::compress(&original, 6)
        .map_err(|e| format!("GZ フィクスチャ作成エラー: {}", e))?;
    let gz_path = dir.path().join("notes.txt.gz");
    fs::write(&gz_path, &gz_bytes).map_err(|e| format!("GZ 書き込みエラー: {}", e))?;

    let out_dir = dir.path().join("out");
    ArchiveHandler::extract_archive(&gz_path, &out_dir)?;
    let extracted = out_dir.join("notes.txt");
    let got = fs::read(&extracted).map_err(|e| format!("解凍ファイル読み込みエラー: {}", e))?;
    assert!(got == original, "GZ 解凍後の内容が不一致です");

    Ok(())
}

/// 7z 形式の可変長数値エンコード（1〜2 バイト形式まで対応）
///
/// 2 バイト形式は「先頭バイトの下位 6 ビットが上位、追加バイトが下位 8 ビット」
/// というリトルエンディアン仕様（oxiarc 0.3.3 が誤読していた形式）。
fn encode_7z_number(value: usize) -> Result<Vec<u8>, String> {
    if value < 0x80 {
        Ok(vec![value as u8])
    } else if value < 0x4000 {
        Ok(vec![0x80 | (value >> 8) as u8, (value & 0xFF) as u8])
    } else {
        Err(format!(
            "テスト用 7z 数値エンコードは 0x4000 未満のみ対応: {}",
            value
        ))
    }
}

/// 最小構成の 7z アーカイブ（Copy コーデック・1 ファイル）をバイト列で構築
fn build_minimal_7z(entry_name: &str, content: &[u8]) -> Result<Vec<u8>, String> {
    use oxiarc_core::Crc32;

    // 非圧縮ヘッダー本体
    let mut header: Vec<u8> = vec![
        0x01, // kHeader
        0x04, // kMainStreamsInfo
        0x06, // kPackInfo
        0x00, // pack_pos = 0
        0x01, // パックストリーム数 = 1
        0x09, // kSize
    ];
    header.extend_from_slice(&encode_7z_number(content.len())?);
    header.push(0x00); // kEnd (PackInfo)

    // kUnpackInfo
    header.push(0x07);
    header.push(0x0B); // kFolder
    header.push(0x01); // フォルダ数 = 1
    header.push(0x00); // external = 0
    header.push(0x01); // コーダー数 = 1
    header.push(0x01); // メインバイト: コーデックID 1 バイト・単純コーダー
    header.push(0x00); // コーデックID = Copy
    header.push(0x0C); // kCodersUnpackSize
    header.extend_from_slice(&encode_7z_number(content.len())?);
    header.push(0x00); // kEnd (UnpackInfo)
    header.push(0x00); // kEnd (StreamsInfo)

    // kFilesInfo
    header.push(0x05);
    header.push(0x01); // ファイル数 = 1
    header.push(0x11); // kName
    let utf16: Vec<u8> = entry_name
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    header.extend_from_slice(&encode_7z_number(1 + utf16.len() + 2)?); // external + 名前 + NUL 終端
    header.push(0x00); // external = 0
    header.extend_from_slice(&utf16);
    header.extend_from_slice(&[0x00, 0x00]); // UTF-16 NUL 終端
    header.push(0x00); // kEnd (FilesInfo)
    header.push(0x00); // kEnd (Header)

    // 署名ヘッダー (32 バイト) + パックデータ + ヘッダー
    let mut tail = [0u8; 20];
    tail[0..8].copy_from_slice(&(content.len() as u64).to_le_bytes()); // 次ヘッダーオフセット
    tail[8..16].copy_from_slice(&(header.len() as u64).to_le_bytes()); // 次ヘッダーサイズ
    tail[16..20].copy_from_slice(&Crc32::compute(&header).to_le_bytes()); // 次ヘッダー CRC

    let mut archive = Vec::with_capacity(32 + content.len() + header.len());
    archive.extend_from_slice(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]); // マジック "7z"
    archive.extend_from_slice(&[0x00, 0x04]); // バージョン 0.4
    archive.extend_from_slice(&Crc32::compute(&tail).to_le_bytes()); // 開始ヘッダー CRC
    archive.extend_from_slice(&tail);
    archive.extend_from_slice(content);
    archive.extend_from_slice(&header);
    Ok(archive)
}

#[test]
fn sevenz_listing_does_not_pollute_temp_dir() -> Result<(), String> {
    let dir = TestDir::new("sevenz")?;

    // 一意なエントリ名を使い、旧実装（一覧時に temp_dir 直下へ解凍していた）の回帰を検出する
    let marker_name = format!("fvrs_7z_marker_{}.txt", std::process::id());
    let content = b"7z regression content";
    let archive_bytes = build_minimal_7z(&marker_name, content)?;
    let archive_path = dir.path().join("fixture.7z");
    fs::write(&archive_path, &archive_bytes).map_err(|e| format!("7Z 書き込みエラー: {}", e))?;

    let polluted_path = std::env::temp_dir().join(&marker_name);
    assert!(
        !polluted_path.exists(),
        "テスト前提エラー: マーカーファイルが既に存在します"
    );

    // 一覧: 実際のエントリが返り、一時ディレクトリに何も作成されないこと
    let entries = ArchiveHandler::list_archive_contents(&archive_path)?;
    assert_eq!(entries.len(), 1, "7Z の一覧は 1 エントリのはずです");
    assert_eq!(entries[0].name, marker_name);
    assert!(!entries[0].is_dir);
    assert!(
        !polluted_path.exists(),
        "7Z の一覧表示が一時ディレクトリへファイルを作成しました（旧実装への回帰）"
    );

    // 一覧のサイズが正しいこと（0 バイト表示の回帰確認）
    assert_eq!(
        entries[0].size,
        content.len() as u64,
        "7Z 一覧の解凍後サイズが不正です"
    );

    // 解凍: 出力は解凍先ディレクトリ内のみに作られ、内容がバイト一致すること
    // （oxiarc 0.3.3 の SevenZReader が全エントリを 0 バイトで抽出する
    //  致命的バグに対する回帰テスト）
    let out_dir = dir.path().join("out");
    ArchiveHandler::extract_archive(&archive_path, &out_dir)?;
    let extracted = fs::read(out_dir.join(&marker_name))
        .map_err(|e| format!("7Z 解凍ファイル読み込みエラー: {}", e))?;
    assert!(
        extracted == content,
        "7Z 解凍後の内容が不一致です (期待 {} バイト, 実際 {} バイト)",
        content.len(),
        extracted.len()
    );
    let extracted_files = collect_files(&out_dir)?;
    assert_eq!(
        extracted_files,
        vec![PathBuf::from(&marker_name)],
        "7Z 解凍で余分なファイルが作成されました"
    );
    assert!(
        !polluted_path.exists(),
        "7Z の解凍が一時ディレクトリ直下へファイルを作成しました"
    );

    Ok(())
}

/// 複数サブストリーム・空ファイル・ディレクトリを含む 7z を構築
///
/// 構成: 1 フォルダー (Copy) に 2 ファイル分のサブストリーム、
/// 空ストリームのディレクトリ "dir" と空ファイル "empty.txt"。
fn build_multi_entry_7z(content_a: &[u8], content_b: &[u8]) -> Result<Vec<u8>, String> {
    use oxiarc_core::Crc32;

    let packed: Vec<u8> = content_a.iter().chain(content_b.iter()).copied().collect();

    let mut header: Vec<u8> = vec![
        0x01, // kHeader
        0x04, // kMainStreamsInfo
        0x06, // kPackInfo
        0x00, // pack_pos = 0
        0x01, // パックストリーム数 = 1
        0x09, // kSize
    ];
    header.extend_from_slice(&encode_7z_number(packed.len())?);
    header.push(0x00); // kEnd

    // kUnpackInfo
    header.push(0x07);
    header.push(0x0B); // kFolder
    header.push(0x01); // フォルダー数 = 1
    header.push(0x00); // external = 0
    header.push(0x01); // コーダー数 = 1
    header.push(0x01); // メインバイト: ID 1 バイト・単純コーダー
    header.push(0x00); // コーデック ID = Copy
    header.push(0x0C); // kCodersUnpackSize
    header.extend_from_slice(&encode_7z_number(packed.len())?);
    header.push(0x00); // kEnd

    // kSubStreamsInfo: 1 フォルダーに 2 サブストリーム、先頭サイズのみ明示
    header.push(0x08);
    header.push(0x0D); // kNumUnpackStream
    header.push(0x02);
    header.push(0x09); // kSize
    header.extend_from_slice(&encode_7z_number(content_a.len())?);
    header.push(0x00); // kEnd (SubStreamsInfo)
    header.push(0x00); // kEnd (StreamsInfo)

    // kFilesInfo: [dir(空ストリーム), a.txt, empty.txt(空ストリーム+空ファイル), b.txt]
    header.push(0x05);
    header.push(0x04); // ファイル数 = 4

    header.push(0x0E); // kEmptyStream
    header.push(0x01); // サイズ = 1
    header.push(0b1010_0000); // dir と empty.txt が空ストリーム

    header.push(0x0F); // kEmptyFile
    header.push(0x01); // サイズ = 1
    header.push(0b0100_0000); // 空ストリーム 2 件中 2 件目 (empty.txt) のみ空ファイル

    // kName
    let names = ["dir", "a.txt", "empty.txt", "b.txt"];
    let mut name_block: Vec<u8> = vec![0x00]; // external = 0
    for name in names {
        for unit in name.encode_utf16() {
            name_block.extend_from_slice(&unit.to_le_bytes());
        }
        name_block.extend_from_slice(&[0x00, 0x00]);
    }
    header.push(0x11);
    header.extend_from_slice(&encode_7z_number(name_block.len())?);
    header.extend_from_slice(&name_block);

    header.push(0x00); // kEnd (FilesInfo)
    header.push(0x00); // kEnd (Header)

    // 署名ヘッダー
    let mut tail = [0u8; 20];
    tail[0..8].copy_from_slice(&(packed.len() as u64).to_le_bytes());
    tail[8..16].copy_from_slice(&(header.len() as u64).to_le_bytes());
    tail[16..20].copy_from_slice(&Crc32::compute(&header).to_le_bytes());

    let mut archive = Vec::with_capacity(32 + packed.len() + header.len());
    archive.extend_from_slice(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]);
    archive.extend_from_slice(&[0x00, 0x04]);
    archive.extend_from_slice(&Crc32::compute(&tail).to_le_bytes());
    archive.extend_from_slice(&tail);
    archive.extend_from_slice(&packed);
    archive.extend_from_slice(&header);
    Ok(archive)
}

/// 回帰: 7z のサブストリームサイズ解析・空ファイル・ディレクトリの取り扱い
///
/// oxiarc 0.3.3 はサブストリームサイズを読み捨てて全エントリを 0 バイトで
/// 抽出し、さらに空ファイル（ストリーム無し）を含むアーカイブの解凍全体を
/// エラーで中断していた。
#[test]
fn sevenz_substreams_empty_file_and_dir_extract_correctly() -> Result<(), String> {
    let dir = TestDir::new("sevenz_multi")?;

    let content_a = b"7z substream content A (first)".to_vec();
    let content_b = b"7z substream content B -- second file".to_vec();
    let archive_bytes = build_multi_entry_7z(&content_a, &content_b)?;
    let archive_path = dir.path().join("multi.7z");
    fs::write(&archive_path, &archive_bytes).map_err(|e| format!("7Z 書き込みエラー: {}", e))?;

    // 一覧: 4 エントリ、サイズと種別が正しいこと
    let entries = ArchiveHandler::list_archive_contents(&archive_path)?;
    assert_eq!(entries.len(), 4, "7Z の一覧は 4 エントリのはずです");
    let find = |name: &str| {
        entries
            .iter()
            .find(|e| e.name == name)
            .ok_or_else(|| format!("一覧にエントリがありません: {}", name))
    };
    assert!(find("dir")?.is_dir, "dir がディレクトリ扱いではありません");
    assert_eq!(find("a.txt")?.size, content_a.len() as u64);
    assert_eq!(find("empty.txt")?.size, 0);
    assert!(
        !find("empty.txt")?.is_dir,
        "空ファイルがディレクトリ扱いです"
    );
    assert_eq!(find("b.txt")?.size, content_b.len() as u64);

    // 解凍: 全エントリが正しい内容で解凍されること（空ファイル入りでも中断しない）
    let out_dir = dir.path().join("out");
    ArchiveHandler::extract_archive(&archive_path, &out_dir)?;

    assert!(
        out_dir.join("dir").is_dir(),
        "ディレクトリが復元されていません"
    );
    let got_a =
        fs::read(out_dir.join("a.txt")).map_err(|e| format!("a.txt 読み込みエラー: {}", e))?;
    assert!(got_a == content_a, "a.txt の内容が不一致です");
    let got_b =
        fs::read(out_dir.join("b.txt")).map_err(|e| format!("b.txt 読み込みエラー: {}", e))?;
    assert!(got_b == content_b, "b.txt の内容が不一致です");
    let got_empty = fs::read(out_dir.join("empty.txt"))
        .map_err(|e| format!("empty.txt 読み込みエラー: {}", e))?;
    assert!(got_empty.is_empty(), "空ファイルが空ではありません");

    Ok(())
}

#[test]
fn zip_slip_entries_are_sanitized() -> Result<(), String> {
    use oxiarc_archive::ZipWriter;

    let dir = TestDir::new("zipslip")?;
    let outer = dir.path().join("outer");
    let extract_root = outer.join("extract");
    fs::create_dir_all(&outer).map_err(|e| format!("ディレクトリ作成エラー: {}", e))?;

    // 絶対パスエントリの標的（テスト用サンドボックス内のみを指す）
    let abs_target = dir.path().join("abs_target").join("abs_evil.txt");
    let abs_name = abs_target.to_string_lossy().to_string();

    // 悪意ある ZIP を構築（`../` エントリと絶対パスエントリ）
    let zip_path = dir.path().join("malicious.zip");
    {
        let file = fs::File::create(&zip_path).map_err(|e| format!("ZIP 作成エラー: {}", e))?;
        let mut zip = ZipWriter::new(file);
        zip.add_file("../evil.txt", b"evil relative")
            .map_err(|e| format!("ZIP エントリ追加エラー: {}", e))?;
        zip.add_file(&abs_name, b"evil absolute")
            .map_err(|e| format!("ZIP エントリ追加エラー: {}", e))?;
        zip.add_file("good.txt", b"good content")
            .map_err(|e| format!("ZIP エントリ追加エラー: {}", e))?;
        zip.finish().map_err(|e| format!("ZIP 完了エラー: {}", e))?;
    }

    ArchiveHandler::extract_archive(&zip_path, &extract_root)?;

    // 解凍先の外（`../` の行き先）に何も書かれていないこと
    assert!(
        !outer.join("evil.txt").exists(),
        "Zip Slip: `../` エントリが解凍先の外に書き込まれました"
    );
    // 絶対パスの標的に何も書かれていないこと
    assert!(
        !abs_target.exists(),
        "Zip Slip: 絶対パスエントリがそのままのパスへ書き込まれました"
    );
    // outer 直下は extract のみであること
    let outer_entries: Vec<String> = fs::read_dir(&outer)
        .map_err(|e| format!("ディレクトリ走査エラー: {}", e))?
        .filter_map(|dir_entry| dir_entry.ok())
        .map(|dir_entry| dir_entry.file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(
        outer_entries,
        vec!["extract".to_string()],
        "解凍先の外にファイルが作成されました"
    );

    // 無害化されたエントリは解凍先の中に配置されること
    let sanitized_rel = fs::read(extract_root.join("evil.txt"))
        .map_err(|e| format!("無害化エントリ読み込みエラー: {}", e))?;
    assert!(
        sanitized_rel == b"evil relative",
        "無害化された `../` エントリの内容が不一致です"
    );

    // 絶対パスエントリはルート成分を除去して解凍先の中に配置されること
    let mut sanitized_abs = extract_root.clone();
    for component in Path::new(&abs_name).components() {
        if let std::path::Component::Normal(part) = component {
            sanitized_abs.push(part);
        }
    }
    let sanitized_abs_data = fs::read(&sanitized_abs)
        .map_err(|e| format!("無害化された絶対パスエントリ読み込みエラー: {}", e))?;
    assert!(
        sanitized_abs_data == b"evil absolute",
        "無害化された絶対パスエントリの内容が不一致です"
    );

    let good = fs::read(extract_root.join("good.txt"))
        .map_err(|e| format!("正常エントリ読み込みエラー: {}", e))?;
    assert!(good == b"good content", "正常エントリの内容が不一致です");

    Ok(())
}

/// liblzma (Python lzma, FORMAT_RAW / FILTER_LZMA1, lc=3 lp=0 pb=2, dict 64KB) が
/// 生成した実 LZMA1 ストリーム。内容は `lzma_fixture_content()` の 252 バイト。
const LZMA1_FIXTURE_PACKED: &[u8] = &[
    0x00, 0x26, 0x16, 0x85, 0xBC, 0x45, 0xF0, 0xEA, 0x70, 0xEC, 0x7A, 0x6E, //
    0x8F, 0xA4, 0x73, 0xFA, 0x7D, 0x40, 0x75, 0xD2, 0x5A, 0x4D, 0x7A, 0x23, //
    0x64, 0xBA, 0x67, 0x69, 0x63, 0x81, 0x42, 0x98, 0xDE, 0x62, 0x4E, 0x71, //
    0x75, 0x0E, 0x64, 0xB8, 0x31, 0x66, 0x6E, 0xD7, 0x89, 0x2C, 0x0C, 0x32, //
    0x4D, 0xA7, 0xD3, 0xE9, 0xDB, 0xF5, 0xDB, 0x28, 0x5B, 0x67, 0xB5, 0x57, //
    0x19, 0xA2, 0x15, 0x9F, 0xB1, 0x61, 0xFF, 0xF8, 0x7D, 0xEE, 0x00,
];

/// LZMA1 フィクスチャの復号後の内容
fn lzma_fixture_content() -> Vec<u8> {
    "LZMA fixture: 実データで検証する 7z コンテンツ。"
        .repeat(4)
        .into_bytes()
}

/// LZMA コーデックの 7z アーカイブ（1 フォルダー・1 ファイル）をバイト列で構築
///
/// 圧縮データには liblzma が生成した実ストリームを使用する
/// （oxiarc-lzma 0.3.3 のエンコーダーは仕様と異なる確率テーブル配置の
///  非標準ストリームを生成するため、実データでの検証にならない）。
fn build_lzma_7z(entry_name: &str, content: &[u8], packed: &[u8]) -> Result<Vec<u8>, String> {
    use oxiarc_core::Crc32;

    let dict_size: u32 = 1 << 16;
    let props_byte: u8 = 0x5D; // lc=3 lp=0 pb=2

    let mut header: Vec<u8> = vec![
        0x01, // kHeader
        0x04, // kMainStreamsInfo
        0x06, // kPackInfo
        0x00, // pack_pos = 0
        0x01, // パックストリーム数 = 1
        0x09, // kSize
    ];
    header.extend_from_slice(&encode_7z_number(packed.len())?);
    header.push(0x00); // kEnd

    // kUnpackInfo (LZMA コーダー: ID = 03 01 01, プロパティ 5 バイト)
    header.push(0x07);
    header.push(0x0B); // kFolder
    header.push(0x01); // フォルダー数 = 1
    header.push(0x00); // external = 0
    header.push(0x01); // コーダー数 = 1
    header.push(0x23); // メインバイト: ID 3 バイト + 属性あり
    header.extend_from_slice(&[0x03, 0x01, 0x01]); // LZMA
    header.push(0x05); // プロパティサイズ
    header.push(props_byte);
    header.extend_from_slice(&dict_size.to_le_bytes());
    header.push(0x0C); // kCodersUnpackSize
    header.extend_from_slice(&encode_7z_number(content.len())?);
    header.push(0x0A); // kCRC (フォルダー CRC で復号結果を検証させる)
    header.push(0x01); // 全定義
    header.extend_from_slice(&Crc32::compute(content).to_le_bytes());
    header.push(0x00); // kEnd
    header.push(0x00); // kEnd (StreamsInfo)

    // kFilesInfo
    header.push(0x05);
    header.push(0x01); // ファイル数 = 1
    header.push(0x11); // kName
    let utf16: Vec<u8> = entry_name
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    header.extend_from_slice(&encode_7z_number(1 + utf16.len() + 2)?);
    header.push(0x00); // external = 0
    header.extend_from_slice(&utf16);
    header.extend_from_slice(&[0x00, 0x00]);
    header.push(0x00); // kEnd (FilesInfo)
    header.push(0x00); // kEnd (Header)

    let mut tail = [0u8; 20];
    tail[0..8].copy_from_slice(&(packed.len() as u64).to_le_bytes());
    tail[8..16].copy_from_slice(&(header.len() as u64).to_le_bytes());
    tail[16..20].copy_from_slice(&Crc32::compute(&header).to_le_bytes());

    let mut archive = Vec::with_capacity(32 + packed.len() + header.len());
    archive.extend_from_slice(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]);
    archive.extend_from_slice(&[0x00, 0x04]);
    archive.extend_from_slice(&Crc32::compute(&tail).to_le_bytes());
    archive.extend_from_slice(&tail);
    archive.extend_from_slice(packed);
    archive.extend_from_slice(&header);
    Ok(archive)
}

/// 回帰: 実 liblzma 生成の LZMA コーデック 7z が正しい内容で解凍されること
/// （フォルダー CRC 検証込み。oxiarc-lzma 0.3.3 は実 LZMA ストリームを
///  復号できないため自前デコーダーで処理される）
#[test]
fn sevenz_lzma_folder_extracts_correct_content() -> Result<(), String> {
    let dir = TestDir::new("sevenz_lzma")?;

    let content = lzma_fixture_content();
    let archive_bytes = build_lzma_7z("lzma_file.txt", &content, LZMA1_FIXTURE_PACKED)?;
    let archive_path = dir.path().join("lzma.7z");
    fs::write(&archive_path, &archive_bytes).map_err(|e| format!("7Z 書き込みエラー: {}", e))?;

    let entries = ArchiveHandler::list_archive_contents(&archive_path)?;
    assert_eq!(entries.len(), 1, "7Z (LZMA) の一覧は 1 エントリのはずです");
    assert_eq!(entries[0].name, "lzma_file.txt");
    assert_eq!(entries[0].size, content.len() as u64);

    let out_dir = dir.path().join("out");
    ArchiveHandler::extract_archive(&archive_path, &out_dir)?;
    let got = fs::read(out_dir.join("lzma_file.txt"))
        .map_err(|e| format!("7Z (LZMA) 解凍ファイル読み込みエラー: {}", e))?;
    assert!(
        got == content,
        "7Z (LZMA) 解凍後の内容が不一致です (期待 {} バイト, 実際 {} バイト)",
        content.len(),
        got.len()
    );

    Ok(())
}

/// 無圧縮 (Stored) エントリのみの ZIP をバイト列で構築（名前は生バイト列で指定）
fn build_stored_zip(entries: &[(&[u8], &[u8])]) -> Vec<u8> {
    use oxiarc_core::Crc32;

    const DOS_DATE_1980_01_01: u16 = 0x0021;

    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();

    for (name, data) in entries {
        let offset = out.len() as u32;
        let crc = Crc32::compute(data);

        // ローカルファイルヘッダー
        out.extend_from_slice(&[0x50, 0x4B, 0x03, 0x04]);
        out.extend_from_slice(&20u16.to_le_bytes()); // 必要バージョン
        out.extend_from_slice(&0u16.to_le_bytes()); // 汎用フラグ (EFS 無し)
        out.extend_from_slice(&0u16.to_le_bytes()); // 無圧縮
        out.extend_from_slice(&0u16.to_le_bytes()); // 時刻
        out.extend_from_slice(&DOS_DATE_1980_01_01.to_le_bytes()); // 日付
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes()); // 圧縮後
        out.extend_from_slice(&(data.len() as u32).to_le_bytes()); // 圧縮前
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // 拡張フィールド長
        out.extend_from_slice(name);
        out.extend_from_slice(data);

        // セントラルディレクトリエントリ
        central.extend_from_slice(&[0x50, 0x4B, 0x01, 0x02]);
        central.extend_from_slice(&20u16.to_le_bytes()); // 作成バージョン
        central.extend_from_slice(&20u16.to_le_bytes()); // 必要バージョン
        central.extend_from_slice(&0u16.to_le_bytes()); // 汎用フラグ (EFS 無し)
        central.extend_from_slice(&0u16.to_le_bytes()); // 無圧縮
        central.extend_from_slice(&0u16.to_le_bytes()); // 時刻
        central.extend_from_slice(&DOS_DATE_1980_01_01.to_le_bytes()); // 日付
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes()); // 拡張フィールド長
        central.extend_from_slice(&0u16.to_le_bytes()); // コメント長
        central.extend_from_slice(&0u16.to_le_bytes()); // ディスク番号
        central.extend_from_slice(&0u16.to_le_bytes()); // 内部属性
        central.extend_from_slice(&0u32.to_le_bytes()); // 外部属性
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name);
    }

    let cd_offset = out.len() as u32;
    let cd_size = central.len() as u32;
    out.extend_from_slice(&central);

    // EOCD
    out.extend_from_slice(&[0x50, 0x4B, 0x05, 0x06]);
    out.extend_from_slice(&0u16.to_le_bytes()); // ディスク番号
    out.extend_from_slice(&0u16.to_le_bytes()); // CD 開始ディスク
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // コメント長

    out
}

/// 回帰: Shift_JIS 名（EFS フラグ無し）の ZIP で異なる名前が衝突しないこと
///
/// oxiarc 0.3.3 は生名を lossy 復号するため「あ.txt」「い.txt」がともに
/// "\u{FFFD}\u{FFFD}.txt" となり、解凍で片方が上書き消失していた。
#[test]
fn zip_shift_jis_names_extract_as_distinct_files() -> Result<(), String> {
    let dir = TestDir::new("zip_sjis")?;

    // "あ.txt" (82 A0) と "い.txt" (82 A2) の Shift_JIS 表現
    let name_a: &[u8] = &[0x82, 0xA0, b'.', b't', b'x', b't'];
    let name_i: &[u8] = &[0x82, 0xA2, b'.', b't', b'x', b't'];
    let zip_bytes = build_stored_zip(&[(name_a, b"content A"), (name_i, b"content B")]);
    let zip_path = dir.path().join("sjis.zip");
    fs::write(&zip_path, &zip_bytes).map_err(|e| format!("ZIP 書き込みエラー: {}", e))?;

    // 一覧: 両方の名前が正しく復号され、別名として区別されること
    let entries = ArchiveHandler::list_archive_contents(&zip_path)?;
    assert_eq!(entries.len(), 2, "ZIP の一覧は 2 エントリのはずです");
    assert!(
        entries.iter().any(|e| e.name == "あ.txt"),
        "一覧に「あ.txt」がありません: {:?}",
        entries.iter().map(|e| e.name.clone()).collect::<Vec<_>>()
    );
    assert!(
        entries.iter().any(|e| e.name == "い.txt"),
        "一覧に「い.txt」がありません"
    );

    // 解凍: 2 ファイルとも失われずに解凍されること
    let out_dir = dir.path().join("out");
    ArchiveHandler::extract_archive(&zip_path, &out_dir)?;
    let extracted = collect_files(&out_dir)?;
    assert_eq!(
        extracted.len(),
        2,
        "Shift_JIS 名の衝突によりファイルが失われました: {:?}",
        extracted
    );
    let got_a =
        fs::read(out_dir.join("あ.txt")).map_err(|e| format!("あ.txt 読み込みエラー: {}", e))?;
    assert!(got_a == b"content A", "あ.txt の内容が不一致です");
    let got_i =
        fs::read(out_dir.join("い.txt")).map_err(|e| format!("い.txt 読み込みエラー: {}", e))?;
    assert!(got_i == b"content B", "い.txt の内容が不一致です");

    Ok(())
}

/// 回帰: 100 バイト超の日本語ファイル名を含む TAR / TAR.GZ の作成
///
/// oxiarc 0.3.3 の `TarHeader::to_block` は `name[..155]` のスライスで
/// 文字境界を無視してパニックしていた（漢字 60 文字のファイル名で再現）。
/// PAX 拡張ヘッダー経由で完全な名前がラウンドトリップすることも検証する。
#[test]
fn tar_long_japanese_names_do_not_panic_and_roundtrip() -> Result<(), String> {
    for (label, file_name, archive_type) in [
        ("tar_long", "long.tar", ArchiveType::Tar),
        ("targz_long", "long.tar.gz", ArchiveType::TarGz),
    ] {
        let dir = TestDir::new(label)?;
        let src_root = dir.path().join("src_root");

        // 180 バイトの名前（バイト 155 が多バイト文字の途中に落ちる）
        let long_name = format!("{}.txt", "日".repeat(60));
        // ディレクトリ名も 100 バイト超
        let long_dir = "あ".repeat(40);
        let nested_name = format!("{}.txt", "い".repeat(52));

        let content_a = "長い名前のファイルの内容".repeat(32).into_bytes();
        let content_b = b"nested long name content".to_vec();

        fs::create_dir_all(src_root.join(&long_dir))
            .map_err(|e| format!("ソースディレクトリ作成エラー: {}", e))?;
        fs::write(src_root.join(&long_name), &content_a)
            .map_err(|e| format!("ソースファイル書き込みエラー: {}", e))?;
        fs::write(src_root.join(&long_dir).join(&nested_name), &content_b)
            .map_err(|e| format!("ソースファイル書き込みエラー: {}", e))?;

        // 作成（旧実装はここでパニックしていた）
        let archive_path = dir.path().join(file_name);
        ArchiveHandler::create_archive(
            std::slice::from_ref(&src_root),
            &archive_path,
            archive_type,
        )?;

        // 一覧: PAX 経由で完全な名前が見えること
        let entries = ArchiveHandler::list_archive_contents(&archive_path)?;
        let expected_a = format!("src_root/{}", long_name);
        let expected_b = format!("src_root/{}/{}", long_dir, nested_name);
        assert!(
            entries.iter().any(|e| e.name == expected_a),
            "一覧に長い名前のエントリがありません ({}): {:?}",
            label,
            entries.iter().map(|e| e.name.clone()).collect::<Vec<_>>()
        );
        assert!(
            entries.iter().any(|e| e.name == expected_b),
            "一覧にネストした長い名前のエントリがありません ({})",
            label
        );

        // 解凍して完全な名前と内容を検証
        let out_dir = dir.path().join("out");
        ArchiveHandler::extract_archive(&archive_path, &out_dir)?;
        let got_a = fs::read(out_dir.join("src_root").join(&long_name))
            .map_err(|e| format!("解凍ファイル読み込みエラー ({}): {}", label, e))?;
        assert!(
            got_a == content_a,
            "長い名前のファイル内容が不一致です ({})",
            label
        );
        let got_b = fs::read(out_dir.join("src_root").join(&long_dir).join(&nested_name))
            .map_err(|e| format!("解凍ファイル読み込みエラー ({}): {}", label, e))?;
        assert!(
            got_b == content_b,
            "ネストした長い名前のファイル内容が不一致です ({})",
            label
        );
    }

    Ok(())
}

/// レベル 0 ヘッダーの LZH エントリをバイト列で構築
fn build_lzh_level0_entry(
    method: &[u8; 5],
    name_sjis: &[u8],
    original: &[u8],
    stored: &[u8],
) -> Vec<u8> {
    use oxiarc_core::Crc16;

    let header_size = 22 + name_sjis.len(); // method..crc16 (先頭 2 バイトを除く)
    let mut entry = Vec::with_capacity(2 + header_size + stored.len());
    entry.push(header_size as u8);
    entry.push(0); // チェックサム (後で計算)
    entry.extend_from_slice(method);
    entry.extend_from_slice(&(stored.len() as u32).to_le_bytes()); // 圧縮後サイズ
    entry.extend_from_slice(&(original.len() as u32).to_le_bytes()); // 元サイズ
    entry.extend_from_slice(&[0x00, 0x00, 0x21, 0x00]); // DOS 日時 (1980-01-01)
    entry.push(0x20); // 属性
    entry.push(0x00); // ヘッダーレベル 0
    entry.push(name_sjis.len() as u8);
    entry.extend_from_slice(name_sjis);
    entry.extend_from_slice(&Crc16::compute(original).to_le_bytes());

    // レベル 0 のチェックサムは先頭 2 バイトを除くヘッダーの単純加算
    let checksum: u8 = entry[2..].iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    entry[1] = checksum;

    entry.extend_from_slice(stored);
    entry
}

/// 回帰: `-lhd-`（ディレクトリ）と `-lh1-` を含む LZH の一覧・解凍
///
/// oxiarc 0.3.3 の `LzhReader` は未知の圧縮方式 ID をヘッダー解析の
/// ハードエラーにするため、`-lhd-` を 1 つでも含む実在の日本語 LZH
/// アーカイブ（LHA / Lhaplus はディレクトリを `-lhd-` で格納する）の
/// 一覧・解凍が丸ごと失敗していた。未対応方式のエントリは一覧に出しつつ
/// 解凍時のみスキップする（旧 delharc 実装と同じ動作）ことも検証する。
#[test]
fn lzh_with_lhd_and_lh1_entries_lists_and_extracts() -> Result<(), String> {
    use oxiarc_lzhuf::lh1::encode_lh1_literals;

    let dir = TestDir::new("lzh_lhd_lh1")?;

    // "サブ" (0x83 0x54 0x83 0x75) — Shift_JIS のディレクトリ名
    let dir_name_sjis: &[u8] = &[0x83, 0x54, 0x83, 0x75];
    let lh1_content = "lh1 で圧縮された日本語コンテンツ。LZHUF adaptive Huffman.".repeat(8);
    let lh1_encoded = encode_lh1_literals(lh1_content.as_bytes());
    let lh0_content = b"plain stored file (lh0)".to_vec();

    let mut archive: Vec<u8> = Vec::new();
    // ディレクトリエントリ (-lhd-)
    archive.extend_from_slice(&build_lzh_level0_entry(b"-lhd-", dir_name_sjis, &[], &[]));
    // -lh1- 圧縮ファイル: "サブ\hello.txt"
    let mut lh1_name = dir_name_sjis.to_vec();
    lh1_name.push(b'\\');
    lh1_name.extend_from_slice(b"hello.txt");
    archive.extend_from_slice(&build_lzh_level0_entry(
        b"-lh1-",
        &lh1_name,
        lh1_content.as_bytes(),
        &lh1_encoded,
    ));
    // -lh0- 無圧縮ファイル
    archive.extend_from_slice(&build_lzh_level0_entry(
        b"-lh0-",
        b"plain.txt",
        &lh0_content,
        &lh0_content,
    ));
    // 未対応方式 (-lh2-) のエントリ（一覧には出るが解凍はスキップされること）
    archive.extend_from_slice(&build_lzh_level0_entry(
        b"-lh2-",
        b"unsupported.bin",
        b"xxxx",
        b"\x00\x01\x02\x03",
    ));
    archive.push(0); // 終端マーカー

    let lzh_path = dir.path().join("fixture.lzh");
    fs::write(&lzh_path, &archive).map_err(|e| format!("LZH 書き込みエラー: {}", e))?;

    // 一覧: 全エントリ（未対応方式を含む）が列挙されること
    let entries = ArchiveHandler::list_archive_contents(&lzh_path)?;
    assert_eq!(entries.len(), 4, "LZH の一覧は 4 エントリのはずです");
    // oxiarc 0.3.4 はディレクトリエントリ名を末尾スラッシュ付きに正規化する
    let dir_entry = entries
        .iter()
        .find(|e| e.name.trim_end_matches('/') == "サブ")
        .ok_or("一覧に -lhd- ディレクトリエントリがありません")?;
    assert!(dir_entry.is_dir, "-lhd- がディレクトリ扱いではありません");
    let lh1_entry = entries
        .iter()
        .find(|e| e.name == "サブ/hello.txt")
        .ok_or("一覧に -lh1- エントリがありません")?;
    assert_eq!(
        lh1_entry.size,
        lh1_content.len() as u64,
        "-lh1- エントリのサイズが不正です"
    );
    assert!(
        entries.iter().any(|e| e.name == "unsupported.bin"),
        "一覧に未対応方式のエントリがありません"
    );

    // 解凍: lh1 / lh0 が正しく解凍され、未対応方式のみスキップされること
    let out_dir = dir.path().join("out");
    ArchiveHandler::extract_archive(&lzh_path, &out_dir)?;

    assert!(
        out_dir.join("サブ").is_dir(),
        "-lhd- ディレクトリが復元されていません"
    );
    let got_lh1 = fs::read(out_dir.join("サブ/hello.txt"))
        .map_err(|e| format!("lh1 解凍ファイル読み込みエラー: {}", e))?;
    assert!(
        got_lh1 == lh1_content.as_bytes(),
        "-lh1- の解凍内容が不一致です (期待 {} バイト, 実際 {} バイト)",
        lh1_content.len(),
        got_lh1.len()
    );
    let got_lh0 = fs::read(out_dir.join("plain.txt"))
        .map_err(|e| format!("lh0 解凍ファイル読み込みエラー: {}", e))?;
    assert!(got_lh0 == lh0_content, "-lh0- の解凍内容が不一致です");
    assert!(
        !out_dir.join("unsupported.bin").exists(),
        "未対応方式のエントリが解凍されてしまいました"
    );

    Ok(())
}

#[cfg(not(feature = "rar"))]
#[test]
fn rar_disabled_returns_graceful_error() -> Result<(), String> {
    let dir = TestDir::new("rar")?;
    let rar_path = dir.path().join("sample.rar");
    fs::write(&rar_path, b"Rar!\x1a\x07\x01\x00dummy")
        .map_err(|e| format!("RAR ダミー書き込みエラー: {}", e))?;

    // 拡張子判定ではアーカイブとして認識されること（ユーザーへエラーを提示するため）
    assert!(ArchiveHandler::is_archive(&rar_path));

    // 一覧・解凍ともにパニックせず、明確なエラーを返すこと
    let list_result = ArchiveHandler::list_archive_contents(&rar_path);
    let list_err = list_result
        .err()
        .ok_or("RAR の一覧がエラーになりませんでした")?;
    assert!(
        list_err.contains("RAR"),
        "RAR 一覧エラーの内容が不明瞭です: {}",
        list_err
    );

    let out_dir = dir.path().join("out");
    let extract_result = ArchiveHandler::extract_archive(&rar_path, &out_dir);
    let extract_err = extract_result
        .err()
        .ok_or("RAR の解凍がエラーになりませんでした")?;
    assert!(
        extract_err.contains("RAR"),
        "RAR 解凍エラーの内容が不明瞭です: {}",
        extract_err
    );

    Ok(())
}
