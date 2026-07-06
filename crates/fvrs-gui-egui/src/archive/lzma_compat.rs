//! 仕様準拠の LZMA1 / LZMA2 デコーダー
//!
//! oxiarc-lzma 0.3.3 のデコーダーは距離スロット 4〜13 の特殊確率テーブルの
//! 割り付けが仕様（LZMA SDK の `PosDecoders + dist - posSlot`）と異なり、
//! ブロックが重複する独自レイアウトになっている。適応確率が一致しないため
//! oxiarc 自身が符号化したストリームしか復号できず、実在の 7z アーカイブ
//! （liblzma / 7-Zip が生成する LZMA1 / LZMA2）の解凍が
//! 「Invalid LZMA data」で失敗する。
//!
//! 本モジュールは LZMA 仕様リファレンス実装 (LzmaSpec.cpp, Igor Pavlov,
//! パブリックドメイン) に忠実な復号器を提供する。上流 oxiarc が修正されたら
//! 削除できる。実在の liblzma / bsdtar 生成ストリームで検証済み。

/// 確率モデルの初期値 (2048 / 2)
const PROB_INIT: u16 = 1024;
/// 確率モデルのビット数
const NUM_BIT_MODEL_BITS: u32 = 11;
/// 適応速度
const NUM_MOVE_BITS: u32 = 5;
/// 距離スロットのうち特殊確率テーブルを使う上限
const END_POS_MODEL_INDEX: usize = 14;
/// 特殊確率テーブルの要素数 (1 + 128 - 14)
const NUM_SPEC_POS: usize = 115;
/// 一致長の最小値
const MATCH_LEN_MIN: u32 = 2;

/// LZMA プロパティ (lc / lp / pb)
#[derive(Clone, Copy)]
pub(super) struct LzmaProps {
    lc: u32,
    lp: u32,
    pb: u32,
}

impl LzmaProps {
    /// プロパティバイトから解析する
    pub(super) fn from_byte(byte: u8) -> Result<Self, String> {
        let mut d = u32::from(byte);
        if d >= 9 * 5 * 5 {
            return Err(format!("LZMA プロパティバイトが不正です: 0x{:02X}", byte));
        }
        let lc = d % 9;
        d /= 9;
        let lp = d % 5;
        let pb = d / 5;
        Ok(Self { lc, lp, pb })
    }
}

/// レンジデコーダー（入力終端以降は 0x00 を供給する寛容モード）
struct RangeDecoder<'a> {
    data: &'a [u8],
    pos: usize,
    range: u32,
    code: u32,
}

impl<'a> RangeDecoder<'a> {
    fn new(data: &'a [u8]) -> Result<Self, String> {
        if data.is_empty() {
            return Err("LZMA ストリームが空です".to_string());
        }
        // 先頭バイトは常に 0（仕様）だが寛容に読み飛ばす
        let mut decoder = Self {
            data,
            pos: 1,
            range: 0xFFFF_FFFF,
            code: 0,
        };
        for _ in 0..4 {
            decoder.code = (decoder.code << 8) | u32::from(decoder.next_byte());
        }
        Ok(decoder)
    }

    #[inline]
    fn next_byte(&mut self) -> u8 {
        let byte = self.data.get(self.pos).copied().unwrap_or(0);
        self.pos += 1;
        byte
    }

    #[inline]
    fn normalize(&mut self) {
        if self.range < (1 << 24) {
            self.range <<= 8;
            self.code = (self.code << 8) | u32::from(self.next_byte());
        }
    }

    #[inline]
    fn decode_bit(&mut self, prob: &mut u16) -> usize {
        let v = u32::from(*prob);
        let bound = (self.range >> NUM_BIT_MODEL_BITS) * v;
        let bit;
        if self.code < bound {
            *prob = (v + (((1 << NUM_BIT_MODEL_BITS) - v) >> NUM_MOVE_BITS)) as u16;
            self.range = bound;
            bit = 0;
        } else {
            *prob = (v - (v >> NUM_MOVE_BITS)) as u16;
            self.code -= bound;
            self.range -= bound;
            bit = 1;
        }
        self.normalize();
        bit
    }

    fn decode_direct_bits(&mut self, count: u32) -> u32 {
        let mut result = 0u32;
        for _ in 0..count {
            self.range >>= 1;
            self.code = self.code.wrapping_sub(self.range);
            let t = 0u32.wrapping_sub(self.code >> 31);
            self.code = self.code.wrapping_add(self.range & t);
            self.normalize();
            result = (result << 1).wrapping_add(t.wrapping_add(1));
        }
        result
    }

