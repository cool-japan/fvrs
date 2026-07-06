//! 仕様準拠の bzip2 デコーダー / エンコーダー
//!
//! oxiarc-bzip2 0.3.3 はハフマンテーブルのアルファベットサイズが実フォーマットと
//! 1 ずれている等の非互換があり、自分が書いたストリームしか読めない
//! （実 libbz2 ストリームは「Invalid number of Huffman tables」で失敗し、
//! oxiarc が書いたストリームは libbz2 / bsdtar で読めない）。
//! 実在の .tar.bz2 / bzip2 コーデックの 7z を扱うため、本モジュールで
//! bzip2 フォーマット（BWT + MTF + RLE + ハフマン）を仕様どおり実装する。
//! 上流 oxiarc が修正されたら削除できる。libbz2 / Python bz2 との
//! 相互運用を実データで検証済み。

/// ブロックマジック (BCD の円周率)
const BLOCK_MAGIC: u64 = 0x3141_5926_5359;
/// ストリーム終端マジック (BCD の √π)
const EOS_MAGIC: u64 = 0x1772_4538_5090;
/// グループあたりのシンボル数
const GROUP_SIZE: usize = 50;
/// ハフマン符号長の上限（デコード時）
const MAX_CODE_LEN: usize = 23;
/// ハフマン符号長の上限（エンコード時、libbz2 と同じ）
const MAX_ENCODE_LEN: usize = 17;

// ---------------------------------------------------------------------------
// bzip2 専用 CRC-32（多項式 0x04C11DB7、非反転ビット順）
// ---------------------------------------------------------------------------

fn bz2_crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    for (i, entry) in table.iter_mut().enumerate() {
        let mut crc = (i as u32) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04C1_1DB7
            } else {
                crc << 1
            };
        }
        *entry = crc;
    }
    table
}

/// bzip2 のブロック CRC 計算器
struct Bz2Crc {
    table: [u32; 256],
    value: u32,
}

impl Bz2Crc {
    fn new() -> Self {
        Self {
            table: bz2_crc_table(),
            value: 0xFFFF_FFFF,
        }
    }

    fn reset(&mut self) {
        self.value = 0xFFFF_FFFF;
    }

    fn update(&mut self, data: &[u8]) {
        for &byte in data {
            self.value =
                (self.value << 8) ^ self.table[(((self.value >> 24) as u8) ^ byte) as usize];
        }
    }

    fn finish(&self) -> u32 {
        !self.value
    }
}

// ---------------------------------------------------------------------------
// MSB ファーストのビット I/O
// ---------------------------------------------------------------------------

struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_pos: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_pos: 0,
        }
    }

    fn read_bit(&mut self) -> Result<u32, String> {
        let byte = *self
            .data
            .get(self.byte_pos)
            .ok_or_else(|| "BZ2 ストリームが途中で終わっています".to_string())?;
        let bit = u32::from((byte >> (7 - self.bit_pos)) & 1);
        self.bit_pos += 1;
        if self.bit_pos == 8 {
            self.bit_pos = 0;
            self.byte_pos += 1;
        }
        Ok(bit)
    }

    fn read_bits(&mut self, count: u32) -> Result<u32, String> {
        let mut value = 0u32;
        for _ in 0..count {
            value = (value << 1) | self.read_bit()?;
        }
        Ok(value)
    }

    fn read_bits_u64(&mut self, count: u32) -> Result<u64, String> {
        let mut value = 0u64;
        for _ in 0..count {
            value = (value << 1) | u64::from(self.read_bit()?);
        }
        Ok(value)
    }
}

struct BitWriter {
    out: Vec<u8>,
    current: u8,
    filled: u32,
}

impl BitWriter {
    fn new() -> Self {
        Self {
            out: Vec::new(),
            current: 0,
            filled: 0,
        }
    }

    fn write_bit(&mut self, bit: u32) {
        self.current = (self.current << 1) | (bit & 1) as u8;
        self.filled += 1;
        if self.filled == 8 {
            self.out.push(self.current);
            self.current = 0;
            self.filled = 0;
        }
    }

