//! FVRS 独自の 7z アーカイブリーダー
//!
//! oxiarc-archive 0.3.3 の `SevenZReader` には以下の問題があり、
//! 全エントリが 0 バイトで抽出される・空ファイル入りアーカイブの解凍が
//! 失敗する等の致命的な動作になるため、上流修正まで本モジュールで
//! 7z ヘッダー解析と解凍を自前で行う。
//!
//! 1. `assign_folder_info()` が `entry.size` を常に 0 の `offset_in_folder` で
//!    上書きし、`parse_substreams_info()` がサブストリームサイズを読み捨てる
//! 2. 空ファイル（ストリーム無し）の抽出がハードエラーになる
//! 3. 可変長数値の追加バイトをビッグエンディアン扱いで復号する（仕様は
//!    リトルエンディアン）ため 16384 以上のサイズが誤読される
//!
//! 対応コーデック: Copy / LZMA / LZMA2 / BZip2 / Deflate / Delta / BCJ(x86)。
//! LZMA / LZMA2 は仕様準拠の自前デコーダー（`lzma_compat`、oxiarc-lzma 0.3.3 は
//! 実在ストリームと非互換のため）、Deflate / BZip2 は oxiarc に委譲する。

use std::io::{Read, Seek, SeekFrom};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use oxiarc_core::Crc32;

use super::lzma_compat;

/// 7z シグネチャ
const SEVENZ_MAGIC: [u8; 6] = [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C];

// プロパティ ID（7zFormat.txt）
const K_END: u8 = 0x00;
const K_HEADER: u8 = 0x01;
const K_ARCHIVE_PROPERTIES: u8 = 0x02;
const K_MAIN_STREAMS_INFO: u8 = 0x04;
const K_FILES_INFO: u8 = 0x05;
const K_PACK_INFO: u8 = 0x06;
const K_UNPACK_INFO: u8 = 0x07;
const K_SUBSTREAMS_INFO: u8 = 0x08;
const K_SIZE: u8 = 0x09;
const K_CRC: u8 = 0x0A;
const K_FOLDER: u8 = 0x0B;
const K_CODERS_UNPACK_SIZE: u8 = 0x0C;
const K_NUM_UNPACK_STREAM: u8 = 0x0D;
const K_EMPTY_STREAM: u8 = 0x0E;
const K_EMPTY_FILE: u8 = 0x0F;
const K_ANTI: u8 = 0x10;
const K_NAME: u8 = 0x11;
const K_MTIME: u8 = 0x14;
const K_ENCODED_HEADER: u8 = 0x17;

/// エントリ数の常識的上限（ヘッダー破損によるメモリ枯渇を防ぐ）
const MAX_REASONABLE_COUNT: u64 = 16 * 1024 * 1024;