    fn decode_bit_tree(&mut self, probs: &mut [u16], num_bits: u32) -> u32 {
        let mut m = 1usize;
        for _ in 0..num_bits {
            m = (m << 1) | self.decode_bit(&mut probs[m]);
        }
        (m as u32) - (1 << num_bits)
    }

    fn decode_bit_tree_reverse(&mut self, probs: &mut [u16], num_bits: u32) -> u32 {
        let mut m = 1usize;
        let mut symbol = 0u32;
        for i in 0..num_bits {
            let bit = self.decode_bit(&mut probs[m]);
            m = (m << 1) | bit;
            symbol |= (bit as u32) << i;
        }
        symbol
    }
}

/// 一致長デコーダー
struct LenDecoder {
    choice: u16,
    choice2: u16,
    low: [[u16; 8]; 16],
    mid: [[u16; 8]; 16],
    high: [u16; 256],
}

impl LenDecoder {
    fn new() -> Self {
        Self {
            choice: PROB_INIT,
            choice2: PROB_INIT,
            low: [[PROB_INIT; 8]; 16],
            mid: [[PROB_INIT; 8]; 16],
            high: [PROB_INIT; 256],
        }
    }

    fn decode(&mut self, rc: &mut RangeDecoder<'_>, pos_state: usize) -> u32 {
        if rc.decode_bit(&mut self.choice) == 0 {
            MATCH_LEN_MIN + rc.decode_bit_tree(&mut self.low[pos_state], 3)
        } else if rc.decode_bit(&mut self.choice2) == 0 {
            MATCH_LEN_MIN + 8 + rc.decode_bit_tree(&mut self.mid[pos_state], 3)
        } else {
            MATCH_LEN_MIN + 16 + rc.decode_bit_tree(&mut self.high, 8)
        }
    }
}

/// チャンク間で持続する LZMA デコーダー状態（確率モデル・状態番号・rep 距離）
pub(super) struct LzmaDecoder {
    props: LzmaProps,
    lit_probs: Vec<u16>,
    is_match: [[u16; 16]; 12],
    is_rep: [u16; 12],
    is_rep_g0: [u16; 12],
    is_rep_g1: [u16; 12],
    is_rep_g2: [u16; 12],
    is_rep0_long: [[u16; 16]; 12],
    pos_slot: [[u16; 64]; 4],
    spec_pos: [u16; NUM_SPEC_POS],
    align_probs: [u16; 16],
    len_dec: LenDecoder,
    rep_len_dec: LenDecoder,
    state: usize,
    reps: [u32; 4],
}

impl LzmaDecoder {
    pub(super) fn new(props: LzmaProps) -> Self {
        let lit_size = 0x300usize << (props.lc + props.lp);
        Self {
            props,
            lit_probs: vec![PROB_INIT; lit_size],
            is_match: [[PROB_INIT; 16]; 12],
            is_rep: [PROB_INIT; 12],
            is_rep_g0: [PROB_INIT; 12],
            is_rep_g1: [PROB_INIT; 12],
            is_rep_g2: [PROB_INIT; 12],
            is_rep0_long: [[PROB_INIT; 16]; 12],
            pos_slot: [[PROB_INIT; 64]; 4],
            spec_pos: [PROB_INIT; NUM_SPEC_POS],
            align_probs: [PROB_INIT; 16],
            len_dec: LenDecoder::new(),
            rep_len_dec: LenDecoder::new(),
            state: 0,
            reps: [0; 4],
        }
    }

    /// 距離を復号する（仕様の `PosDecoders + dist - posSlot` レイアウトに準拠）
    fn decode_distance(&mut self, rc: &mut RangeDecoder<'_>, len: u32) -> u32 {
        let len_state = (len - MATCH_LEN_MIN).min(3) as usize;
        let slot = rc.decode_bit_tree(&mut self.pos_slot[len_state], 6) as usize;
        if slot < 4 {
            return slot as u32;
        }

        let num_direct_bits = ((slot >> 1) - 1) as u32;
        let mut dist = ((2 | (slot & 1)) as u32) << num_direct_bits;

        if slot < END_POS_MODEL_INDEX {
            let base = dist as usize - slot;
            dist += rc.decode_bit_tree_reverse(&mut self.spec_pos[base..], num_direct_bits);
        } else {
            dist = dist.wrapping_add(rc.decode_direct_bits(num_direct_bits - 4) << 4);
            dist = dist.wrapping_add(rc.decode_bit_tree_reverse(&mut self.align_probs, 4));
        }

        dist
    }

