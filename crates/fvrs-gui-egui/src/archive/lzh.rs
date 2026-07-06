//! FVRS 独自の LZH アーカイブリーダー / ライター
//!
//! oxiarc-archive 0.3.3 の `LzhReader` は `-lhd-`（ディレクトリ）や `-lh1-` を
//! 含むアーカイブ全体をハードエラーにしてしまい、実在する日本語 LZH
//! アーカイブ（LHA / Lhaplus はディレクトリを `-lhd-` で格納する）の一覧・解凍が
//! 丸ごと失敗する。また `LzhWriter` はファイル名を UTF-8 生バイトで書き込むため
//! Shift_JIS を前提とする LZH の慣習と非互換になる。
//!
//! 本モジュールはヘッダー解析（レベル 0〜3）とレベル 2 ヘッダーの書き込みを
//! 自前で行い、未対応圧縮方式のエントリは「一覧には出すが解凍時のみスキップ」
//! という旧 delharc 実装と同じ寛容な動作を提供する。
//! 復号は lh0/lz4/pm0（無圧縮）と lh4〜lh7（oxiarc-lzhuf）、lh1（自前実装）に対応。

use std::io::{Read, Seek, SeekFrom, Write};

use oxiarc_core::Crc16;
use oxiarc_lzhuf::LzhMethod;

use super::{lzhuf1, name_codec};

/// LZH エントリ情報
pub(super) struct LzhEntry {
    pub name: String,
    pub is_dir: bool,
    /// 解凍後サイズ
    pub size: u64,
    /// 圧縮データのサイズ（拡張ヘッダーを除いた実データ長）
    pub compressed_size: u64,
    /// 圧縮方式 ID（例: `-lh5-`）
    pub method: [u8; 5],
    pub crc16: u16,
    /// アーカイブ内の圧縮データ先頭オフセット
    pub data_offset: u64,
    /// 更新時刻（UNIX エポック秒）
    pub mtime_unix: Option<i64>,
}

/// エントリデータの読み出し結果
pub(super) enum LzhData {
    /// 復号済みデータ
    Ok(Vec<u8>),
    /// 未対応の圧縮方式（呼び出し側でスキップする）
    Unsupported(String),
}

/// MS-DOS 形式の日時を UNIX エポック秒へ変換
fn dos_datetime_to_unix(time: u16, date: u16) -> Option<i64> {
    let year = i32::from((date >> 9) & 0x7F) + 1980;
    let month = u32::from((date >> 5) & 0x0F);
    let day = u32::from(date & 0x1F);
    let hour = u32::from((time >> 11) & 0x1F);
    let minute = u32::from((time >> 5) & 0x3F);
    let second = u32::from(time & 0x1F) * 2;
    let naive = chrono::NaiveDate::from_ymd_opt(year, month, day)?
        .and_hms_opt(hour, minute, second)?;
    Some(naive.and_utc().timestamp())
}

/// ディレクトリ名拡張ヘッダー (0x02) を復号（コンポーネント区切りは 0xFF）
fn decode_dirname(bytes: &[u8]) -> String {
    bytes
        .split(|&b| b == 0xFF)
        .filter(|component| !component.is_empty())
        .map(name_codec::decode_lzh_name)
        .collect::<Vec<_>>()
        .join("/")
}

/// 拡張ヘッダー解析で収集するメタデータ
#[derive(Default)]
struct ExtHeaderData {
    filename: Option<String>,
    dirname: Option<String>,
    unix_mtime: Option<u32>,
    size64: Option<(u64, u64)>,
}

impl ExtHeaderData {
    fn apply(&mut self, ext_type: u8, data: &[u8]) {
        match ext_type {
            0x01 => self.filename = Some(name_codec::decode_lzh_name(data)),
            0x02 => self.dirname = Some(decode_dirname(data)),
            0x42 if data.len() >= 8 => {
                // 64 ビット非圧縮サイズ
                let original = u64::from_le_bytes([
                    data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
                ]);
                let compressed = self.size64.map(|(c, _)| c).unwrap_or(0);
                self.size64 = Some((compressed, original));
            }
            0x54 if data.len() >= 4 => {
                self.unix_mtime = Some(u32::from_le_bytes([data[0], data[1], data[2], data[3]]));
            }
            _ => {}
        }
    }
}