    fn write_bits(&mut self, value: u32, count: u32) {
        for i in (0..count).rev() {
            self.write_bit((value >> i) & 1);
        }
    }

    fn write_bits_u64(&mut self, value: u64, count: u32) {
        for i in (0..count).rev() {
            self.write_bit(((value >> i) & 1) as u32);
        }
    }

    fn finish(mut self) -> Vec<u8> {
        while self.filled != 0 {
            self.write_bit(0);
        }
        self.out
    }
}

// ---------------------------------------------------------------------------
// デコーダー
// ---------------------------------------------------------------------------

/// 符号長からのカノニカルハフマン復号テーブル（libbz2 の limit/base/perm 方式）
struct HuffmanTable {
    limit: [i64; MAX_CODE_LEN + 2],
    base: [i64; MAX_CODE_LEN + 2],
    perm: Vec<u16>,
    min_len: usize,
}

impl HuffmanTable {
    fn from_lengths(lengths: &[u8]) -> Result<Self, String> {
        let min_len = lengths
            .iter()
            .copied()
            .min()
            .ok_or_else(|| "BZ2 ハフマン表が空です".to_string())? as usize;
        let max_len = lengths.iter().copied().max().unwrap_or(0) as usize;
        if min_len == 0 || max_len > MAX_CODE_LEN {
            return Err("BZ2 ハフマン符号長が不正です".to_string());
        }

        // シンボル順に長さ別へ振り分け（libbz2 hbCreateDecodeTables と同じ順序）
        let mut perm = Vec::with_capacity(lengths.len());
        for len in min_len..=max_len {
            for (symbol, &l) in lengths.iter().enumerate() {
                if l as usize == len {
                    perm.push(symbol as u16);
                }
            }
        }

        let mut count = [0i64; MAX_CODE_LEN + 2];
        for &l in lengths {
            count[l as usize + 1] += 1;
        }
        for i in 1..MAX_CODE_LEN + 2 {
            count[i] += count[i - 1];
        }

        let mut limit = [0i64; MAX_CODE_LEN + 2];
        let mut base = [0i64; MAX_CODE_LEN + 2];
        let mut vec = 0i64;
        for len in min_len..=max_len {
            vec += count[len + 1] - count[len];
            limit[len] = vec - 1;
            vec <<= 1;
        }
        for len in (min_len + 1)..=max_len {
            base[len] = ((limit[len - 1] + 1) << 1) - count[len];
        }

        Ok(Self {
            limit,
            base,
            perm,
            min_len,
        })
    }

    fn decode(&self, reader: &mut BitReader<'_>) -> Result<u16, String> {
        let mut len = self.min_len;
        let mut value = i64::from(reader.read_bits(len as u32)?);
        loop {
            if len > MAX_CODE_LEN {
                return Err("BZ2 ハフマン復号が収束しません".to_string());
            }
            if value <= self.limit[len] {
                let index = (value - self.base[len]) as usize;
                return self
                    .perm
                    .get(index)
                    .copied()
                    .ok_or_else(|| "BZ2 ハフマン復号の索引が不正です".to_string());
            }
            len += 1;
            value = (value << 1) | i64::from(reader.read_bit()?);
        }
    }
}

