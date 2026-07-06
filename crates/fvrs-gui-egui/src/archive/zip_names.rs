//! ZIP セントラルディレクトリから生のエントリ名バイト列を回収するモジュール
//!
//! oxiarc-archive 0.3.3 の `ZipReader` はエントリ名を `String::from_utf8_lossy`
//! で復号するため、Shift_JIS 名（EFS フラグ無しの日本語 Windows 製 ZIP）が
//! U+FFFD に潰れ、異なる名前が衝突して解凍時にファイルが上書き消失する。
//! 本モジュールはセントラルディレクトリを自前で走査して生バイト列と
//! EFS フラグを取得し、`name_codec` の判別復号でエントリ名を復元する。
//! 上流 oxiarc が生バイト列を公開したら本モジュールは削除できる。

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use oxiarc_core::Entry;

use super::name_codec;

/// End of central directory record のシグネチャ
const EOCD_SIG: [u8; 4] = [0x50, 0x4B, 0x05, 0x06];
/// Zip64 end of central directory locator のシグネチャ
const ZIP64_LOCATOR_SIG: [u8; 4] = [0x50, 0x4B, 0x06, 0x07];
/// Central directory file header のシグネチャ
const CD_HEADER_SIG: [u8; 4] = [0x50, 0x4B, 0x01, 0x02];
/// 汎用フラグの EFS ビット（bit 11: 名前・コメントが UTF-8）
const FLAG_EFS: u16 = 1 << 11;

/// セントラルディレクトリから (生ファイル名, EFS フラグ) の一覧を読み取る
fn read_raw_names(path: &Path) -> Result<Vec<(Vec<u8>, bool)>, String> {
    let mut file = File::open(path).map_err(|e| format!("ファイルオープンエラー: {}", e))?;
    let file_size = file
        .seek(SeekFrom::End(0))
        .map_err(|e| format!("シークエラー: {}", e))?;

    // EOCD をファイル末尾から探索（コメント最大 65535 バイト + EOCD 22 バイト）
    let search_start = file_size.saturating_sub(65_535 + 22);
    file.seek(SeekFrom::Start(search_start))
        .map_err(|e| format!("シークエラー: {}", e))?;
    let mut tail = vec![0u8; (file_size - search_start) as usize];
    file.read_exact(&mut tail)
        .map_err(|e| format!("読み込みエラー: {}", e))?;

    let eocd_offset = tail
        .windows(4)
        .rposition(|w| w == EOCD_SIG)
        .ok_or_else(|| "EOCD が見つかりません".to_string())?;
    let eocd = &tail[eocd_offset..];
    if eocd.len() < 22 {
        return Err("EOCD が短すぎます".to_string());
    }

    let mut total_entries = u16::from_le_bytes([eocd[10], eocd[11]]) as u64;
    let mut cd_offset = u32::from_le_bytes([eocd[16], eocd[17], eocd[18], eocd[19]]) as u64;

    // Zip64: EOCD の直前に locator があれば Zip64 EOCD を参照する
    let eocd_pos = search_start + eocd_offset as u64;
    if eocd_pos >= 20 {
        file.seek(SeekFrom::Start(eocd_pos - 20))
            .map_err(|e| format!("シークエラー: {}", e))?;
        let mut locator = [0u8; 20];
        if file.read_exact(&mut locator).is_ok() && locator[0..4] == ZIP64_LOCATOR_SIG {
            let zip64_eocd_offset = u64::from_le_bytes([
                locator[8], locator[9], locator[10], locator[11], //
                locator[12], locator[13], locator[14], locator[15],
            ]);
            file.seek(SeekFrom::Start(zip64_eocd_offset))
                .map_err(|e| format!("シークエラー: {}", e))?;
            let mut zip64_eocd = [0u8; 56];
            file.read_exact(&mut zip64_eocd)
                .map_err(|e| format!("Zip64 EOCD 読み込みエラー: {}", e))?;
            total_entries = u64::from_le_bytes([
                zip64_eocd[32], zip64_eocd[33], zip64_eocd[34], zip64_eocd[35], //
                zip64_eocd[36], zip64_eocd[37], zip64_eocd[38], zip64_eocd[39],
            ]);
            cd_offset = u64::from_le_bytes([
                zip64_eocd[48], zip64_eocd[49], zip64_eocd[50], zip64_eocd[51], //
                zip64_eocd[52], zip64_eocd[53], zip64_eocd[54], zip64_eocd[55],
            ]);
        }
    }

    // セントラルディレクトリを順に走査
    file.seek(SeekFrom::Start(cd_offset))
        .map_err(|e| format!("シークエラー: {}", e))?;
    let mut names = Vec::new();
    for _ in 0..total_entries {
        let mut header = [0u8; 46];
        file.read_exact(&mut header)
            .map_err(|e| format!("セントラルディレクトリ読み込みエラー: {}", e))?;
        if header[0..4] != CD_HEADER_SIG {
            return Err("セントラルディレクトリのシグネチャが不正です".to_string());
        }

        let flags = u16::from_le_bytes([header[8], header[9]]);
        let filename_len = u16::from_le_bytes([header[28], header[29]]) as usize;
        let extra_len = u16::from_le_bytes([header[30], header[31]]) as u64;
        let comment_len = u16::from_le_bytes([header[32], header[33]]) as u64;

        let mut filename = vec![0u8; filename_len];
        file.read_exact(&mut filename)
            .map_err(|e| format!("エントリ名読み込みエラー: {}", e))?;
        file.seek(SeekFrom::Current((extra_len + comment_len) as i64))
            .map_err(|e| format!("シークエラー: {}", e))?;

        names.push((filename, flags & FLAG_EFS != 0));
    }

    Ok(names)
}

/// oxiarc が lossy 復号したエントリ名を、生バイト列からの判別復号で置き換える
///
/// セントラルディレクトリが読めない場合や oxiarc のエントリと整合しない場合は
/// 何もしない（既存の名前を維持する）。抽出は `Entry::offset` ベースのため
/// 名前の置き換えは安全である。
pub(super) fn refine_entry_names(path: &Path, entries: &mut [Entry]) {
    let raw_names = match read_raw_names(path) {
        Ok(names) => names,
        Err(e) => {
            tracing::debug!("ZIP セントラルディレクトリの名前回収をスキップ: {}", e);
            return;
        }
    };

    if raw_names.len() != entries.len() {
        tracing::debug!(
            "ZIP エントリ数の不一致により名前回収をスキップ: {} != {}",
            raw_names.len(),
            entries.len()
        );
        return;
    }

    for (entry, (raw, utf8_flag)) in entries.iter_mut().zip(raw_names) {
        // oxiarc の名前は同じバイト列の lossy 復号のはず（不一致なら整列ずれとみなす）
        if String::from_utf8_lossy(&raw) == entry.name {
            entry.name = name_codec::decode_zip_name(&raw, utf8_flag);
        }
    }
}