    /// 1 チャンク分を復号して `out` へ追記する
    ///
    /// - `dict_start`: 現在の辞書の開始位置（LZMA2 の辞書リセット境界）
    /// - `limit`: このチャンクで生成する非圧縮バイト数
    /// - `allow_end_marker`: LZMA1 のように終端マーカーを許容するか
    pub(super) fn decode_chunk(
        &mut self,
        data: &[u8],
        out: &mut Vec<u8>,
        dict_start: usize,
        limit: u64,
        allow_end_marker: bool,
    ) -> Result<(), String> {
        let mut rc = RangeDecoder::new(data)?;
        let target = (out.len() as u64)
            .checked_add(limit)
            .ok_or_else(|| "LZMA 出力サイズが大きすぎます".to_string())?;

        let pb_mask = (1usize << self.props.pb) - 1;
        let lp_mask = (1usize << self.props.lp) - 1;
        let lc = self.props.lc;

        while (out.len() as u64) < target {
            let processed = out.len() - dict_start;
            let pos_state = processed & pb_mask;

            if rc.decode_bit(&mut self.is_match[self.state][pos_state]) == 0 {
                // リテラル
                let prev_byte = if processed > 0 {
                    out[out.len() - 1] as usize
                } else {
                    0
                };
                let lit_state = ((processed & lp_mask) << lc) | (prev_byte >> (8 - lc as usize));
                let base = 0x300 * lit_state;
                let probs = &mut self.lit_probs[base..base + 0x300];

                let mut symbol = 1usize;
                if self.state >= 7 {
                    // 一致後リテラル: 直前一致のバイトと比較しながら復号する
                    let dist = self.reps[0] as usize;
                    if dist >= processed {
                        return Err("LZMA データが破損しています (一致後リテラル)".to_string());
                    }
                    let mut match_byte = out[out.len() - dist - 1];
                    while symbol < 0x100 {
                        let match_bit = ((match_byte >> 7) & 1) as usize;
                        match_byte <<= 1;
                        let bit = rc.decode_bit(&mut probs[((1 + match_bit) << 8) + symbol]);
                        symbol = (symbol << 1) | bit;
                        if match_bit != bit {
                            break;
                        }
                    }
                }
                while symbol < 0x100 {
                    symbol = (symbol << 1) | rc.decode_bit(&mut probs[symbol]);
                }

                out.push((symbol & 0xFF) as u8);
                self.state = if self.state < 4 {
                    0
                } else if self.state < 10 {
                    self.state - 3
                } else {
                    self.state - 6
                };
                continue;
            }

            // 一致（match / rep）
            let len;
            if rc.decode_bit(&mut self.is_rep[self.state]) == 0 {
                // 通常の一致
                self.reps[3] = self.reps[2];
                self.reps[2] = self.reps[1];
                self.reps[1] = self.reps[0];
                len = self.len_dec.decode(&mut rc, pos_state);
                self.state = if self.state < 7 { 7 } else { 10 };
                let dist = self.decode_distance(&mut rc, len);
                self.reps[0] = dist;
                if dist == 0xFFFF_FFFF {
                    // 終端マーカー
                    if allow_end_marker && (out.len() as u64) == target {
                        return Ok(());
                    }
                    if allow_end_marker {
                        return Err(format!(
                            "LZMA 終端マーカーが早すぎます: {} / {}",
                            out.len() as u64,
                            target
                        ));
                    }
                    return Err("LZMA2 チャンク内で予期しない終端マーカー".to_string());
                }
            } else if rc.decode_bit(&mut self.is_rep_g0[self.state]) == 0 {
                if rc.decode_bit(&mut self.is_rep0_long[self.state][pos_state]) == 0 {
                    // 長さ 1 のショート rep
                    self.state = if self.state < 7 { 9 } else { 11 };
                    let dist = self.reps[0] as usize;
                    if dist >= out.len() - dict_start {
                        return Err("LZMA データが破損しています (short rep)".to_string());
                    }
                    let byte = out[out.len() - dist - 1];
                    out.push(byte);
                    continue;
                }
                len = self.rep_len_dec.decode(&mut rc, pos_state);
                self.state = if self.state < 7 { 8 } else { 11 };
            } else {
                let dist;
                if rc.decode_bit(&mut self.is_rep_g1[self.state]) == 0 {
                    dist = self.reps[1];
                } else {
                    if rc.decode_bit(&mut self.is_rep_g2[self.state]) == 0 {
                        dist = self.reps[2];
                    } else {
                        dist = self.reps[3];
                        self.reps[3] = self.reps[2];
                    }
                    self.reps[2] = self.reps[1];
                }
                self.reps[1] = self.reps[0];
                self.reps[0] = dist;
                len = self.rep_len_dec.decode(&mut rc, pos_state);
                self.state = if self.state < 7 { 8 } else { 11 };
            }

            // 辞書からコピーする
            let dist = self.reps[0] as usize;
            if (dist as u64) >= (out.len() - dict_start) as u64 {
                return Err("LZMA データが破損しています (距離が範囲外)".to_string());
            }
            let mut copy_len = len as u64;
            if out.len() as u64 + copy_len > target {
                if allow_end_marker {
                    // LZMA1 は既知サイズで打ち切られる場合がある
                    copy_len = target - out.len() as u64;
                } else {
                    return Err("LZMA2 チャンクの出力サイズ超過".to_string());
                }
            }
            for _ in 0..copy_len {
                let byte = out[out.len() - dist - 1];
                out.push(byte);
            }
        }

        Ok(())
    }
}