/// 2 バイトサイズの拡張ヘッダーチェーンを読み取る
///
/// 各拡張ヘッダーは宣言サイズ N の中に `[種別(1)][データ(N-3)][次サイズ(2)]` を
/// 含む（LHa for UNIX / delharc と同じ実仕様）。読み取った総バイト数を返す。
fn read_ext_chain<R: Read>(
    reader: &mut R,
    first_size: u16,
    ext: &mut ExtHeaderData,
) -> Result<u64, String> {
    let mut total: u64 = 0;
    let mut size = first_size as usize;
    while size > 0 {
        if size < 3 {
            return Err("LZH 拡張ヘッダーのサイズが不正です".to_string());
        }
        let mut block = vec![0u8; size];
        reader
            .read_exact(&mut block)
            .map_err(|e| format!("LZH 拡張ヘッダー読み込みエラー: {}", e))?;
        total += size as u64;

        let ext_type = block[0];
        let data = &block[1..size - 2];
        ext.apply(ext_type, data);

        size = u16::from_le_bytes([block[size - 2], block[size - 1]]) as usize;
    }
    Ok(total)
}

/// 基本ヘッダー 21 バイト（サイズ/チェックサム + 共通 19 バイト）から取り出す共通値
struct BaseFields {
    method: [u8; 5],
    compressed: u32,
    original: u32,
    time_raw: [u8; 4],
    level: u8,
}

fn parse_base_fields(buf: &[u8; 21]) -> BaseFields {
    let mut method = [0u8; 5];
    method.copy_from_slice(&buf[2..7]);
    BaseFields {
        method,
        compressed: u32::from_le_bytes([buf[7], buf[8], buf[9], buf[10]]),
        original: u32::from_le_bytes([buf[11], buf[12], buf[13], buf[14]]),
        time_raw: [buf[15], buf[16], buf[17], buf[18]],
        level: buf[20],
    }
}

/// 名前部品からエントリ名を組み立てる
fn assemble_name(base_name: String, ext: &ExtHeaderData) -> String {
    let file = ext.filename.clone().unwrap_or(base_name);
    match ext.dirname.as_deref() {
        Some(dir) if !dir.is_empty() && file.is_empty() => dir.to_string(),
        Some(dir) if !dir.is_empty() => format!("{}/{}", dir, file),
        _ => file,
    }
}

/// LZH アーカイブの全エントリを読み取る
pub(super) fn read_entries<R: Read + Seek>(reader: &mut R) -> Result<Vec<LzhEntry>, String> {
    let mut entries = Vec::new();
    let mut offset: u64 = 0;

    loop {
        reader
            .seek(SeekFrom::Start(offset))
            .map_err(|e| format!("LZH シークエラー: {}", e))?;

        // 終端判定: EOF または 0x00 の終端マーカー
        let mut first = [0u8; 1];
        match reader.read_exact(&mut first) {
            Ok(()) => {}
            Err(_) => break,
        }
        if first[0] == 0 {
            break;
        }

        let mut rest = [0u8; 20];
        reader
            .read_exact(&mut rest)
            .map_err(|e| format!("LZH ヘッダー読み込みエラー: {}", e))?;
        let mut buf = [0u8; 21];
        buf[0] = first[0];
        buf[1..].copy_from_slice(&rest);

        let base = parse_base_fields(&buf);
        let entry = match base.level {
            0 => parse_level0(reader, offset, &buf, &base)?,
            1 => parse_level1(reader, offset, &buf, &base)?,
            2 => parse_level2(reader, offset, &buf, &base)?,
            3 => parse_level3(reader, offset, &buf, &base)?,
            level => {
                return Err(format!("未対応の LZH ヘッダーレベル: {}", level));
            }
        };

        offset = entry.data_offset + entry.compressed_size;
        entries.push(entry);
    }

    Ok(entries)
}

/// 名前・種別・メソッドからエントリを構築する共通処理
#[allow(clippy::too_many_arguments)]
fn build_entry(
    name: String,
    method: [u8; 5],
    size: u64,
    compressed_size: u64,
    crc16: u16,
    data_offset: u64,
    mtime_unix: Option<i64>,
) -> LzhEntry {
    let is_dir = &method == b"-lhd-" || name.ends_with('/');
    let name = name.trim_end_matches('/').to_string();
    LzhEntry {
        name,
        is_dir,
        size: if is_dir { 0 } else { size },
        compressed_size: if is_dir { 0 } else { compressed_size },
        method,
        crc16,
        data_offset,
        mtime_unix,
    }
}

