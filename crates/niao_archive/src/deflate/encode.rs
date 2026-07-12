//! RFC 1951 deflate encode (fixed Huffman + stored fallback).

use crate::crc32;
use crate::error::Result;

fn write_bits(out: &mut Vec<u8>, state: &mut BitState, value: u32, bits: u8) {
    state.buf |= value << state.count;
    state.count += bits;
    while state.count >= 8 {
        out.push(state.buf as u8);
        state.buf >>= 8;
        state.count -= 8;
    }
}

struct BitState {
    buf: u32,
    count: u8,
}

fn flush_bits(out: &mut Vec<u8>, state: &mut BitState) {
    if state.count > 0 {
        out.push(state.buf as u8);
        state.buf = 0;
        state.count = 0;
    }
}

fn encode_table(lengths: &[u8], max: u8) -> Vec<u16> {
    let mut counts = [0u16; 16];
    for &l in lengths {
        if l != 0 && (l as usize) < counts.len() {
            counts[l as usize] += 1;
        }
    }
    let mut next = [0u16; 16];
    let mut code = 0u16;
    for bits in 1..=max {
        code = (code + counts[bits as usize - 1]) << 1;
        next[bits as usize] = code;
    }
    let mut table = vec![0u16; lengths.len()];
    for (sym, &len) in lengths.iter().enumerate() {
        if len != 0 {
            let c = next[len as usize];
            next[len as usize] += 1;
            table[sym] = reverse_bits(c, len);
        }
    }
    table
}

fn fixed_codes() -> (Vec<u16>, Vec<u16>, [u8; 288], [u8; 32]) {
    let mut lit_len = [0u8; 288];
    for i in 0..144 {
        lit_len[i] = 8;
    }
    for i in 144..256 {
        lit_len[i] = 9;
    }
    for i in 256..280 {
        lit_len[i] = 7;
    }
    for i in 280..288 {
        lit_len[i] = 8;
    }
    let dist_len = [5u8; 32];
    let lit_codes = encode_table(&lit_len, 15);
    let dist_codes = encode_table(&dist_len, 15);
    (lit_codes, dist_codes, lit_len, dist_len)
}

fn reverse_bits(mut code: u16, len: u8) -> u16 {
    let mut out = 0u16;
    for _ in 0..len {
        out = (out << 1) | (code & 1);
        code >>= 1;
    }
    out
}

pub fn deflate(input: &[u8]) -> Result<Vec<u8>> {
    if input.len() < 32 {
        return Ok(stored_block(input, true));
    }
    let mut out = Vec::with_capacity(input.len());
    let mut st = BitState { buf: 0, count: 0 };
    write_bits(&mut out, &mut st, 1, 1); // final
    write_bits(&mut out, &mut st, 1, 2); // fixed
    let (lit_codes, dist_codes, lit_len, _dist_len) = fixed_codes();
    let mut i = 0;
    while i < input.len() {
        let best_len = (0..=258.min(input.len() - i))
            .rev()
            .find(|&len| len >= 3 && input[i..i + len].len() == len && find_match(&input[..i], &input[i..i + len]).is_some())
            .unwrap_or(1);
        if best_len >= 3 {
            if let Some((dist, len)) = find_match(&input[..i], &input[i..i + best_len]) {
                write_len_dist(&mut out, &mut st, len, dist, &lit_codes, &dist_codes, &lit_len);
                i += len;
                continue;
            }
        }
        write_lit(&mut out, &mut st, input[i] as u16, &lit_codes, &lit_len);
        i += 1;
    }
    write_lit(&mut out, &mut st, 256, &lit_codes, &lit_len);
    flush_bits(&mut out, &mut st);
    Ok(out)
}

fn find_match(history: &[u8], pattern: &[u8]) -> Option<(u16, usize)> {
    if pattern.len() < 3 {
        return None;
    }
    let max_dist = history.len().min(32_768);
    let start = history.len().saturating_sub(max_dist);
    for pos in (start..history.len()).rev() {
        let mut len = 0;
        while len < 258
            && pos + len < history.len()
            && len < pattern.len()
            && history[pos + len] == pattern[len]
        {
            len += 1;
        }
        if len >= 3 {
            let dist = history.len() - pos;
            return Some((dist as u16, len));
        }
    }
    None
}

fn write_lit(out: &mut Vec<u8>, st: &mut BitState, sym: u16, codes: &[u16], lens: &[u8; 288]) {
    let len = lens[sym as usize];
    write_bits(out, st, u32::from(codes[sym as usize]), len);
}

fn write_len_dist(
    out: &mut Vec<u8>,
    st: &mut BitState,
    len: usize,
    dist: u16,
    lit_codes: &[u16],
    dist_codes: &[u16],
    lit_len: &[u8; 288],
) {
    const LENS: [u16; 29] = [
        3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99,
        115, 131, 163, 195, 227, 258,
    ];
    const LEN_EXTRA: [u8; 29] = [
        0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
    ];
    const DISTS: [u16; 30] = [
        1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025,
        1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
    ];
    const DIST_EXTRA: [u8; 30] = [
        0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12,
        12, 13, 13,
    ];
    let mut sym = 257usize;
    for (idx, &base) in LENS.iter().enumerate() {
        let max = base as usize + if LEN_EXTRA[idx] == 0 { 0 } else { (1 << LEN_EXTRA[idx]) - 1 };
        if len >= base as usize && len <= max {
            sym = 257 + idx;
            break;
        }
    }
    write_lit(out, st, sym as u16, lit_codes, lit_len);
    let base = LENS[sym - 257] as u32;
    let extra = LEN_EXTRA[sym - 257];
    if extra > 0 {
        write_bits(out, st, (len as u32 - base) as u32, extra);
    }
    let mut dsym = 0usize;
    for (idx, &base) in DISTS.iter().enumerate() {
        let max = base as usize + if DIST_EXTRA[idx] == 0 { 0 } else { (1 << DIST_EXTRA[idx]) - 1 };
        if dist as usize >= base as usize && dist as usize <= max {
            dsym = idx;
            break;
        }
    }
    write_bits(out, st, u32::from(dist_codes[dsym]), 5);
    let base = DISTS[dsym] as u32;
    let extra = DIST_EXTRA[dsym];
    if extra > 0 {
        write_bits(out, st, u32::from(dist) - base, extra);
    }
}

fn stored_block(input: &[u8], final_block: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len() + 8);
    let mut st = BitState { buf: 0, count: 0 };
    write_bits(&mut out, &mut st, if final_block { 1 } else { 0 }, 1);
    write_bits(&mut out, &mut st, 0, 2);
    flush_bits(&mut out, &mut st);
    let len = input.len() as u16;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&(!len).to_le_bytes());
    out.extend_from_slice(input);
    out
}

pub fn zlib_encode(input: &[u8]) -> Result<Vec<u8>> {
    let mut out = vec![0x78, 0x01];
    out.extend(deflate(input)?);
    let adler = crate::adler32::adler32(input, 1);
    out.extend_from_slice(&adler.to_be_bytes());
    Ok(out)
}

pub fn gzip_encode(input: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() + 18);
    out.extend_from_slice(&[0x1f, 0x8b, 0x08, 0x00, 0, 0, 0, 0, 0, 0]);
    out.extend(deflate(input)?);
    let crc = crc32::crc32(input);
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&(input.len() as u32).to_le_bytes());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deflate::inflate;

    #[test]
    fn stored_roundtrip() {
        let data = b"abc";
        let enc = stored_block(data, true);
        let dec = inflate::inflate(&enc).unwrap();
        assert_eq!(dec, data);
    }
}