/// bzip2 ストリーム全体を復号する
pub(super) fn decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 4 || &data[0..3] != b"BZh" {
        return Err("BZ2 ヘッダーが不正です".to_string());
    }
    let level = data[3];
    if !(b'1'..=b'9').contains(&level) {
        return Err("BZ2 ブロックサイズレベルが不正です".to_string());
    }

    let mut reader = BitReader::new(&data[4..]);
    let mut out = Vec::new();
    let mut crc = Bz2Crc::new();
    let mut combined_crc = 0u32;

    loop {
        let magic = reader.read_bits_u64(48)?;
        if magic == EOS_MAGIC {
            let stored = reader.read_bits(32)?;
            if stored != combined_crc {
                return Err(format!(
                    "BZ2 ストリーム CRC 不一致: 期待 {:08X} 実際 {:08X}",
                    stored, combined_crc
                ));
            }
            break;
        }
        if magic != BLOCK_MAGIC {
            return Err("BZ2 ブロックマジックが不正です".to_string());
        }

        let block_crc = reader.read_bits(32)?;
        let randomized = reader.read_bits(1)?;
        if randomized != 0 {
            return Err("ランダム化された BZ2 ブロックは未対応です (廃止された形式)".to_string());
        }
        let orig_ptr = reader.read_bits(24)? as usize;

        // 使用シンボルのビットマップ
        let used_groups = reader.read_bits(16)?;
        let mut used = [false; 256];
        let mut used_symbols: Vec<u8> = Vec::new();
        for group in 0..16 {
            if (used_groups >> (15 - group)) & 1 == 1 {
                let bits = reader.read_bits(16)?;
                for bit in 0..16 {
                    if (bits >> (15 - bit)) & 1 == 1 {
                        used[group * 16 + bit] = true;
                    }
                }
            }
        }
        for (byte, &is_used) in used.iter().enumerate() {
            if is_used {
                used_symbols.push(byte as u8);
            }
        }
        if used_symbols.is_empty() {
            return Err("BZ2 の使用シンボルがありません".to_string());
        }
        let alpha_size = used_symbols.len() + 2;

        // ハフマンテーブル数とセレクター
        let num_tables = reader.read_bits(3)? as usize;
        if !(2..=6).contains(&num_tables) {
            return Err("BZ2 ハフマンテーブル数が不正です".to_string());
        }
        let num_selectors = reader.read_bits(15)? as usize;
        let mut selector_mtf: Vec<u8> = (0..num_tables as u8).collect();
        let mut selectors = Vec::with_capacity(num_selectors);
        for _ in 0..num_selectors {
            let mut index = 0usize;
            while reader.read_bit()? == 1 {
                index += 1;
                if index >= num_tables {
                    return Err("BZ2 セレクターが不正です".to_string());
                }
            }
            let selected = selector_mtf[index];
            selector_mtf.copy_within(0..index, 1);
            selector_mtf[0] = selected;
            selectors.push(selected as usize);
        }

        // ハフマン符号長（デルタ符号）
        let mut tables = Vec::with_capacity(num_tables);
        for _ in 0..num_tables {
            let mut current = reader.read_bits(5)? as i32;
            let mut lengths = Vec::with_capacity(alpha_size);
            for _ in 0..alpha_size {
                loop {
                    if !(1..=MAX_CODE_LEN as i32).contains(&current) {
                        return Err("BZ2 ハフマン符号長が範囲外です".to_string());
                    }
                    if reader.read_bit()? == 0 {
                        break;
                    }
                    if reader.read_bit()? == 0 {
                        current += 1;
                    } else {
                        current -= 1;
                    }
                }
                lengths.push(current as u8);
            }
            tables.push(HuffmanTable::from_lengths(&lengths)?);
        }

        // シンボル列を復号し、RLE2 (RUNA/RUNB) + MTF を戻して BWT 列を得る
        let eob = (alpha_size - 1) as u16;
        let mut mtf_list = used_symbols.clone();
        let mut bwt: Vec<u8> = Vec::new();
        let mut group_pos = 0usize;
        let mut group_index = 0usize;
        let mut run_length: u64 = 0;
        let mut run_bit: u32 = 0;

        loop {
            if group_pos == 0 {
                let selector = *selectors
                    .get(group_index)
                    .ok_or_else(|| "BZ2 セレクターが不足しています".to_string())?;
                if selector >= tables.len() {
                    return Err("BZ2 セレクターが範囲外です".to_string());
                }
                group_index += 1;
                group_pos = GROUP_SIZE;
            }
            group_pos -= 1;

            let symbol = tables[selectors[group_index - 1]].decode(&mut reader)?;

            if symbol <= 1 {
                // RUNA (0) / RUNB (1): ゼロランの長さをビジェクティブ 2 進で累積
                run_length += u64::from(symbol + 1) << run_bit;
                run_bit += 1;
                continue;
            }

            if run_length > 0 {
                let byte = mtf_list[0];
                for _ in 0..run_length {
                    bwt.push(byte);
                }
                run_length = 0;
                run_bit = 0;
            }

            if symbol == eob {
                break;
            }

            // MTF 復号（symbol - 1 が MTF 索引）
            let index = (symbol - 1) as usize;
            if index >= mtf_list.len() {
                return Err("BZ2 MTF 索引が範囲外です".to_string());
            }
            let byte = mtf_list[index];
            mtf_list.copy_within(0..index, 1);
            mtf_list[0] = byte;
            bwt.push(byte);
        }

        if orig_ptr >= bwt.len() {
            return Err("BZ2 の原文ポインタが範囲外です".to_string());
        }

        // 逆 BWT（libbz2 と同じ tt 方式）
        let n = bwt.len();
        let mut counts = [0usize; 256];
        for &b in &bwt {
            counts[b as usize] += 1;
        }
        let mut cftab = [0usize; 256];
        let mut sum = 0usize;
        for (byte, &count) in counts.iter().enumerate() {
            cftab[byte] = sum;
            sum += count;
        }
        let mut tt = vec![0u32; n];
        {
            let mut next = cftab;
            for (i, &b) in bwt.iter().enumerate() {
                tt[next[b as usize]] = i as u32;
                next[b as usize] += 1;
            }
        }

        let mut block = Vec::with_capacity(n);
        let mut pos = tt[orig_ptr];
        for _ in 0..n {
            block.push(bwt[pos as usize]);
            pos = tt[pos as usize];
        }

        // RLE1 復号（4 連続バイトの後に追加ラン長バイト）
        let mut decoded = Vec::with_capacity(block.len());
        let mut i = 0usize;
        while i < block.len() {
            let byte = block[i];
            let mut run = 1usize;
            while run < 4 && i + run < block.len() && block[i + run] == byte {
                run += 1;
            }
            if run == 4 {
                let extra = *block
                    .get(i + 4)
                    .ok_or_else(|| "BZ2 RLE ラン長が欠落しています".to_string())?
                    as usize;
                for _ in 0..(4 + extra) {
                    decoded.push(byte);
                }
                i += 5;
            } else {
                for _ in 0..run {
                    decoded.push(byte);
                }
                i += run;
            }
        }

        // ブロック CRC 検証
        crc.reset();
        crc.update(&decoded);
        let computed = crc.finish();
        if computed != block_crc {
            return Err(format!(
                "BZ2 ブロック CRC 不一致: 期待 {:08X} 実際 {:08X}",
                block_crc, computed
            ));
        }
        combined_crc = (combined_crc.rotate_left(1)) ^ block_crc;

        out.extend_from_slice(&decoded);
    }

    Ok(out)
}