/// レベル 0 ヘッダー解析
fn parse_level0<R: Read>(
    reader: &mut R,
    offset: u64,
    buf: &[u8; 21],
    base: &BaseFields,
) -> Result<LzhEntry, String> {
    let header_size = buf[0] as u64; // 先頭 2 バイトを除いたヘッダーサイズ

    let mut len_buf = [0u8; 1];
    reader
        .read_exact(&mut len_buf)
        .map_err(|e| format!("LZH ヘッダー読み込みエラー: {}", e))?;
    let name_len = len_buf[0] as usize;
    let mut name_buf = vec![0u8; name_len];
    reader
        .read_exact(&mut name_buf)
        .map_err(|e| format!("LZH エントリ名読み込みエラー: {}", e))?;
    let mut crc_buf = [0u8; 2];
    reader
        .read_exact(&mut crc_buf)
        .map_err(|e| format!("LZH CRC 読み込みエラー: {}", e))?;
    let crc16 = u16::from_le_bytes(crc_buf);

    // 拡張エリア（OS ID など）は読み飛ばす
    let consumed = 19 + 1 + name_len as u64 + 2;
    if header_size < consumed {
        return Err("LZH レベル 0 ヘッダーのサイズが不正です".to_string());
    }

    // レベル 0 の名前は '\' 区切りのパスを含み得る
    let name = name_codec::decode_lzh_name(&name_buf).replace('\\', "/");
    let time = u16::from_le_bytes([base.time_raw[0], base.time_raw[1]]);
    let date = u16::from_le_bytes([base.time_raw[2], base.time_raw[3]]);

    Ok(build_entry(
        name,
        base.method,
        base.original as u64,
        base.compressed as u64,
        crc16,
        offset + 2 + header_size,
        dos_datetime_to_unix(time, date),
    ))
}

/// レベル 1 ヘッダー解析
fn parse_level1<R: Read>(
    reader: &mut R,
    offset: u64,
    buf: &[u8; 21],
    base: &BaseFields,
) -> Result<LzhEntry, String> {
    // レベル 1 のヘッダーサイズは先頭 2 バイトから拡張エリアまでを含む
    let header_size = buf[0] as u64;

    let mut len_buf = [0u8; 1];
    reader
        .read_exact(&mut len_buf)
        .map_err(|e| format!("LZH ヘッダー読み込みエラー: {}", e))?;
    let name_len = len_buf[0] as usize;
    let mut name_buf = vec![0u8; name_len];
    reader
        .read_exact(&mut name_buf)
        .map_err(|e| format!("LZH エントリ名読み込みエラー: {}", e))?;
    let mut crc_buf = [0u8; 2];
    reader
        .read_exact(&mut crc_buf)
        .map_err(|e| format!("LZH CRC 読み込みエラー: {}", e))?;
    let crc16 = u16::from_le_bytes(crc_buf);
    let mut os_buf = [0u8; 1];
    reader
        .read_exact(&mut os_buf)
        .map_err(|e| format!("LZH OS ID 読み込みエラー: {}", e))?;

    // 拡張エリア（基本ヘッダー内の余剰バイト）を読み飛ばす
    let consumed = 2 + 19 + 1 + name_len as u64 + 2 + 1;
    let extra_len = header_size
        .checked_sub(consumed)
        .ok_or_else(|| "LZH レベル 1 ヘッダーのサイズが不正です".to_string())?;
    if extra_len > 0 {
        let mut skip = vec![0u8; extra_len as usize];
        reader
            .read_exact(&mut skip)
            .map_err(|e| format!("LZH 拡張エリア読み込みエラー: {}", e))?;
    }

    // 最初の拡張ヘッダーサイズ（2 バイト）と拡張ヘッダーチェーン
    let mut first_buf = [0u8; 2];
    reader
        .read_exact(&mut first_buf)
        .map_err(|e| format!("LZH 拡張ヘッダーサイズ読み込みエラー: {}", e))?;
    let first_size = u16::from_le_bytes(first_buf);
    let mut ext = ExtHeaderData::default();
    let ext_total = read_ext_chain(reader, first_size, &mut ext)?;

    // レベル 1 の「圧縮サイズ」は拡張ヘッダー分を含むスキップサイズ
    let data_size = (base.compressed as u64)
        .checked_sub(ext_total)
        .ok_or_else(|| "LZH レベル 1 のスキップサイズが不正です".to_string())?;

    let base_name = name_codec::decode_lzh_name(&name_buf).replace('\\', "/");
    let name = assemble_name(base_name, &ext);
    let time = u16::from_le_bytes([base.time_raw[0], base.time_raw[1]]);
    let date = u16::from_le_bytes([base.time_raw[2], base.time_raw[3]]);
    let mtime = ext
        .unix_mtime
        .map(i64::from)
        .or_else(|| dos_datetime_to_unix(time, date));

    Ok(build_entry(
        name,
        base.method,
        base.original as u64,
        data_size,
        crc16,
        offset + header_size + 2 + ext_total,
        mtime,
    ))
}

