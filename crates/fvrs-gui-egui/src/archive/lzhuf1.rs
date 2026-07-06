//! `-lh1-`（LZHUF: 4KB LZSS + 適応型ハフマン符号）コーデック
//!
//! oxiarc-lzhuf 0.3.3 は lh0/lh4〜lh7 のみ対応で `-lh1-` を復号できないため、
//! LHarc 1.x 世代の日本語アーカイブ互換のために FVRS 独自実装を提供する。
//! アルゴリズムは Okumura/Yoshizaki の古典 LZHUF.C に忠実に従う。
//!
//! 本モジュールは `archive` の内部実装であり、`pub` なのは統合テストから
//! フィクスチャ生成関数を利用するためである（`#[doc(hidden)]`）。

/// リングバッファサイズ（lh1 は 4KB ウィンドウ）
const RING_SIZE: usize = 4096;
/// 最長一致長
const MAX_MATCH: usize = 60;
/// 一致とみなす最小長のしきい値
const THRESHOLD: usize = 2;
/// 文字コード数（リテラル 256 + 一致長コード）
const NUM_CHAR: usize = 256 - THRESHOLD + MAX_MATCH; // 314
/// ハフマン木のノード総数
const TABLE_SIZE: usize = NUM_CHAR * 2 - 1; // 627
/// ルートノードの位置
const ROOT: usize = TABLE_SIZE - 1; // 626
/// 頻度の上限（到達したら木を再構築）
const MAX_FREQ: u32 = 0x8000;

/// MSB ファーストのビットリーダー（データ末尾以降は 0 を返す = LZHUF のパディング仕様）
struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_pos: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_pos: 0,
        }
    }

    fn get_bit(&mut self) -> u32 {
        let bit = match self.data.get(self.byte_pos) {
            Some(&byte) => u32::from((byte >> (7 - self.bit_pos)) & 1),
            None => 0,
        };
        self.bit_pos += 1;
        if self.bit_pos == 8 {
            self.bit_pos = 0;
            self.byte_pos += 1;
        }
        bit
    }

    fn get_bits(&mut self, count: u32) -> u32 {
        let mut value = 0;
        for _ in 0..count {
            value = (value << 1) | self.get_bit();
        }
        value
    }
}

/// MSB ファーストのビットライター
///
/// テストフィクスチャ生成 (`encode_lh1_literals`) 専用のためライブラリ
/// ターゲットでのみ参照される（バイナリターゲットでは未使用）。
#[allow(dead_code)]
struct BitWriter {
    out: Vec<u8>,
    current: u8,
    filled: u8,
}

#[allow(dead_code)]
impl BitWriter {
    fn new() -> Self {
        Self {
            out: Vec::new(),
            current: 0,
            filled: 0,
        }
    }

    fn put_bit(&mut self, bit: u32) {
        self.current = (self.current << 1) | (bit & 1) as u8;
        self.filled += 1;
        if self.filled == 8 {
            self.out.push(self.current);
            self.current = 0;
            self.filled = 0;
        }
    }

    /// `code` の上位 `len` ビット（bit15 から）を出力する（LZHUF の Putcode 相当）
    fn put_code(&mut self, len: u32, code: u16) {
        for i in 0..len {
            self.put_bit(u32::from((code >> (15 - i)) & 1));
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.filled > 0 {
            self.current <<= 8 - self.filled;
            self.out.push(self.current);
        }
        self.out
    }
}

/// LZHUF の適応型ハフマン木
struct AdaptiveHuffman {
    freq: Vec<u32>,
    /// 親ノード（葉は `TABLE_SIZE + シンボル値` の位置に格納）
    prnt: Vec<usize>,
    son: Vec<usize>,
}

impl AdaptiveHuffman {
    fn new() -> Self {
        let mut freq = vec![0u32; TABLE_SIZE + 1];
        let mut prnt = vec![0usize; TABLE_SIZE + NUM_CHAR];
        let mut son = vec![0usize; TABLE_SIZE];

        for i in 0..NUM_CHAR {
            freq[i] = 1;
            son[i] = i + TABLE_SIZE;
            prnt[i + TABLE_SIZE] = i;
        }
        let mut i = 0;
        let mut j = NUM_CHAR;
        while j <= ROOT {
            freq[j] = freq[i] + freq[i + 1];
            son[j] = i;
            prnt[i] = j;
            prnt[i + 1] = j;
            i += 2;
            j += 1;
        }
        freq[TABLE_SIZE] = u32::MAX; // 番兵
        prnt[ROOT] = 0;

        Self { freq, prnt, son }
    }