// ---------------------------------------------------------------------------
// エンコーダー
// ---------------------------------------------------------------------------

/// 頻度から libbz2 互換の長さ制限付きハフマン符号長を求める
fn make_code_lengths(freqs: &[u32]) -> Vec<u8> {
    let n = freqs.len();
    let mut weights: Vec<u64> = freqs.iter().map(|&f| u64::from(f.max(1))).collect();

    loop {
        // 単純なハフマン木構築（(重み, ノード) の最小ヒープ相当を線形で処理）
        #[derive(Clone)]
        struct Node {
            weight: u64,
            left: Option<usize>,
            right: Option<usize>,
            symbol: Option<usize>,
        }
        let mut nodes: Vec<Node> = weights
            .iter()
            .enumerate()
            .map(|(symbol, &weight)| Node {
                weight,
                left: None,
                right: None,
                symbol: Some(symbol),
            })
            .collect();
        let mut heap: Vec<usize> = (0..nodes.len()).collect();

        while heap.len() > 1 {
            heap.sort_by(|&a, &b| nodes[b].weight.cmp(&nodes[a].weight));
            let a = heap.pop().unwrap_or(0);
            let b = heap.pop().unwrap_or(0);
            let merged = Node {
                weight: nodes[a].weight + nodes[b].weight,
                left: Some(a),
                right: Some(b),
                symbol: None,
            };
            nodes.push(merged);
            heap.push(nodes.len() - 1);
        }

        // 深さを求める
        let mut lengths = vec![0u8; n];
        let mut max_depth = 0usize;
        if let Some(&root) = heap.first() {
            let mut stack = vec![(root, 0usize)];
            while let Some((index, depth)) = stack.pop() {
                let node = &nodes[index];
                match node.symbol {
                    Some(symbol) => {
                        // 1 シンボルのみの場合も長さ 1 以上にする
                        lengths[symbol] = depth.max(1) as u8;
                        max_depth = max_depth.max(depth.max(1));
                    }
                    None => {
                        if let (Some(l), Some(r)) = (node.left, node.right) {
                            stack.push((l, depth + 1));
                            stack.push((r, depth + 1));
                        }
                    }
                }
            }
        }

        if max_depth <= MAX_ENCODE_LEN {
            return lengths;
        }
        // libbz2 と同様に頻度を圧縮して再構築する
        for weight in weights.iter_mut() {
            *weight = 1 + (*weight / 2);
        }
    }
}