/// 生の LZMA1 ストリームを復号する（7z の 030101 コーダー）
pub(super) fn decode_lzma1(input: &[u8], props_byte: u8, expected: u64) -> Result<Vec<u8>, String> {
    let props = LzmaProps::from_byte(props_byte)?;
    let mut decoder = LzmaDecoder::new(props);
    let mut out = Vec::with_capacity((expected as usize).min(16 * 1024 * 1024));
    decoder.decode_chunk(input, &mut out, 0, expected, true)?;
    Ok(out)
}

/// 生の LZMA2 ストリームを復号する（7z の 21 コーダー）
pub(super) fn decode_lzma2(input: &[u8], expected: u64) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity((expected as usize).min(16 * 1024 * 1024));
    let mut pos = 0usize;
    let mut dict_start = 0usize;
    let mut decoder: Option<LzmaDecoder> = None;
    let mut last_props: Option<LzmaProps> = None;

    let read = |pos: &mut usize, input: &[u8]| -> Result<u8, String> {
        let byte = *input
            .get(*pos)
            .ok_or_else(|| "LZMA2 ストリームが途中で終わっています".to_string())?;
        *pos += 1;
        Ok(byte)
    };

    // 終端バイト無し(入力の尽き)も寛容に扱う
    while let Some(&control) = input.get(pos) {
        pos += 1;

        if control == 0 {
            break;
        }

        if control < 0x80 {
            // 非圧縮チャンク (0x01: 辞書リセット, 0x02: 継続)
            if control > 2 {
                return Err(format!("LZMA2 制御バイトが不正です: 0x{:02X}", control));
            }
            let size = ((usize::from(read(&mut pos, input)?)) << 8
                | usize::from(read(&mut pos, input)?))
                + 1;
            if control == 1 {
                dict_start = out.len();
            }
            let end = pos
                .checked_add(size)
                .filter(|&end| end <= input.len())
                .ok_or_else(|| "LZMA2 非圧縮チャンクが範囲外です".to_string())?;
            out.extend_from_slice(&input[pos..end]);
            pos = end;
            // 非圧縮チャンク後の LZMA チャンクは状態リセットが必須
            decoder = None;
            continue;
        }

        // LZMA チャンク
        let unpacked = ((usize::from(control & 0x1F)) << 16
            | usize::from(read(&mut pos, input)?) << 8
            | usize::from(read(&mut pos, input)?))
            + 1;
        let packed =
            ((usize::from(read(&mut pos, input)?)) << 8 | usize::from(read(&mut pos, input)?)) + 1;
        let reset = (control >> 5) & 0x3;

        if reset >= 2 {
            last_props = Some(LzmaProps::from_byte(read(&mut pos, input)?)?);
        }
        if reset == 3 {
            dict_start = out.len();
        }
        if reset >= 1 {
            let props = last_props.ok_or_else(|| "LZMA2 プロパティが未指定です".to_string())?;
            decoder = Some(LzmaDecoder::new(props));
        }

        let end = pos
            .checked_add(packed)
            .filter(|&end| end <= input.len())
            .ok_or_else(|| "LZMA2 チャンクが範囲外です".to_string())?;
        let chunk = &input[pos..end];
        pos = end;

        let current = decoder
            .as_mut()
            .ok_or_else(|| "LZMA2 の状態リセットがありません".to_string())?;
        current.decode_chunk(chunk, &mut out, dict_start, unpacked as u64, false)?;
    }

    if out.len() as u64 != expected {
        return Err(format!(
            "LZMA2 解凍サイズの不一致: 期待 {} 実際 {}",
            expected,
            out.len()
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// liblzma (Python lzma, FORMAT_RAW / FILTER_LZMA1, lc=3 lp=0 pb=2) が生成した
    /// 実ストリーム。復号結果は「LZMA fixture: ...」×4 の 252 バイト。
    const LZMA1_PACKED: &[u8] = &[
        0x00, 0x26, 0x16, 0x85, 0xBC, 0x45, 0xF0, 0xEA, 0x70, 0xEC, 0x7A, 0x6E, //
        0x8F, 0xA4, 0x73, 0xFA, 0x7D, 0x40, 0x75, 0xD2, 0x5A, 0x4D, 0x7A, 0x23, //
        0x64, 0xBA, 0x67, 0x69, 0x63, 0x81, 0x42, 0x98, 0xDE, 0x62, 0x4E, 0x71, //
        0x75, 0x0E, 0x64, 0xB8, 0x31, 0x66, 0x6E, 0xD7, 0x89, 0x2C, 0x0C, 0x32, //
        0x4D, 0xA7, 0xD3, 0xE9, 0xDB, 0xF5, 0xDB, 0x28, 0x5B, 0x67, 0xB5, 0x57, //
        0x19, 0xA2, 0x15, 0x9F, 0xB1, 0x61, 0xFF, 0xF8, 0x7D, 0xEE, 0x00,
    ];

    /// liblzma (FORMAT_RAW / FILTER_LZMA2) が生成した実ストリーム（同じ内容）
    const LZMA2_PACKED: &[u8] = &[
        0xE0, 0x00, 0xFB, 0x00, 0x41, 0x5D, 0x00, 0x26, 0x16, 0x85, 0xBC, 0x45, //
        0xF0, 0xEA, 0x70, 0xEC, 0x7A, 0x6E, 0x8F, 0xA4, 0x73, 0xFA, 0x7D, 0x40, //
        0x75, 0xD2, 0x5A, 0x4D, 0x7A, 0x23, 0x64, 0xBA, 0x67, 0x69, 0x63, 0x81, //
        0x42, 0x98, 0xDE, 0x62, 0x4E, 0x71, 0x75, 0x0E, 0x64, 0xB8, 0x31, 0x66, //
        0x6E, 0xD7, 0x89, 0x2C, 0x0C, 0x32, 0x4D, 0xA7, 0xD3, 0xE9, 0xDB, 0xF5, //
        0xDB, 0x28, 0x5B, 0x67, 0xB5, 0x57, 0x19, 0xA2, 0x06, 0xFF, 0xC0, 0x00, //
        0x00,
    ];

    fn fixture_content() -> Vec<u8> {
        "LZMA fixture: 実データで検証する 7z コンテンツ。"
            .repeat(4)
            .into_bytes()
    }

    #[test]
    fn decodes_real_liblzma_lzma1_stream() -> Result<(), String> {
        let expected = fixture_content();
        let decoded = decode_lzma1(LZMA1_PACKED, 0x5D, expected.len() as u64)?;
        if decoded != expected {
            return Err("liblzma LZMA1 ストリームの復号結果が不一致です".to_string());
        }
        Ok(())
    }

    #[test]
    fn decodes_real_liblzma_lzma2_stream() -> Result<(), String> {
        let expected = fixture_content();
        let decoded = decode_lzma2(LZMA2_PACKED, expected.len() as u64)?;
        if decoded != expected {
            return Err("liblzma LZMA2 ストリームの復号結果が不一致です".to_string());
        }
        Ok(())
    }

    #[test]
    fn rejects_invalid_props_byte() {
        assert!(LzmaProps::from_byte(0xFF).is_err());
    }
}
