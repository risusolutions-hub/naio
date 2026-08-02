//! RFC 1951 inflate.

use crate::bitstream::BitReader;
use crate::error::{Error, Result};

const LENS: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DISTS: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

struct Huffman {
    max_bits: u8,
    counts: Vec<u16>,
    symbols: Vec<u16>,
}

impl Huffman {
    fn decode(&self, br: &mut BitReader<'_>) -> Result<u16> {
        let mut code = 0u32;
        let mut first = 0u16;
        let mut index = 0usize;
        for bits in 1..=self.max_bits {
            code |= br.take_bits(1)?;
            let count = self.counts[bits as usize];
            if code < first as u32 + count as u32 {
                return Ok(self.symbols[index + (code - first as u32) as usize]);
            }
            index += count as usize;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err(Error::Message("invalid huffman code".into()))
    }
}

fn build_tree(lengths: &[u8], max: u8) -> Result<Huffman> {
    let mut counts = vec![0u16; max as usize + 1];
    for &len in lengths {
        if len != 0 && len <= max {
            counts[len as usize] += 1;
        }
    }
    let total: usize = counts.iter().map(|&c| c as usize).sum();
    let mut symbols = vec![0u16; total.max(1)];
    let mut offs = vec![0usize; max as usize + 1];
    let mut sum = 0usize;
    for bits in 1..=max {
        offs[bits as usize] = sum;
        sum += counts[bits as usize] as usize;
    }
    for (sym, &len) in lengths.iter().enumerate() {
        if len != 0 && len <= max {
            let idx = offs[len as usize];
            offs[len as usize] += 1;
            symbols[idx] = sym as u16;
        }
    }
    Ok(Huffman {
        max_bits: max,
        counts,
        symbols,
    })
}

fn fixed_lit() -> Huffman {
    let mut lengths = vec![0u8; 288];
    for i in 0..144 {
        lengths[i] = 8;
    }
    for i in 144..256 {
        lengths[i] = 9;
    }
    for i in 256..280 {
        lengths[i] = 7;
    }
    for i in 280..288 {
        lengths[i] = 8;
    }
    build_tree(&lengths, 15).expect("fixed lit")
}

fn fixed_dist() -> Huffman {
    let lengths = vec![5u8; 32];
    build_tree(&lengths, 15).expect("fixed dist")
}

fn read_code_lengths(br: &mut BitReader<'_>, count: usize, cl_count: usize) -> Result<Vec<u8>> {
    let order = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];
    let mut cl_lens = vec![0u8; 19];
    for i in 0..cl_count {
        cl_lens[order[i] as usize] = br.take_bits(3)? as u8;
    }
    let cl_tree = build_tree(&cl_lens, 7)?;
    let mut lengths = vec![0u8; count];
    let mut i = 0;
    while i < count {
        let sym = cl_tree.decode(br)?;
        match sym {
            0..=15 => {
                lengths[i] = sym as u8;
                i += 1;
            }
            16 => {
                let rep = br.take_bits(2)? as usize + 3;
                if i == 0 {
                    return Err(Error::Message("bad repeat".into()));
                }
                let v = lengths[i - 1];
                for _ in 0..rep {
                    if i >= count {
                        break;
                    }
                    lengths[i] = v;
                    i += 1;
                }
            }
            17 => {
                let rep = br.take_bits(3)? as usize + 3;
                for _ in 0..rep {
                    if i >= count {
                        break;
                    }
                    lengths[i] = 0;
                    i += 1;
                }
            }
            18 => {
                let rep = br.take_bits(7)? as usize + 11;
                for _ in 0..rep {
                    if i >= count {
                        break;
                    }
                    lengths[i] = 0;
                    i += 1;
                }
            }
            _ => return Err(Error::Message("bad code length symbol".into())),
        }
    }
    Ok(lengths)
}

fn inflate_block(
    br: &mut BitReader<'_>,
    lit: &Huffman,
    dist: &Huffman,
    out: &mut Vec<u8>,
) -> Result<()> {
    loop {
        let sym = lit.decode(br)?;
        if sym < 256 {
            out.push(sym as u8);
        } else if sym == 256 {
            break;
        } else {
            let len_idx = sym as usize - 257;
            if len_idx >= LENS.len() {
                return Err(Error::Message("bad length code".into()));
            }
            let mut len = LENS[len_idx] as u32;
            if LEN_EXTRA[len_idx] != 0 {
                len += br.take_bits(LEN_EXTRA[len_idx])?;
            }
            let dsym = dist.decode(br)? as usize;
            if dsym >= DISTS.len() {
                return Err(Error::Message("bad dist code".into()));
            }
            let mut dist_v = DISTS[dsym] as u32;
            if DIST_EXTRA[dsym] != 0 {
                dist_v += br.take_bits(DIST_EXTRA[dsym])?;
            }
            if dist_v as usize > out.len() {
                return Err(Error::Message("bad distance".into()));
            }
            let start = out.len() - dist_v as usize;
            for i in 0..len as usize {
                out.push(out[start + i]);
            }
        }
    }
    Ok(())
}

pub fn inflate(input: &[u8]) -> Result<Vec<u8>> {
    let mut br = BitReader::new(input);
    let mut out = Vec::with_capacity(input.len() * 2);
    let fixed_lit = fixed_lit();
    let fixed_dist = fixed_dist();
    loop {
        let final_block = br.take_bits(1)? != 0;
        let btype = br.take_bits(2)?;
        match btype {
            0 => {
                br.align_byte();
                if br.remaining_bytes() < 4 {
                    return Err(Error::Truncated);
                }
                let len = u16::from_le_bytes([input[br.pos], input[br.pos + 1]]) as usize;
                let nlen = u16::from_le_bytes([input[br.pos + 2], input[br.pos + 3]]) as usize;
                if (len ^ nlen) != 0xFFFF {
                    return Err(Error::Message("bad stored len".into()));
                }
                br.pos += 4;
                if br.remaining_bytes() < len {
                    return Err(Error::Truncated);
                }
                out.extend_from_slice(&input[br.pos..br.pos + len]);
                br.pos += len;
            }
            1 => inflate_block(&mut br, &fixed_lit, &fixed_dist, &mut out)?,
            2 => {
                let hlit = br.take_bits(5)? as usize + 257;
                let hdist = br.take_bits(5)? as usize + 1;
                let hclen = br.take_bits(4)? as usize + 4;
                let mut lens = read_code_lengths(&mut br, hlit + hdist, hclen)?;
                let dist_lens = lens.split_off(hlit);
                let lit_lens = lens;
                let lit = build_tree(&lit_lens, 15)?;
                let dist = build_tree(&dist_lens, 15)?;
                inflate_block(&mut br, &lit, &dist, &mut out)?;
            }
            _ => return Err(Error::Unsupported("invalid deflate block type".into())),
        }
        if final_block {
            break;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gzip;

    #[test]
    fn roundtrip_gzip_fixture() {
        let data = include_bytes!("../../tests/fixtures/hello.txt.gz");
        let out = gzip::decode(data).unwrap();
        assert_eq!(&out, b"hello archive\n");
    }
}