/// レベル 2 ヘッダー解析
fn parse_level2<R: Read + Seek>(
    reader: &mut R,
    offset: u64,
    buf: &[u8; 21],
    base: &BaseFields,
) -> Result<LzhEntry, String> {
    // レベル 2 の先頭 2 バイトはヘッダー全体のサイズ (u16 LE)
    let total_size = u16::from_le_bytes([buf[0], buf[1]]) as u64;

    let mut fixed = [0u8; 5];
    reader
        .read_exact(&mut fixed)
        .map_err(|e| format!("LZH ヘッダー読み込みエラー: {}", e))?;
    let crc16 = u16::from_le_bytes([fixed[0], fixed[1]]);
    let _os_id = fixed[2];
    let first_size = u16::from_le_bytes([fixed[3], fixed[4]]);

    let mut ext = ExtHeaderData::default();
    read_ext_chain(reader, first_size, &mut ext)?;

    let name = assemble_name(String::new(), &ext);
    let mtime_unix = ext
        .unix_mtime
        .map(i64::from)
        .or(Some(i64::from(u32::from_le_bytes(base.time_raw))));
    let (compressed, original) = match ext.size64 {
        Some((_, original64)) => (base.compressed as u64, original64),
        None => (base.compressed as u64, base.original as u64),
    };

    Ok(build_entry(
        name,
        base.method,
        original,
        compressed,
        crc16,
        offset + total_size,
        mtime_unix,
    ))
}

/// レベル 3 ヘッダー解析（レベル 2 の 4 バイトサイズ版）
fn parse_level3<R: Read + Seek>(
    reader: &mut R,
    offset: u64,
    buf: &[u8; 21],
    base: &BaseFields,
) -> Result<LzhEntry, String> {
    // buf[0..2] はワードサイズ (0x0004 固定)
    if buf[0] != 0x04 || buf[1] != 0x00 {
        return Err("LZH レベル 3 ヘッダーのワードサイズが不正です".to_string());
    }

    let mut fixed = [0u8; 11];
    reader
        .read_exact(&mut fixed)
        .map_err(|e| format!("LZH ヘッダー読み込みエラー: {}", e))?;
    let crc16 = u16::from_le_bytes([fixed[0], fixed[1]]);
    let _os_id = fixed[2];
    let total_size = u32::from_le_bytes([fixed[3], fixed[4], fixed[5], fixed[6]]) as u64;
    let mut next_size = u32::from_le_bytes([fixed[7], fixed[8], fixed[9], fixed[10]]) as usize;

    // 4 バイトサイズの拡張ヘッダーチェーン
    let mut ext = ExtHeaderData::default();
    while next_size > 0 {
        if next_size < 5 {
            return Err("LZH レベル 3 拡張ヘッダーのサイズが不正です".to_string());
        }
        let mut block = vec![0u8; next_size];
        reader
            .read_exact(&mut block)
            .map_err(|e| format!("LZH 拡張ヘッダー読み込みエラー: {}", e))?;
        let ext_type = block[0];
        let data = &block[1..next_size - 4];
        ext.apply(ext_type, data);
        next_size = u32::from_le_bytes([
            block[next_size - 4],
            block[next_size - 3],
            block[next_size - 2],
            block[next_size - 1],
        ]) as usize;
    }

    let name = assemble_name(String::new(), &ext);
    let mtime_unix = ext
        .unix_mtime
        .map(i64::from)
        .or(Some(i64::from(u32::from_le_bytes(base.time_raw))));

    Ok(build_entry(
        name,
        base.method,
        base.original as u64,
        base.compressed as u64,
        crc16,
        offset + total_size,
        mtime_unix,
    ))
}