/// バイト列カーソル（すべて境界チェック付き）
struct ByteCursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> ByteCursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn u8(&mut self) -> Result<u8, String> {
        let byte = *self
            .data
            .get(self.pos)
            .ok_or_else(|| "7Z ヘッダーが途中で終わっています".to_string())?;
        self.pos += 1;
        Ok(byte)
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], String> {
        let end = self
            .pos
            .checked_add(len)
            .filter(|&end| end <= self.data.len())
            .ok_or_else(|| "7Z ヘッダーが途中で終わっています".to_string())?;
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn u32_le(&mut self) -> Result<u32, String> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn u64_le(&mut self) -> Result<u64, String> {
        let bytes = self.take(8)?;
        Ok(u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    /// 7z 可変長数値（追加バイトはリトルエンディアン、先頭バイトの残りが上位）
    fn number(&mut self) -> Result<u64, String> {
        let first = self.u8()?;
        let mut mask: u8 = 0x80;
        let mut value: u64 = 0;
        for i in 0..8 {
            if first & mask == 0 {
                let high = u64::from(first & mask.wrapping_sub(1));
                return Ok(value | (high << (8 * i)));
            }
            value |= u64::from(self.u8()?) << (8 * i);
            mask >>= 1;
        }
        Ok(value)
    }

    fn count(&mut self) -> Result<usize, String> {
        let value = self.number()?;
        if value > MAX_REASONABLE_COUNT {
            return Err(format!("7Z ヘッダーの要素数が異常です: {}", value));
        }
        Ok(value as usize)
    }

    /// MSB ファーストのビットベクター
    fn bits(&mut self, count: usize) -> Result<Vec<bool>, String> {
        let bytes = self.take(count.div_ceil(8))?;
        Ok((0..count)
            .map(|i| (bytes[i / 8] >> (7 - (i % 8))) & 1 != 0)
            .collect())
    }

    /// AllAreDefined 形式のビットベクター
    fn optional_bits(&mut self, count: usize) -> Result<Vec<bool>, String> {
        let all_defined = self.u8()?;
        if all_defined != 0 {
            Ok(vec![true; count])
        } else {
            self.bits(count)
        }
    }

    /// CRC ダイジェスト列（定義ビット + u32 LE）
    fn digests(&mut self, count: usize) -> Result<Vec<Option<u32>>, String> {
        let defined = self.optional_bits(count)?;
        defined
            .into_iter()
            .map(|is_defined| {
                if is_defined {
                    Ok(Some(self.u32_le()?))
                } else {
                    Ok(None)
                }
            })
            .collect()
    }
}

/// フォルダー内のコーダー
struct Coder {
    id: Vec<u8>,
    num_in: u64,
    num_out: u64,
    props: Vec<u8>,
}

/// フォルダー（圧縮単位）
struct Folder {
    coders: Vec<Coder>,
    /// (入力ストリーム索引, 出力ストリーム索引)
    bind_pairs: Vec<(u64, u64)>,
    /// パックストリームに接続される入力ストリーム索引
    packed_indices: Vec<u64>,
    /// 出力ストリームごとの解凍後サイズ
    unpack_sizes: Vec<u64>,
    crc: Option<u32>,
}

impl Folder {
    /// 最終出力ストリーム（バインドされていない出力）の索引
    fn main_output(&self) -> Option<usize> {
        (0..self.unpack_sizes.len())
            .find(|&out| !self.bind_pairs.iter().any(|&(_, o)| o as usize == out))
    }

    /// フォルダー全体の解凍後サイズ
    fn output_size(&self) -> u64 {
        self.main_output()
            .and_then(|out| self.unpack_sizes.get(out).copied())
            .unwrap_or(0)
    }
}

/// ストリーム情報（メインヘッダー・符号化ヘッダーの両方で使用）
struct StreamsInfo {
    /// パックストリーム先頭の絶対オフセット（シグネチャヘッダー 32 バイトを含む）
    pack_pos: u64,
    pack_sizes: Vec<u64>,
    folders: Vec<Folder>,
    /// フォルダーごとのサブストリーム数
    num_unpack_streams: Vec<u64>,
    /// (フォルダー, サブストリーム) 順のサイズ一覧
    substream_sizes: Vec<u64>,
    /// (フォルダー, サブストリーム) 順の CRC 一覧
    substream_crcs: Vec<Option<u32>>,
}

impl StreamsInfo {
    fn empty() -> Self {
        Self {
            pack_pos: 32,
            pack_sizes: Vec::new(),
            folders: Vec::new(),
            num_unpack_streams: Vec::new(),
            substream_sizes: Vec::new(),
            substream_crcs: Vec::new(),
        }
    }

    /// フォルダーが使用するパックストリームの (先頭索引, 本数)
    fn folder_pack_range(&self, folder_index: usize) -> (usize, usize) {
        let start: usize = self.folders[..folder_index]
            .iter()
            .map(|f| f.packed_indices.len())
            .sum();
        (start, self.folders[folder_index].packed_indices.len())
    }

    /// フォルダーのパック済み合計サイズ
    fn folder_packed_size(&self, folder_index: usize) -> u64 {
        let (start, count) = self.folder_pack_range(folder_index);
        self.pack_sizes
            .iter()
            .skip(start)
            .take(count)
            .copied()
            .sum()
    }
}

/// FilesInfo の解析結果
struct FilesInfo {
    names: Vec<String>,
    empty_stream: Vec<bool>,
    empty_file: Vec<bool>,
    anti: Vec<bool>,
    mtimes: Vec<Option<SystemTime>>,
}

/// 7z エントリ情報
pub(super) struct SevenZEntryInfo {
    pub name: String,
    pub is_dir: bool,
    pub is_anti: bool,
    pub size: u64,
    /// 一覧表示用の圧縮後サイズ（単独サブストリームのフォルダーのみ厳密値）
    pub packed_size: u64,
    pub mtime: Option<SystemTime>,
    crc: Option<u32>,
    folder: Option<usize>,
    offset_in_folder: u64,
}

/// 7z アーカイブリーダー
pub(super) struct SevenZArchive<R: Read + Seek> {
    reader: R,
    streams: StreamsInfo,
    entries: Vec<SevenZEntryInfo>,
    /// 直近に復号したフォルダーのキャッシュ（同一フォルダー内の連続抽出を高速化）
    cache: Option<(usize, Vec<u8>)>,
}

impl<R: Read + Seek> SevenZArchive<R> {
    /// アーカイブを開いてヘッダーを解析する
    pub(super) fn new(mut reader: R) -> Result<Self, String> {
        let mut sig = [0u8; 32];
        reader
            .read_exact(&mut sig)
            .map_err(|e| format!("7Z シグネチャ読み込みエラー: {}", e))?;
        if sig[0..6] != SEVENZ_MAGIC {
            return Err("7Z シグネチャが不正です".to_string());
        }

        let start_crc = u32::from_le_bytes([sig[8], sig[9], sig[10], sig[11]]);
        if Crc32::compute(&sig[12..32]) != start_crc {
            return Err("7Z 開始ヘッダーの CRC が一致しません".to_string());
        }

        let next_offset = u64::from_le_bytes([
            sig[12], sig[13], sig[14], sig[15], sig[16], sig[17], sig[18], sig[19],
        ]);
        let next_size = u64::from_le_bytes([
            sig[20], sig[21], sig[22], sig[23], sig[24], sig[25], sig[26], sig[27],
        ]);
        let next_crc = u32::from_le_bytes([sig[28], sig[29], sig[30], sig[31]]);

        if next_size > (1u64 << 31) {
            return Err("7Z ヘッダーサイズが異常です".to_string());
        }
        reader
            .seek(SeekFrom::Start(32 + next_offset))
            .map_err(|e| format!("7Z シークエラー: {}", e))?;
        let mut header_data = vec![0u8; next_size as usize];
        reader
            .read_exact(&mut header_data)
            .map_err(|e| format!("7Z ヘッダー読み込みエラー: {}", e))?;
        if Crc32::compute(&header_data) != next_crc {
            return Err("7Z ヘッダーの CRC が一致しません".to_string());
        }

        let mut cursor = ByteCursor::new(&header_data);
        let (streams, files) = match cursor.u8()? {
            K_HEADER => parse_header_body(&mut cursor)?,
            K_ENCODED_HEADER => {
                // ヘッダー自体が圧縮されている場合は復号してから再解析する
                let header_streams = parse_streams_info(&mut cursor)?;
                if header_streams.folders.is_empty() {
                    return Err("7Z 符号化ヘッダーのフォルダーがありません".to_string());
                }
                let decoded = decode_folder_data(&mut reader, &header_streams, 0)?;
                let mut inner = ByteCursor::new(&decoded);
                if inner.u8()? != K_HEADER {
                    return Err("7Z 符号化ヘッダーの内容が不正です".to_string());
                }
                parse_header_body(&mut inner)?
            }
            other => {
                return Err(format!("7Z ヘッダー種別が不正です: 0x{:02X}", other));
            }
        };

        let entries = assemble_entries(&streams, files)?;

        Ok(Self {
            reader,
            streams,
            entries,
            cache: None,
        })
    }

    /// エントリ一覧
    pub(super) fn entries(&self) -> &[SevenZEntryInfo] {
        &self.entries
    }

    /// エントリのデータを復号して返す（ディレクトリ・空ファイルは空データ）
    pub(super) fn read_file_data(&mut self, index: usize) -> Result<Vec<u8>, String> {
        let (folder_index, offset, size, crc, name) = {
            let entry = self
                .entries
                .get(index)
                .ok_or_else(|| "7Z エントリ索引が不正です".to_string())?;
            let Some(folder_index) = entry.folder else {
                // ディレクトリまたは空ファイル
                return Ok(Vec::new());
            };
            (
                folder_index,
                entry.offset_in_folder as usize,
                entry.size as usize,
                entry.crc,
                entry.name.clone(),
            )
        };

        let cache_hit = matches!(&self.cache, Some((cached, _)) if *cached == folder_index);
        if !cache_hit {
            let data = decode_folder_data(&mut self.reader, &self.streams, folder_index)?;
            self.cache = Some((folder_index, data));
        }
        let Some((_, folder_data)) = &self.cache else {
            return Err("7Z フォルダーキャッシュの内部整合性エラー".to_string());
        };

        let end = offset
            .checked_add(size)
            .filter(|&end| end <= folder_data.len())
            .ok_or_else(|| format!("7Z エントリがフォルダー範囲外です: {}", name))?;
        let data = folder_data[offset..end].to_vec();

        if let Some(expected) = crc {
            let actual = Crc32::compute(&data);
            if actual != expected {
                return Err(format!(
                    "7Z CRC-32 不一致 ({}): 期待 {:08X} 実際 {:08X}",
                    name, expected, actual
                ));
            }
        }

        Ok(data)
    }
}

/// kHeader の本体（MainStreamsInfo + FilesInfo）を解析する
fn parse_header_body(cursor: &mut ByteCursor<'_>) -> Result<(StreamsInfo, FilesInfo), String> {
    let mut streams = StreamsInfo::empty();
    let mut files = FilesInfo {
        names: Vec::new(),
        empty_stream: Vec::new(),
        empty_file: Vec::new(),
        anti: Vec::new(),
        mtimes: Vec::new(),
    };

    loop {
        let id = cursor.u8()?;
        match id {
            K_END => break,
            K_MAIN_STREAMS_INFO => streams = parse_streams_info(cursor)?,
            K_FILES_INFO => files = parse_files_info(cursor)?,
            K_ARCHIVE_PROPERTIES => skip_archive_properties(cursor)?,
            other => {
                return Err(format!("7Z ヘッダーの未知プロパティ: 0x{:02X}", other));
            }
        }
    }

    Ok((streams, files))
}

/// ArchiveProperties を読み飛ばす
fn skip_archive_properties(cursor: &mut ByteCursor<'_>) -> Result<(), String> {
    loop {
        let prop_type = cursor.u8()?;
        if prop_type == K_END {
            return Ok(());
        }
        let size = cursor.count()?;
        cursor.take(size)?;
    }
}

/// StreamsInfo（PackInfo / UnpackInfo / SubStreamsInfo）を解析する
fn parse_streams_info(cursor: &mut ByteCursor<'_>) -> Result<StreamsInfo, String> {
    let mut info = StreamsInfo::empty();
    let mut substreams_seen = false;

    loop {
        let id = cursor.u8()?;
        match id {
            K_END => break,
            K_PACK_INFO => parse_pack_info(cursor, &mut info)?,
            K_UNPACK_INFO => parse_unpack_info(cursor, &mut info)?,
            K_SUBSTREAMS_INFO => {
                parse_substreams_info(cursor, &mut info)?;
                substreams_seen = true;
            }
            other => {
                return Err(format!("7Z StreamsInfo の未知プロパティ: 0x{:02X}", other));
            }
        }
    }

    if !substreams_seen {
        // SubStreamsInfo が無い場合はフォルダーごとに 1 サブストリーム
        info.num_unpack_streams = vec![1; info.folders.len()];
        info.substream_sizes = info.folders.iter().map(Folder::output_size).collect();
        info.substream_crcs = info.folders.iter().map(|f| f.crc).collect();
    }

    Ok(info)
}

/// PackInfo を解析する
fn parse_pack_info(cursor: &mut ByteCursor<'_>, info: &mut StreamsInfo) -> Result<(), String> {
    info.pack_pos = 32u64
        .checked_add(cursor.number()?)
        .ok_or_else(|| "7Z パック位置が異常です".to_string())?;
    let num_streams = cursor.count()?;

    loop {
        let id = cursor.u8()?;
        match id {
            K_END => break,
            K_SIZE => {
                info.pack_sizes = (0..num_streams)
                    .map(|_| cursor.number())
                    .collect::<Result<_, _>>()?;
            }
            K_CRC => {
                let _ = cursor.digests(num_streams)?;
            }
            other => {
                return Err(format!("7Z PackInfo の未知プロパティ: 0x{:02X}", other));
            }
        }
    }

    if info.pack_sizes.len() != num_streams {
        return Err("7Z パックサイズが読み取れませんでした".to_string());
    }
    Ok(())
}

/// UnpackInfo（フォルダー定義）を解析する
fn parse_unpack_info(cursor: &mut ByteCursor<'_>, info: &mut StreamsInfo) -> Result<(), String> {
    if cursor.u8()? != K_FOLDER {
        return Err("7Z UnpackInfo に kFolder がありません".to_string());
    }
    let num_folders = cursor.count()?;
    if cursor.u8()? != 0 {
        return Err("7Z 外部フォルダー定義は未対応です".to_string());
    }
    info.folders = (0..num_folders)
        .map(|_| parse_folder(cursor))
        .collect::<Result<_, _>>()?;

    if cursor.u8()? != K_CODERS_UNPACK_SIZE {
        return Err("7Z UnpackInfo に kCodersUnpackSize がありません".to_string());
    }
    for folder in &mut info.folders {
        let num_out: u64 = folder.coders.iter().map(|c| c.num_out).sum();
        folder.unpack_sizes = (0..num_out)
            .map(|_| cursor.number())
            .collect::<Result<_, _>>()?;
    }

    loop {
        let id = cursor.u8()?;
        match id {
            K_END => break,
            K_CRC => {
                let crcs = cursor.digests(info.folders.len())?;
                for (folder, crc) in info.folders.iter_mut().zip(crcs) {
                    folder.crc = crc;
                }
            }
            other => {
                return Err(format!("7Z UnpackInfo の未知プロパティ: 0x{:02X}", other));
            }
        }
    }

    Ok(())
}

/// フォルダー 1 つ分（コーダー列・バインドペア・パック索引）を解析する
fn parse_folder(cursor: &mut ByteCursor<'_>) -> Result<Folder, String> {
    let num_coders = cursor.count()?;
    if num_coders == 0 {
        return Err("7Z フォルダーにコーダーがありません".to_string());
    }

    let mut coders = Vec::with_capacity(num_coders);
    let mut total_in: u64 = 0;
    let mut total_out: u64 = 0;
    for _ in 0..num_coders {
        let flags = cursor.u8()?;
        let id_size = (flags & 0x0F) as usize;
        let is_complex = flags & 0x10 != 0;
        let has_attrs = flags & 0x20 != 0;
        let id = cursor.take(id_size)?.to_vec();

        let (num_in, num_out) = if is_complex {
            (cursor.number()?, cursor.number()?)
        } else {
            (1, 1)
        };
        total_in = total_in
            .checked_add(num_in)
            .ok_or_else(|| "7Z コーダー入力数が異常です".to_string())?;
        total_out = total_out
            .checked_add(num_out)
            .ok_or_else(|| "7Z コーダー出力数が異常です".to_string())?;

        let props = if has_attrs {
            let size = cursor.count()?;
            cursor.take(size)?.to_vec()
        } else {
            Vec::new()
        };

        coders.push(Coder {
            id,
            num_in,
            num_out,
            props,
        });
    }

    let num_bind_pairs = total_out.saturating_sub(1);
    let mut bind_pairs = Vec::with_capacity(num_bind_pairs as usize);
    for _ in 0..num_bind_pairs {
        let in_index = cursor.number()?;
        let out_index = cursor.number()?;
        bind_pairs.push((in_index, out_index));
    }

    let num_packed = total_in.saturating_sub(num_bind_pairs);
    let packed_indices = if num_packed == 1 {
        // バインドされていない入力ストリームを探す
        let unbound = (0..total_in)
            .find(|i| !bind_pairs.iter().any(|&(in_idx, _)| in_idx == *i))
            .ok_or_else(|| "7Z パックストリーム索引が特定できません".to_string())?;
        vec![unbound]
    } else {
        (0..num_packed)
            .map(|_| cursor.number())
            .collect::<Result<_, _>>()?
    };

    Ok(Folder {
        coders,
        bind_pairs,
        packed_indices,
        unpack_sizes: Vec::new(),
        crc: None,
    })
}

/// SubStreamsInfo を解析する
fn parse_substreams_info(
    cursor: &mut ByteCursor<'_>,
    info: &mut StreamsInfo,
) -> Result<(), String> {
    let mut nums: Vec<u64> = vec![1; info.folders.len()];
    let mut sizes: Vec<u64> = Vec::new();
    let mut sizes_read = false;
    let mut crcs: Vec<Option<u32>> = Vec::new();
    let mut crcs_read = false;

    loop {
        let id = cursor.u8()?;
        match id {
            K_END => break,
            K_NUM_UNPACK_STREAM => {
                for num in nums.iter_mut() {
                    *num = cursor.number()?;
                    if *num > MAX_REASONABLE_COUNT {
                        return Err("7Z サブストリーム数が異常です".to_string());
                    }
                }
            }
            K_SIZE => {
                // 各フォルダーの先頭 (n-1) 個を読み、最後の 1 個は残差から求める
                for (folder, &num) in info.folders.iter().zip(nums.iter()) {
                    if num == 0 {
                        continue;
                    }
                    let mut sum: u64 = 0;
                    for _ in 1..num {
                        let size = cursor.number()?;
                        sizes.push(size);
                        sum = sum
                            .checked_add(size)
                            .ok_or_else(|| "7Z サブストリームサイズが異常です".to_string())?;
                    }
                    let last = folder
                        .output_size()
                        .checked_sub(sum)
                        .ok_or_else(|| "7Z サブストリームサイズの合計が不正です".to_string())?;
                    sizes.push(last);
                }
                sizes_read = true;
            }
            K_CRC => {
                // フォルダー CRC が既知の単独サブストリームは対象外
                let unknown_count: usize = info
                    .folders
                    .iter()
                    .zip(nums.iter())
                    .map(|(folder, &num)| {
                        if num == 1 && folder.crc.is_some() {
                            0
                        } else {
                            num as usize
                        }
                    })
                    .sum();
                let digests = cursor.digests(unknown_count)?;
                let mut digest_iter = digests.into_iter();
                for (folder, &num) in info.folders.iter().zip(nums.iter()) {
                    if num == 1 && folder.crc.is_some() {
                        crcs.push(folder.crc);
                    } else {
                        for _ in 0..num {
                            crcs.push(digest_iter.next().flatten());
                        }
                    }
                }
                crcs_read = true;
            }
            other => {
                return Err(format!(
                    "7Z SubStreamsInfo の未知プロパティ: 0x{:02X}",
                    other
                ));
            }
        }
    }

    if !sizes_read {
        for (folder, &num) in info.folders.iter().zip(nums.iter()) {
            match num {
                0 => {}
                1 => sizes.push(folder.output_size()),
                _ => {
                    return Err("7Z サブストリームサイズ情報がありません".to_string());
                }
            }
        }
    }
    if !crcs_read {
        for (folder, &num) in info.folders.iter().zip(nums.iter()) {
            for _ in 0..num {
                crcs.push(if num == 1 { folder.crc } else { None });
            }
        }
    }

    info.num_unpack_streams = nums;
    info.substream_sizes = sizes;
    info.substream_crcs = crcs;
    Ok(())
}

/// FilesInfo を解析する
fn parse_files_info(cursor: &mut ByteCursor<'_>) -> Result<FilesInfo, String> {
    let num_files = cursor.count()?;

    let mut info = FilesInfo {
        names: Vec::new(),
        empty_stream: vec![false; num_files],
        empty_file: Vec::new(),
        anti: Vec::new(),
        mtimes: vec![None; num_files],
    };

    loop {
        let id = cursor.u8()?;
        if id == K_END {
            break;
        }
        let size = cursor.count()?;
        let block = cursor.take(size)?;
        let mut sub = ByteCursor::new(block);

        match id {
            K_EMPTY_STREAM => {
                info.empty_stream = sub.bits(num_files)?;
            }
            K_EMPTY_FILE => {
                let num_empty = info.empty_stream.iter().filter(|&&e| e).count();
                info.empty_file = sub.bits(num_empty)?;
            }
            K_ANTI => {
                let num_empty = info.empty_stream.iter().filter(|&&e| e).count();
                info.anti = sub.bits(num_empty)?;
            }
            K_NAME => {
                if sub.u8()? != 0 {
                    return Err("7Z 外部ファイル名は未対応です".to_string());
                }
                for _ in 0..num_files {
                    info.names.push(read_utf16_name(&mut sub)?);
                }
            }
            K_MTIME => {
                let defined = sub.optional_bits(num_files)?;
                if sub.u8()? != 0 {
                    return Err("7Z 外部タイムスタンプは未対応です".to_string());
                }
                for (i, is_defined) in defined.iter().enumerate() {
                    if *is_defined {
                        info.mtimes[i] = filetime_to_system_time(sub.u64_le()?);
                    }
                }
            }
            _ => {
                // 属性・作成/アクセス時刻・kDummy 等はサイズ分読み飛ばし済み
            }
        }
    }

    if info.names.len() != num_files {
        return Err("7Z ファイル名の数が一致しません".to_string());
    }
    Ok(info)
}

/// NUL 終端の UTF-16LE 名を 1 つ読み取る
fn read_utf16_name(cursor: &mut ByteCursor<'_>) -> Result<String, String> {
    let mut units = Vec::new();
    loop {
        let bytes = cursor.take(2)?;
        let unit = u16::from_le_bytes([bytes[0], bytes[1]]);
        if unit == 0 {
            break;
        }
        units.push(unit);
    }
    // 7z はパス区切りに '\\' を使う場合がある
    Ok(String::from_utf16_lossy(&units).replace('\\', "/"))
}

/// Windows FILETIME を SystemTime へ変換する
fn filetime_to_system_time(filetime: u64) -> Option<SystemTime> {
    const EPOCH_DIFF: u64 = 116_444_736_000_000_000; // 1601-01-01 から 1970-01-01 の 100ns 数
    let unix_100ns = filetime.checked_sub(EPOCH_DIFF)?;
    let secs = unix_100ns / 10_000_000;
    let nanos = (unix_100ns % 10_000_000) * 100;
    Some(UNIX_EPOCH + Duration::new(secs, nanos as u32))
}

/// StreamsInfo と FilesInfo からエントリ一覧を組み立てる
fn assemble_entries(
    streams: &StreamsInfo,
    files: FilesInfo,
) -> Result<Vec<SevenZEntryInfo>, String> {
    let num_files = files.names.len();
    let total_substreams: u64 = streams.num_unpack_streams.iter().sum();
    let stream_files = files.empty_stream.iter().filter(|&&e| !e).count() as u64;
    if stream_files != total_substreams {
        return Err(format!(
            "7Z ストリーム数の不整合: ファイル {} / サブストリーム {}",
            stream_files, total_substreams
        ));
    }

    let mut entries = Vec::with_capacity(num_files);
    let mut folder_index = 0usize;
    let mut within_folder = 0u64;
    let mut offset_in_folder = 0u64;
    let mut global_stream = 0usize;
    let mut empty_rank = 0usize;

    for i in 0..num_files {
        let name = files.names[i].clone();
        let mtime = files.mtimes[i];

        if files.empty_stream[i] {
            let is_empty_file = files.empty_file.get(empty_rank).copied().unwrap_or(false);
            let is_anti = files.anti.get(empty_rank).copied().unwrap_or(false);
            empty_rank += 1;
            entries.push(SevenZEntryInfo {
                name,
                // ストリーム無し・空ファイル指定無しはディレクトリ
                is_dir: !is_empty_file && !is_anti,
                is_anti,
                size: 0,
                packed_size: 0,
                mtime,
                crc: None,
                folder: None,
                offset_in_folder: 0,
            });
            continue;
        }

        // 次のサブストリームを持つフォルダーへ進める
        while folder_index < streams.folders.len()
            && within_folder
                >= streams
                    .num_unpack_streams
                    .get(folder_index)
                    .copied()
                    .unwrap_or(0)
        {
            folder_index += 1;
            within_folder = 0;
            offset_in_folder = 0;
        }
        if folder_index >= streams.folders.len() {
            return Err("7Z フォルダー割り当ての不整合".to_string());
        }

        let size = streams
            .substream_sizes
            .get(global_stream)
            .copied()
            .ok_or_else(|| "7Z サブストリームサイズの不整合".to_string())?;
        let crc = streams.substream_crcs.get(global_stream).copied().flatten();
        let single = streams
            .num_unpack_streams
            .get(folder_index)
            .copied()
            .unwrap_or(1)
            == 1;

        entries.push(SevenZEntryInfo {
            name,
            is_dir: false,
            is_anti: false,
            size,
            packed_size: if single {
                streams.folder_packed_size(folder_index)
            } else {
                0
            },
            mtime,
            crc,
            folder: Some(folder_index),
            offset_in_folder,
        });

        offset_in_folder = offset_in_folder
            .checked_add(size)
            .ok_or_else(|| "7Z フォルダー内オフセットが異常です".to_string())?;
        within_folder += 1;
        global_stream += 1;
    }

    Ok(entries)
}

/// フォルダー全体を読み込んで復号する
fn decode_folder_data<R: Read + Seek>(
    reader: &mut R,
    streams: &StreamsInfo,
    folder_index: usize,
) -> Result<Vec<u8>, String> {
    let folder = streams
        .folders
        .get(folder_index)
        .ok_or_else(|| "7Z フォルダー索引が不正です".to_string())?;

    if folder.packed_indices.len() != 1 {
        return Err("7Z の複数入力コーダー構成 (BCJ2 等) は未対応です".to_string());
    }
    for coder in &folder.coders {
        if coder.num_in != 1 || coder.num_out != 1 {
            return Err("7Z の複雑なコーダー構成は未対応です".to_string());
        }
    }

    // このフォルダーのパックストリームを読み込む
    let (pack_start, _) = streams.folder_pack_range(folder_index);
    let pack_offset: u64 = streams.pack_pos
        + streams
            .pack_sizes
            .iter()
            .take(pack_start)
            .copied()
            .sum::<u64>();
    let pack_size = streams
        .pack_sizes
        .get(pack_start)
        .copied()
        .ok_or_else(|| "7Z パックストリーム索引が不正です".to_string())?;

    reader
        .seek(SeekFrom::Start(pack_offset))
        .map_err(|e| format!("7Z シークエラー: {}", e))?;
    let mut packed = vec![0u8; pack_size as usize];
    reader
        .read_exact(&mut packed)
        .map_err(|e| format!("7Z パックデータ読み込みエラー: {}", e))?;

    // コーダーチェーンを辿って復号する（1 入力 1 出力の線形チェーンのみ対応）
    let start = *folder
        .packed_indices
        .first()
        .ok_or_else(|| "7Z パック索引がありません".to_string())? as usize;
    if start >= folder.coders.len() {
        return Err("7Z パック索引が範囲外です".to_string());
    }

    let mut data = decode_coder(
        &folder.coders[start],
        packed,
        folder.unpack_sizes.get(start).copied(),
    )?;
    let mut current = start;
    for _ in 0..folder.coders.len() {
        match folder
            .bind_pairs
            .iter()
            .find(|&&(_, out)| out as usize == current)
        {
            Some(&(in_index, _)) => {
                let next = in_index as usize;
                if next >= folder.coders.len() {
                    return Err("7Z バインドペアが範囲外です".to_string());
                }
                data = decode_coder(
                    &folder.coders[next],
                    data,
                    folder.unpack_sizes.get(next).copied(),
                )?;
                current = next;
            }
            None => break,
        }
    }

    if let Some(expected) = folder.crc {
        let actual = Crc32::compute(&data);
        if actual != expected {
            return Err(format!(
                "7Z フォルダー CRC-32 不一致: 期待 {:08X} 実際 {:08X}",
                expected, actual
            ));
        }
    }

    Ok(data)
}

/// 1 コーダー分の復号を行う
fn decode_coder(coder: &Coder, input: Vec<u8>, out_size: Option<u64>) -> Result<Vec<u8>, String> {
    let output = match coder.id.as_slice() {
        // Copy
        [0x00] => input,
        // LZMA2
        [0x21] => {
            let expected =
                out_size.ok_or_else(|| "7Z LZMA2 の解凍後サイズが不明です".to_string())?;
            lzma_compat::decode_lzma2(&input, expected)
                .map_err(|e| format!("7Z LZMA2 解凍エラー: {}", e))?
        }
        // LZMA
        [0x03, 0x01, 0x01] => {
            if coder.props.is_empty() {
                return Err("7Z LZMA プロパティが不正です".to_string());
            }
            let expected =
                out_size.ok_or_else(|| "7Z LZMA の解凍後サイズが不明です".to_string())?;
            lzma_compat::decode_lzma1(&input, coder.props[0], expected)
                .map_err(|e| format!("7Z LZMA 解凍エラー: {}", e))?
        }
        // Delta フィルター
        [0x03] => {
            let distance = coder.props.first().map(|&d| d as usize + 1).unwrap_or(1);
            let mut data = input;
            for i in distance..data.len() {
                data[i] = data[i].wrapping_add(data[i - distance]);
            }
            data
        }
        // BCJ (x86) フィルター
        [0x03, 0x03, 0x01, 0x03] => {
            let mut data = input;
            bcj_x86_decode(&mut data);
            data
        }
        // Deflate
        [0x04, 0x01, 0x08] => {
            oxiarc_deflate::inflate(&input).map_err(|e| format!("7Z Deflate 解凍エラー: {}", e))?
        }
        // BZip2 (自前実装。oxiarc-bzip2 0.3.3 は実 libbz2 ストリームと非互換)
        [0x04, 0x02, 0x02] => super::bzip2_compat::decompress(&input)
            .map_err(|e| format!("7Z BZip2 解凍エラー: {}", e))?,
        // AES-256 (暗号化)
        [0x06, 0xF1, 0x07, 0x01] => {
            return Err("パスワード付き 7Z アーカイブは未対応です".to_string());
        }
        other => {
            let id_hex: String = other.iter().map(|b| format!("{:02X}", b)).collect();
            return Err(format!("7Z の未対応コーデックです: {}", id_hex));
        }
    };

    if let Some(expected) = out_size {
        if output.len() as u64 != expected {
            return Err(format!(
                "7Z 解凍サイズの不一致: 期待 {} 実際 {}",
                expected,
                output.len()
            ));
        }
    }

    Ok(output)
}

/// BCJ (x86) フィルターの復号（xz / 7-Zip の Bra86 と同一アルゴリズム）
fn bcj_x86_decode(buf: &mut [u8]) {
    const MASK_TO_ALLOWED_STATUS: [bool; 8] = [true, true, true, false, true, false, false, false];
    const MASK_TO_BIT_NUMBER: [u32; 8] = [0, 1, 2, 2, 3, 3, 3, 3];

    #[inline(always)]
    fn test_ms_byte(b: u8) -> bool {
        b == 0x00 || b == 0xFF
    }

    let len = buf.len();
    if len < 5 {
        return;
    }
    let end = len - 5;
    let ip: u32 = 5; // ストリーム先頭からの命令ポインタ補正
    let mut prev_pos: isize = -1;
    let mut prev_mask: u32 = 0;
    let mut i: usize = 0;

    while i <= end {
        let b = buf[i];
        if b != 0xE8 && b != 0xE9 {
            i += 1;
            continue;
        }
        prev_pos = i as isize - prev_pos;
        if (prev_pos & !3) != 0 {
            prev_mask = 0;
        } else {
            prev_mask = (prev_mask << (prev_pos - 1)) & 7;
            if prev_mask != 0 {
                let bit_number = MASK_TO_BIT_NUMBER[prev_mask as usize] as usize;
                if !MASK_TO_ALLOWED_STATUS[prev_mask as usize]
                    || test_ms_byte(buf[i + 4 - bit_number])
                {
                    prev_pos = i as isize;
                    prev_mask = ((prev_mask << 1) & 7) | 1;
                    i += 1;
                    continue;
                }
            }
        }
        prev_pos = i as isize;

        if test_ms_byte(buf[i + 4]) {
            let mut src = u32::from_le_bytes([buf[i + 1], buf[i + 2], buf[i + 3], buf[i + 4]]);
            let mut dest: u32;
            loop {
                dest = src.wrapping_sub(ip.wrapping_add(i as u32));
                if prev_mask == 0 {
                    break;
                }
                let index = MASK_TO_BIT_NUMBER[prev_mask as usize] * 8;
                let check = ((dest >> (24 - index)) & 0xFF) as u8;
                if !test_ms_byte(check) {
                    break;
                }
                src = dest ^ ((1u32 << (32 - index)).wrapping_sub(1));
            }
            buf[i + 1] = dest as u8;
            buf[i + 2] = (dest >> 8) as u8;
            buf[i + 3] = (dest >> 16) as u8;
            buf[i + 4] = if dest & 0x0100_0000 != 0 { 0xFF } else { 0x00 };
            i += 5;
        } else {
            prev_mask = ((prev_mask << 1) & 7) | 1;
            i += 1;
        }
    }
}