    /// 木の再構築（頻度が MAX_FREQ に達した際に全頻度を半減する）
    fn reconst(&mut self) {
        // 葉ノードを前半に集め、頻度を (freq + 1) / 2 に半減する
        let mut j = 0;
        for i in 0..TABLE_SIZE {
            if self.son[i] >= TABLE_SIZE {
                self.freq[j] = self.freq[i].div_ceil(2);
                self.son[j] = self.son[i];
                j += 1;
            }
        }
        // 子を結合しながら内部ノードを昇順位置に挿入して木を構築する
        let mut i = 0;
        let mut j = NUM_CHAR;
        while j < TABLE_SIZE {
            let f = self.freq[i] + self.freq[i + 1];
            self.freq[j] = f;
            let mut k = j - 1;
            while k > 0 && f < self.freq[k] {
                k -= 1;
            }
            if f < self.freq[k] {
                // 到達しない防御的分岐（freq は昇順のため k=0 でも f >= freq[0]）
                k = 0;
            } else {
                k += 1;
            }
            self.freq.copy_within(k..j, k + 1);
            self.freq[k] = f;
            self.son.copy_within(k..j, k + 1);
            self.son[k] = i;
            i += 2;
            j += 1;
        }
        // 親リンクを張り直す
        for i in 0..TABLE_SIZE {
            let k = self.son[i];
            self.prnt[k] = i;
            if k < TABLE_SIZE {
                self.prnt[k + 1] = i;
            }
        }
    }

    /// シンボル `symbol` の頻度を加算し、順序が崩れたノードを交換する
    fn update(&mut self, symbol: usize) {
        if self.freq[ROOT] == MAX_FREQ {
            self.reconst();
        }
        let mut c = self.prnt[symbol + TABLE_SIZE];
        loop {
            self.freq[c] += 1;
            let k = self.freq[c];

            // 順序が崩れたらノードを交換する
            let mut l = c + 1;
            if k > self.freq[l] {
                while k > self.freq[l + 1] {
                    l += 1;
                }
                self.freq[c] = self.freq[l];
                self.freq[l] = k;

                let i = self.son[c];
                self.prnt[i] = l;
                if i < TABLE_SIZE {
                    self.prnt[i + 1] = l;
                }

                let j = self.son[l];
                self.son[l] = i;
                self.prnt[j] = c;
                if j < TABLE_SIZE {
                    self.prnt[j + 1] = c;
                }
                self.son[c] = j;

                c = l;
            }

            c = self.prnt[c];
            if c == 0 {
                break; // ルートまで更新完了
            }
        }
    }

    /// 1 シンボルを復号する
    fn decode_char(&mut self, reader: &mut BitReader<'_>) -> usize {
        let mut c = self.son[ROOT];
        // ルートから葉へ: ビット 0 なら小さい子、1 なら大きい子を辿る
        while c < TABLE_SIZE {
            c += reader.get_bit() as usize;
            c = self.son[c];
        }
        let symbol = c - TABLE_SIZE;
        self.update(symbol);
        symbol
    }