/// エントリの圧縮データを読み出して復号する
pub(super) fn read_entry_data<R: Read + Seek>(
    reader: &mut R,
    entry: &LzhEntry,
) -> Result<LzhData, String> {
    if entry.is_dir {
        return Ok(LzhData::Ok(Vec::new()));
    }

    reader
        .seek(SeekFrom::Start(entry.data_offset))
        .map_err(|e| format!("LZH シークエラー: {}", e))?;
    let mut compressed = vec![0u8; entry.compressed_size as usize];
    reader
        .read_exact(&mut compressed)
        .map_err(|e| format!("LZH データ読み込みエラー: {}", e))?;

    let decompressed = match &entry.method {
        b"-lh0-" | b"-lz4-" | b"-pm0-" => compressed,
        b"-lh1-" => lzhuf1::decode_lh1(&compressed, entry.size)
            .map_err(|e| format!("LZH (lh1) 解凍エラー: {}", e))?,
        b"-lh4-" | b"-lh5-" | b"-lh6-" | b"-lh7-" => {
            let method = LzhMethod::from_id(&entry.method).ok_or_else(|| {
                format!(
                    "LZH 圧縮方式の解決に失敗: {}",
                    String::from_utf8_lossy(&entry.method)
                )
            })?;
            oxiarc_lzhuf::decode_lzh(&compressed, method, entry.size)
                .map_err(|e| format!("LZH 解凍エラー: {}", e))?
        }
        other => {
            return Ok(LzhData::Unsupported(
                String::from_utf8_lossy(other).into_owned(),
            ));
        }
    };

    // CRC-16 検証
    let computed = Crc16::compute(&decompressed);
    if computed != entry.crc16 {
        return Err(format!(
            "LZH CRC-16 不一致 ({}): 期待 {:04X} 実際 {:04X}",
            entry.name, entry.crc16, computed
        ));
    }

    Ok(LzhData::Ok(decompressed))
}

// ---------------------------------------------------------------------------
// ライター（レベル 2 ヘッダー・lh0 格納・Shift_JIS 名）
// ---------------------------------------------------------------------------

/// LZH ライター
///
/// - ヘッダーレベル 2（LHA 2.x 以降・Lhaplus・delharc が対応する標準形式）
/// - ファイル名は Shift_JIS（LZH の慣習。レガシー日本語ツールと互換）
/// - oxiarc-archive 0.3.3 の lh5 エンコーダーは 8KB 超のデータで往復破損を
///   起こす既知の問題があるため、上流修正まで無圧縮 (lh0) で格納して
///   データの完全性を保証する
pub(super) struct LzhWriter2<W: Write> {
    out: W,
    finished: bool,
}

impl<W: Write> LzhWriter2<W> {
    pub(super) fn new(out: W) -> Self {
        Self {
            out,
            finished: false,
        }
    }

    /// ファイルを lh0（無圧縮）で追加する
    pub(super) fn add_file(
        &mut self,
        name: &str,
        data: &[u8],
        mtime_unix: u32,
    ) -> Result<(), String> {
        let crc16 = Crc16::compute(data);
        let header = build_level2_header(
            b"-lh0-",
            name,
            data.len() as u32,
            data.len() as u32,
            mtime_unix,
            crc16,
        )?;
        self.out
            .write_all(&header)
            .map_err(|e| format!("LZH ヘッダー書き込みエラー: {}", e))?;
        self.out
            .write_all(data)
            .map_err(|e| format!("LZH データ書き込みエラー: {}", e))?;
        Ok(())
    }