/// 符号長からカノニカル符号を割り当てる（libbz2 hbAssignCodes と同じ規則）
fn assign_codes(lengths: &[u8]) -> Vec<u32> {
    let min_len = lengths.iter().copied().min().unwrap_or(1);
    let max_len = lengths.iter().copied().max().unwrap_or(1);
    let mut codes = vec![0u32; lengths.len()];
    let mut vec = 0u32;
    for len in min_len..=max_len {
        for (symbol, &l) in lengths.iter().enumerate() {
            if l == len {
                codes[symbol] = vec;
                vec += 1;
            }
        }
        vec <<= 1;
    }
    codes
}

/// 巡回シフトの接尾辞配列を倍加法で構築し、BWT 列と原文ポインタを返す
fn bwt_transform(block: &[u8]) -> (Vec<u8>, usize) {
    let n = block.len();
    if n == 0 {
        return (Vec::new(), 0);
    }

    let mut sa: Vec<u32> = (0..n as u32).collect();
    let mut rank: Vec<u32> = block.iter().map(|&b| u32::from(b)).collect();
    let mut tmp = vec![0u32; n];
    let mut k = 1usize;

    while k < n {
        let key = |i: u32| -> (u32, u32) {
            let second = rank[(i as usize + k) % n];
            (rank[i as usize], second)
        };
        sa.sort_unstable_by_key(|&i| key(i));

        tmp[sa[0] as usize] = 0;
        for w in 1..n {
            let prev = sa[w - 1];
            let cur = sa[w];
            tmp[cur as usize] = tmp[prev as usize] + u32::from(key(prev) != key(cur));
        }
        rank.copy_from_slice(&tmp);
        if rank[sa[n - 1] as usize] as usize == n - 1 {
            break;
        }
        k <<= 1;
    }

    let mut bwt = Vec::with_capacity(n);
    let mut orig_ptr = 0usize;
    for (row, &start) in sa.iter().enumerate() {
        if start == 0 {
            orig_ptr = row;
        }
        bwt.push(block[(start as usize + n - 1) % n]);
    }
    (bwt, orig_ptr)
}

/// RLE1 符号化（4 連続以上のバイトを 4 バイト + ラン長バイトに畳む）
fn rle1_encode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 8);
    let mut i = 0usize;
    while i < data.len() {
        let byte = data[i];
        let mut run = 1usize;
        while run < 255 && i + run < data.len() && data[i + run] == byte {
            run += 1;
        }
        if run >= 4 {
            out.extend_from_slice(&[byte; 4]);
            out.push((run - 4) as u8);
        } else {
            for _ in 0..run {
                out.push(byte);
            }
        }
        i += run;
    }
    out
}