    /// 1 シンボルを符号化する
    /// （テストフィクスチャ生成専用。バイナリターゲットでは未使用）
    #[allow(dead_code)]
    fn encode_char(&mut self, writer: &mut BitWriter, symbol: usize) {
        let mut code: u16 = 0;
        let mut len: u32 = 0;
        let mut k = self.prnt[symbol + TABLE_SIZE];
        // 葉からルートへ遡りながら逆順にビットを積む
        loop {
            code >>= 1;
            if k & 1 == 1 {
                code |= 0x8000;
            }
            len += 1;
            k = self.prnt[k];
            if k == ROOT {
                break;
            }
        }
        writer.put_code(len, code);
        self.update(symbol);
    }
}

/// 位置符号の上位 6 ビット用静的テーブル（LZHUF.C の d_code / d_len 相当）
///
/// 符号長列 {3×1, 4×3, 5×8, 6×12, 7×24, 8×16} の正準ハフマン符号から生成する。
fn build_position_tables() -> ([u8; 256], [u8; 256]) {
    const LEN_COUNTS: [(u8, usize); 6] = [(3, 1), (4, 3), (5, 8), (6, 12), (7, 24), (8, 16)];

    let mut p_len = [0u8; 64];
    let mut idx = 0;
    for (len, count) in LEN_COUNTS {
        for _ in 0..count {
            p_len[idx] = len;
            idx += 1;
        }
    }

    // 左詰め 8 ビットの正準符号を割り当てる
    let mut p_code = [0u8; 64];
    let mut code: u32 = 0;
    let mut prev_len = p_len[0];
    for i in 0..64 {
        code <<= p_len[i] - prev_len;
        prev_len = p_len[i];
        p_code[i] = ((code << (8 - p_len[i])) & 0xFF) as u8;
        code += 1;
    }

    let mut d_code = [0u8; 256];
    let mut d_len = [0u8; 256];
    for j in 0..64 {
        let len = p_len[j] as u32;
        let prefix = (p_code[j] as u32) >> (8 - len);
        for (i, (dc, dl)) in d_code.iter_mut().zip(d_len.iter_mut()).enumerate() {
            if (i as u32) >> (8 - len) == prefix {
                *dc = j as u8;
                *dl = p_len[j];
            }
        }
    }

    (d_code, d_len)
}

/// 一致位置を復号する（上位 6 ビットは静的ハフマン、下位 6 ビットは生値）
fn decode_position(reader: &mut BitReader<'_>, d_code: &[u8; 256], d_len: &[u8; 256]) -> usize {
    let mut i = reader.get_bits(8) as usize;
    let c = (d_code[i] as usize) << 6;
    let extra = u32::from(d_len[i]) - 2;
    for _ in 0..extra {
        i = (i << 1) + reader.get_bit() as usize;
    }
    c | (i & 0x3F)
}

/// `-lh1-` 圧縮データを復号する
///
/// # Errors
/// 復号結果が `original_size` に満たない・超過する等の異常時にエラー文字列を返す。
pub fn decode_lh1(data: &[u8], original_size: u64) -> Result<Vec<u8>, String> {
    let expected =
        usize::try_from(original_size).map_err(|_| "lh1: 元サイズが大きすぎます".to_string())?;

    let mut tree = AdaptiveHuffman::new();
    let mut reader = BitReader::new(data);
    let (d_code, d_len) = build_position_tables();

    // リングバッファは空白 (0x20) で初期化するのが LZHUF の仕様
    let mut ring = [0x20u8; RING_SIZE];
    let mut r = RING_SIZE - MAX_MATCH;
    let mut out = Vec::with_capacity(expected.min(16 * 1024 * 1024));

    while out.len() < expected {
        let c = tree.decode_char(&mut reader);
        if c < 256 {
            out.push(c as u8);
            ring[r] = c as u8;
            r = (r + 1) & (RING_SIZE - 1);
        } else {
            let pos = decode_position(&mut reader, &d_code, &d_len);
            let start = (r + RING_SIZE - pos - 1) & (RING_SIZE - 1);
            let length = c - 255 + THRESHOLD;
            for k in 0..length {
                let byte = ring[(start + k) & (RING_SIZE - 1)];
                out.push(byte);
                ring[r] = byte;
                r = (r + 1) & (RING_SIZE - 1);
                if out.len() >= expected {
                    break;
                }
            }
        }
    }

    Ok(out)
}

/// テストフィクスチャ生成用: 全バイトをリテラルとして符号化した正当な lh1 ストリームを返す
///
/// 一致（コピー）符号を使わないため圧縮率は悪いが、lh1 デコーダーで
/// 完全に復号できる正規のビットストリームである。
#[doc(hidden)]
#[allow(dead_code)] // ライブラリターゲットの統合テストから使用（バイナリでは未使用）
pub fn encode_lh1_literals(data: &[u8]) -> Vec<u8> {
    let mut tree = AdaptiveHuffman::new();
    let mut writer = BitWriter::new();
    for &byte in data {
        tree.encode_char(&mut writer, byte as usize);
    }
    writer.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_data(len: usize) -> Vec<u8> {
        let mut state: u32 = 0x1234_5678;
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

    #[test]
    fn position_tables_cover_all_prefixes() {
        let (_, d_len) = build_position_tables();
        assert!(
            d_len.iter().all(|&l| (3..=8).contains(&l)),
            "位置テーブルに未割り当てのプレフィックスがあります"
        );
    }

    #[test]
    fn literal_roundtrip_small() -> Result<(), String> {
        let original = "こんにちは、lh1 の世界！Hello LZHUF.".as_bytes();
        let encoded = encode_lh1_literals(original);
        let decoded = decode_lh1(&encoded, original.len() as u64)?;
        if decoded != original {
            return Err("lh1 リテラル復号の結果が一致しません".to_string());
        }
        Ok(())
    }

    #[test]
    fn literal_roundtrip_large_exercises_reconst() -> Result<(), String> {
        // MAX_FREQ (0x8000) を超えるシンボル数で reconst() を通す
        let original = make_data(40 * 1024);
        let encoded = encode_lh1_literals(&original);
        let decoded = decode_lh1(&encoded, original.len() as u64)?;
        if decoded != original {
            return Err("lh1 大容量リテラル復号の結果が一致しません".to_string());
        }
        Ok(())
    }

    #[test]
    fn empty_input_decodes_to_empty() -> Result<(), String> {
        let decoded = decode_lh1(&[], 0)?;
        if !decoded.is_empty() {
            return Err("空データの復号結果が空ではありません".to_string());
        }
        Ok(())
    }
}