    /// ディレクトリエントリ (`-lhd-`) を追加する
    pub(super) fn add_directory(&mut self, name: &str, mtime_unix: u32) -> Result<(), String> {
        let header = build_level2_header(b"-lhd-", name, 0, 0, mtime_unix, 0)?;
        self.out
            .write_all(&header)
            .map_err(|e| format!("LZH ヘッダー書き込みエラー: {}", e))?;
        Ok(())
    }

    /// 終端マーカーを書き込んで完了する
    pub(super) fn finish(&mut self) -> Result<(), String> {
        if !self.finished {
            self.out
                .write_all(&[0u8])
                .map_err(|e| format!("LZH 終端書き込みエラー: {}", e))?;
            self.out
                .flush()
                .map_err(|e| format!("LZH フラッシュエラー: {}", e))?;
            self.finished = true;
        }
        Ok(())
    }
}

/// レベル 2 ヘッダーを構築する
fn build_level2_header(
    method: &[u8; 5],
    name: &str,
    compressed: u32,
    original: u32,
    mtime_unix: u32,
    crc16: u16,
) -> Result<Vec<u8>, String> {
    let normalized = name.trim_end_matches('/');
    let is_dir = method == b"-lhd-";

    // パスをディレクトリ部とファイル名部に分割する
    let (dir_part, file_part): (Option<&str>, &str) = if is_dir {
        (Some(normalized), "")
    } else {
        match normalized.rsplit_once('/') {
            Some((dir, file)) => (Some(dir), file),
            None => (None, normalized),
        }
    };

    // 拡張ヘッダーブロック: [種別(1)][データ][次サイズ(2)] で宣言サイズ = 全長
    let mut blocks: Vec<(u8, Vec<u8>)> = Vec::new();
    if !file_part.is_empty() {
        blocks.push((0x01, name_codec::encode_lzh_name(file_part)));
    }
    if let Some(dir) = dir_part {
        if !dir.is_empty() {
            // コンポーネントごとに Shift_JIS へ符号化し 0xFF で終端する
            let mut encoded = Vec::new();
            for component in dir.split('/').filter(|c| !c.is_empty()) {
                encoded.extend_from_slice(&name_codec::encode_lzh_name(component));
                encoded.push(0xFF);
            }
            blocks.push((0x02, encoded));
        }
    }
    if blocks.is_empty() {
        return Err("LZH エントリ名が空です".to_string());
    }

    let block_sizes: Vec<usize> = blocks.iter().map(|(_, data)| 3 + data.len()).collect();
    for &size in &block_sizes {
        if size > u16::MAX as usize {
            return Err("LZH エントリ名が長すぎます".to_string());
        }
    }

    let mut total: usize = 26 + block_sizes.iter().sum::<usize>();
    // ヘッダー総サイズの下位バイトが 0 だとレベル 0/1 の終端マーカーと誤認される
    // ため、1 バイトのパディングを追加して回避する（LHA と同じ慣習）
    let pad = total.is_multiple_of(256);
    if pad {
        total += 1;
    }
    if total > u16::MAX as usize {
        return Err("LZH ヘッダーが長すぎます".to_string());
    }

    let mut header = Vec::with_capacity(total);
    header.extend_from_slice(&(total as u16).to_le_bytes()); // ヘッダー総サイズ
    header.extend_from_slice(method);
    header.extend_from_slice(&compressed.to_le_bytes());
    header.extend_from_slice(&original.to_le_bytes());
    header.extend_from_slice(&mtime_unix.to_le_bytes());
    header.push(0x20); // 予約属性
    header.push(0x02); // ヘッダーレベル 2
    header.extend_from_slice(&crc16.to_le_bytes());
    header.push(0x4D); // OS ID: 'M' (MS-DOS 汎用)

    // 拡張ヘッダーチェーン: 基本ヘッダー末尾に最初のサイズ、各ブロック末尾に次サイズ
    for (index, (ext_type, data)) in blocks.iter().enumerate() {
        header.extend_from_slice(&(block_sizes[index] as u16).to_le_bytes());
        header.push(*ext_type);
        header.extend_from_slice(data);
    }
    header.extend_from_slice(&0u16.to_le_bytes()); // チェーン終端

    if pad {
        header.push(0);
    }

    if header.len() != total {
        return Err("LZH ヘッダー構築の内部整合性エラー".to_string());
    }

    Ok(header)
}