/// 1 ブロックを符号化して書き出す
fn encode_block(writer: &mut BitWriter, raw: &[u8], crc: &mut Bz2Crc) -> Result<u32, String> {
    // ブロック CRC は RLE1 前の生データに対して計算する
    crc.reset();
    crc.update(raw);
    let block_crc = crc.finish();

    let rle1 = rle1_encode(raw);
    let (bwt, orig_ptr) = bwt_transform(&rle1);

    // 使用シンボル表
    let mut used = [false; 256];
    for &b in &bwt {
        used[b as usize] = true;
    }
    let used_symbols: Vec<u8> = (0..=255u8).filter(|&b| used[b as usize]).collect();
    let alpha_size = used_symbols.len() + 2;
    let eob = (alpha_size - 1) as u16;

    // MTF + RLE2 (RUNA/RUNB)
    let mut mtf_list = used_symbols.clone();
    let mut symbols: Vec<u16> = Vec::with_capacity(bwt.len());
    let mut zero_run: u64 = 0;

    let flush_zero_run = |run: &mut u64, symbols: &mut Vec<u16>| {
        if *run == 0 {
            return;
        }
        let mut pending = *run - 1;
        loop {
            symbols.push((pending & 1) as u16); // 偶数 → RUNA(0), 奇数 → RUNB(1)
            if pending < 2 {
                break;
            }
            pending = (pending - 2) / 2;
        }
        *run = 0;
    };

    for &byte in &bwt {
        let index = mtf_list
            .iter()
            .position(|&b| b == byte)
            .ok_or_else(|| "BZ2 MTF の内部整合性エラー".to_string())?;
        if index == 0 {
            zero_run += 1;
            continue;
        }
        flush_zero_run(&mut zero_run, &mut symbols);
        symbols.push((index + 1) as u16);
        mtf_list.copy_within(0..index, 1);
        mtf_list[0] = byte;
    }
    flush_zero_run(&mut zero_run, &mut symbols);
    symbols.push(eob);

    // ハフマン表: 2 表構成（最小構成）で全セレクターが表 0 を指す
    let mut freqs = vec![0u32; alpha_size];
    for &s in &symbols {
        freqs[s as usize] += 1;
    }
    let lengths = make_code_lengths(&freqs);
    let codes = assign_codes(&lengths);
    let num_selectors = symbols.len().div_ceil(GROUP_SIZE);

    // ブロックヘッダー
    writer.write_bits_u64(BLOCK_MAGIC, 48);
    writer.write_bits(block_crc, 32);
    writer.write_bit(0); // randomized = 0
    writer.write_bits(orig_ptr as u32, 24);

    // 使用シンボルビットマップ
    let mut group_bits = 0u32;
    for group in 0..16 {
        if (0..16).any(|bit| used[group * 16 + bit]) {
            group_bits |= 1 << (15 - group);
        }
    }
    writer.write_bits(group_bits, 16);
    for group in 0..16 {
        if group_bits & (1 << (15 - group)) != 0 {
            let mut bits = 0u32;
            for bit in 0..16 {
                if used[group * 16 + bit] {
                    bits |= 1 << (15 - bit);
                }
            }
            writer.write_bits(bits, 16);
        }
    }

    // テーブル数 2・セレクター（全て表 0 = 単一ビット 0）
    writer.write_bits(2, 3);
    writer.write_bits(num_selectors as u32, 15);
    for _ in 0..num_selectors {
        writer.write_bit(0);
    }

    // 符号長のデルタ符号化（2 表とも同一内容）
    for _ in 0..2 {
        let mut current = i32::from(lengths[0]);
        writer.write_bits(current as u32, 5);
        for &len in &lengths {
            let target = i32::from(len);
            while current < target {
                writer.write_bits(0b10, 2); // +1
                current += 1;
            }
            while current > target {
                writer.write_bits(0b11, 2); // -1
                current -= 1;
            }
            writer.write_bit(0);
        }
    }

    // シンボル列
    for &symbol in &symbols {
        let len = lengths[symbol as usize];
        writer.write_bits(codes[symbol as usize], u32::from(len));
    }

    Ok(block_crc)
}

