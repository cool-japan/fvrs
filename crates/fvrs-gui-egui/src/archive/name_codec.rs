//! アーカイブ内ファイル名のエンコーディング変換ユーティリティ
//!
//! oxiarc-archive 0.3.3 は ZIP の生ファイル名バイト列を `String::from_utf8_lossy`
//! で復号するため、Shift_JIS 名（日本語 Windows 製 ZIP の標準）が U+FFFD へ
//! 潰れて別名エントリが衝突する。本モジュールは生バイト列から
//! UTF-8 / Shift_JIS / CP437 を判別して復号し、名前の一意性を保つ。

use encoding_rs::SHIFT_JIS;

/// CP437（IBM PC 標準コードページ）の 0x80..=0xFF 領域の対応表
///
/// 旧 `zip` クレートと同じ単射な復号を行うためのフォールバックに使用する。
const CP437_HIGH: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å', //
    'É', 'æ', 'Æ', 'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ', //
    'á', 'í', 'ó', 'ú', 'ñ', 'Ñ', 'ª', 'º', '¿', '⌐', '¬', '½', '¼', '¡', '«', '»', //
    '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕', '╣', '║', '╗', '╝', '╜', '╛', '┐', //
    '└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦', '╠', '═', '╬', '╧', //
    '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐', '▀', //
    'α', 'ß', 'Γ', 'π', 'Σ', 'σ', 'µ', 'τ', 'Φ', 'Θ', 'Ω', 'δ', '∞', 'φ', 'ε', '∩', //
    '≡', '±', '≥', '≤', '⌠', '⌡', '÷', '≈', '°', '∙', '·', '√', 'ⁿ', '²', '■', '\u{00A0}',
];

/// CP437 として復号（単射: 異なるバイト列は必ず異なる文字列になる）
fn decode_cp437(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| {
            if b < 0x80 {
                b as char
            } else {
                CP437_HIGH[(b - 0x80) as usize]
            }
        })
        .collect()
}

/// ZIP エントリ名の復号
///
/// - EFS フラグ（汎用フラグ bit 11）が立っていれば UTF-8
/// - それ以外は UTF-8 として妥当なら UTF-8（Linux/macOS 製 ZIP の一般的ケース）
/// - 次に Shift_JIS（日本語 Windows 製 ZIP の標準）
/// - 最後に CP437（旧 `zip` クレートと同じ単射フォールバック）
pub(super) fn decode_zip_name(bytes: &[u8], utf8_flag: bool) -> String {
    if utf8_flag {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    if let Ok(name) = std::str::from_utf8(bytes) {
        return name.to_string();
    }
    let (decoded, _, had_errors) = SHIFT_JIS.decode(bytes);
    if !had_errors {
        return decoded.into_owned();
    }
    decode_cp437(bytes)
}

/// LZH エントリ名の復号（LZH の慣習に従い Shift_JIS を優先）
pub(super) fn decode_lzh_name(bytes: &[u8]) -> String {
    if bytes.is_ascii() {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    let (decoded, _, had_errors) = SHIFT_JIS.decode(bytes);
    if !had_errors {
        return decoded.into_owned();
    }
    if let Ok(name) = std::str::from_utf8(bytes) {
        return name.to_string();
    }
    String::from_utf8_lossy(bytes).into_owned()
}

/// LZH エントリ名のエンコード
///
/// LZH の標準である Shift_JIS で符号化する（レガシー日本語ツールとの互換性のため）。
/// Shift_JIS に写像できない文字を含む場合のみ UTF-8 バイト列にフォールバックする。
pub(super) fn encode_lzh_name(name: &str) -> Vec<u8> {
    let (encoded, _, had_errors) = SHIFT_JIS.encode(name);
    if had_errors {
        name.as_bytes().to_vec()
    } else {
        encoded.into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_names_shift_jis_and_utf8_stay_distinct() {
        // "あ.txt" / "い.txt" の Shift_JIS 表現
        let a = [0x82, 0xA0, b'.', b't', b'x', b't'];
        let i = [0x82, 0xA2, b'.', b't', b'x', b't'];
        assert_eq!(decode_zip_name(&a, false), "あ.txt");
        assert_eq!(decode_zip_name(&i, false), "い.txt");

        // EFS フラグ付き UTF-8
        assert_eq!(decode_zip_name("日本語.txt".as_bytes(), true), "日本語.txt");
        // フラグ無し UTF-8（Linux 製 ZIP）
        assert_eq!(
            decode_zip_name("日本語.txt".as_bytes(), false),
            "日本語.txt"
        );
    }

    #[test]
    fn zip_names_fall_back_to_cp437_injectively() {
        // UTF-8 でも Shift_JIS でも不正なバイト列 (0x81 は SJIS 先行バイトだが後続が不正)
        let x = [0x81, 0x20, 0x41];
        let y = [0x82, 0x20, 0x41];
        let dx = decode_zip_name(&x, false);
        let dy = decode_zip_name(&y, false);
        assert_ne!(dx, dy, "CP437 フォールバックが単射になっていません");
        assert!(!dx.contains('\u{FFFD}'));
    }

    #[test]
    fn lzh_name_roundtrip_shift_jis() {
        let encoded = encode_lzh_name("日本語ファイル.txt");
        assert_eq!(decode_lzh_name(&encoded), "日本語ファイル.txt");
    }
}