/// bzip2 ストリーム全体を符号化する
///
/// `level` はブロックサイズ (1〜9、×100KB)。
pub(super) fn compress(data: &[u8], level: u8) -> Result<Vec<u8>, String> {
    let level = level.clamp(1, 9);
    // RLE1 で膨張する最悪ケース (4/5 倍) を考慮した入力チャンク上限
    let block_limit = (level as usize) * 100_000 - 20;
    let input_limit = block_limit * 4 / 5;

    let mut writer = BitWriter::new();
    writer.out.extend_from_slice(b"BZh");
    writer.out.push(b'0' + level);

    let mut crc = Bz2Crc::new();
    let mut combined_crc = 0u32;

    if data.is_empty() {
        // 空ストリーム: ブロック無しで終端のみ
        writer.write_bits_u64(EOS_MAGIC, 48);
        writer.write_bits(0, 32);
        return Ok(writer.finish());
    }

    for chunk in data.chunks(input_limit) {
        let block_crc = encode_block(&mut writer, chunk, &mut crc)?;
        combined_crc = combined_crc.rotate_left(1) ^ block_crc;
    }

    writer.write_bits_u64(EOS_MAGIC, 48);
    writer.write_bits(combined_crc, 32);
    Ok(writer.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Python bz2 (libbz2) が生成した実ストリーム: b"hello bzip2 world\n" * 100
    const REAL_BZ2: &[u8] = &[
        0x42, 0x5A, 0x68, 0x39, 0x31, 0x41, 0x59, 0x26, 0x53, 0x59, 0x2B, 0x90, //
        0x7C, 0x8C, 0x00, 0x01, 0x8F, 0xD9, 0x80, 0x00, 0x10, 0x40, 0x00, 0x10, //
        0x00, 0x16, 0x64, 0xD0, 0x90, 0x20, 0x00, 0x70, 0x40, 0x00, 0x00, 0xA5, //
        0x50, 0x06, 0x86, 0x9B, 0x91, 0x60, 0x8B, 0x72, 0x2F, 0x08, 0xB8, 0x22, //
        0xF0, 0x8B, 0x04, 0x5D, 0x11, 0x64, 0x8B, 0xE2, 0x2F, 0x48, 0xB8, 0x11, //
        0x64, 0x8B, 0x24, 0x5E, 0x91, 0x60, 0x8B, 0x24, 0x5F, 0x8B, 0xB9, 0x22, //
        0x9C, 0x28, 0x48, 0x15, 0xC8, 0x3E, 0x46, 0x00,
    ];

    fn real_content() -> Vec<u8> {
        b"hello bzip2 world\n".repeat(100)
    }

    #[test]
    fn decodes_real_libbz2_stream() -> Result<(), String> {
        let decoded = decompress(REAL_BZ2)?;
        if decoded != real_content() {
            return Err("libbz2 ストリームの復号結果が不一致です".to_string());
        }
        Ok(())
    }

    #[test]
    fn roundtrip_binary_data() -> Result<(), String> {
        let mut state: u32 = 0x9E37_79B9;
        let mut data = Vec::with_capacity(300 * 1024);
        while data.len() < 300 * 1024 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            data.extend_from_slice(&state.to_le_bytes());
        }
        let compressed = compress(&data, 1)?; // 複数ブロックを通す
        let decoded = decompress(&compressed)?;
        if decoded != data {
            return Err("BZ2 バイナリラウンドトリップが不一致です".to_string());
        }
        Ok(())
    }

    #[test]
    fn roundtrip_text_and_runs() -> Result<(), String> {
        let mut data = "bzip2 日本語テキストの往復テスト。".repeat(64).into_bytes();
        data.extend_from_slice(&[0xAA; 1000]); // RLE1 の長いラン
        data.extend_from_slice(&[0x00; 4]); // ちょうど 4 連続
        let compressed = compress(&data, 9)?;
        let decoded = decompress(&compressed)?;
        if decoded != data {
            return Err("BZ2 テキストラウンドトリップが不一致です".to_string());
        }
        Ok(())
    }

    #[test]
    fn roundtrip_empty_input() -> Result<(), String> {
        let compressed = compress(&[], 9)?;
        let decoded = decompress(&compressed)?;
        if !decoded.is_empty() {
            return Err("空入力の往復が空になりません".to_string());
        }
        Ok(())
    }
}
