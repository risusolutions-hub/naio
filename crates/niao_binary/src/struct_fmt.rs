//! Python `struct` format pack/unpack.

use crate::endian::Endian;
use std::fmt::Write as _;

const ISIZE: usize = std::mem::size_of::<isize>();
const USIZE: usize = std::mem::size_of::<usize>();

#[derive(Debug, Clone, PartialEq)]
pub enum PackValue {
    I8(i8),
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    Isize(isize),
    Usize(usize),
    F32(f32),
    F64(f64),
    Bool(bool),
    Bytes(Vec<u8>),
    Char(u8),
    Pointer(u64),
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnpackValue {
    I8(i8),
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    Isize(isize),
    Usize(usize),
    F32(f32),
    F64(f64),
    Bool(bool),
    Bytes(Vec<u8>),
    Char(u8),
    Pointer(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ItemKind {
    Pad,
    Char,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    Isize,
    Usize,
    F16,
    F32,
    F64,
    FixedString,
    PascalString,
    Pointer,
    Bool,
}

#[derive(Debug, Clone)]
struct FormatItem {
    kind: ItemKind,
    count: usize,
}

#[derive(Debug, Clone)]
pub struct CompiledStruct {
    pub format: String,
    pub endian: Endian,
    items: Vec<FormatItem>,
    size: usize,
}

impl CompiledStruct {
    pub fn parse(format: &str) -> Result<Self, String> {
        let (endian, rest) = parse_endian(format)?;
        let items = parse_items(rest)?;
        let size = calc_size(endian, &items)?;
        Ok(CompiledStruct {
            format: format.to_string(),
            endian,
            items,
            size,
        })
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn pack(&self, values: &[PackValue]) -> Result<Vec<u8>, String> {
        let expected = value_count(&self.items);
        if values.len() != expected {
            return Err(format!(
                "pack requires {expected} value(s), got {}",
                values.len()
            ));
        }
        let mut buf = Vec::with_capacity(self.size);
        let mut vi = 0usize;
        let mut offset = 0usize;
        for item in &self.items {
            offset = align_offset(self.endian, offset, item_align(item.kind));
            match item.kind {
                ItemKind::Pad => {
                    buf.resize(offset + item.count, 0);
                    offset += item.count;
                }
                ItemKind::FixedString => {
                    let PackValue::Bytes(b) = &values[vi] else {
                        return Err(type_mismatch("bytes/string", vi));
                    };
                    if b.len() > item.count {
                        return Err(format!(
                            "string length {} exceeds format size {}",
                            b.len(),
                            item.count
                        ));
                    }
                    buf.resize(offset, 0);
                    buf.extend_from_slice(b);
                    if b.len() < item.count {
                        buf.resize(offset + item.count, 0);
                    }
                    offset += item.count;
                    vi += 1;
                }
                ItemKind::PascalString => {
                    let PackValue::Bytes(b) = &values[vi] else {
                        return Err(type_mismatch("bytes/string", vi));
                    };
                    if b.len() > 255 {
                        return Err("pascal string exceeds 255 bytes".into());
                    }
                    if 1 + b.len() > item.count {
                        return Err(format!(
                            "pascal string length {} exceeds field size {}",
                            b.len(),
                            item.count
                        ));
                    }
                    buf.resize(offset, 0);
                    buf.push(b.len() as u8);
                    buf.extend_from_slice(b);
                    buf.resize(offset + item.count, 0);
                    offset += item.count;
                    vi += 1;
                }
                _ => {
                    for _ in 0..item.count {
                        let size = item_size(item.kind);
                        buf.resize(offset + size, 0);
                        pack_one(
                            self.endian,
                            item.kind,
                            &values[vi],
                            &mut buf[offset..offset + size],
                        )?;
                        offset += size;
                        vi += 1;
                    }
                }
            }
        }
        buf.resize(self.size, 0);
        Ok(buf)
    }

    pub fn unpack(&self, data: &[u8], offset: usize) -> Result<(Vec<UnpackValue>, usize), String> {
        if offset > data.len() {
            return Err(format!(
                "offset {offset} past end of buffer (len={})",
                data.len()
            ));
        }
        let need = self.size;
        if offset + need > data.len() {
            return Err(format!(
                "need {need} bytes at offset {offset}, buffer has {}",
                data.len()
            ));
        }
        let slice = &data[offset..offset + need];
        let mut out = Vec::new();
        let mut pos = 0usize;
        for item in &self.items {
            pos = align_offset(self.endian, pos, item_align(item.kind));
            match item.kind {
                ItemKind::Pad => pos += item.count,
                ItemKind::FixedString => {
                    let end = pos + item.count;
                    let mut bytes = slice[pos..end].to_vec();
                    if let Some(nul) = bytes.iter().position(|&b| b == 0) {
                        bytes.truncate(nul);
                    }
                    out.push(UnpackValue::Bytes(bytes));
                    pos = end;
                }
                ItemKind::PascalString => {
                    let end = pos + item.count;
                    let len = slice[pos] as usize;
                    if pos + 1 + len > end {
                        return Err("invalid pascal string length".into());
                    }
                    out.push(UnpackValue::Bytes(slice[pos + 1..pos + 1 + len].to_vec()));
                    pos = end;
                }
                _ => {
                    for _ in 0..item.count {
                        let (v, n) = unpack_one(self.endian, item.kind, &slice[pos..])?;
                        out.push(v);
                        pos += n;
                    }
                }
            }
        }
        Ok((out, offset + self.size))
    }

    pub fn iter_unpack<'a>(
        &self,
        data: &'a [u8],
    ) -> Result<Vec<(Vec<UnpackValue>, usize)>, String> {
        let mut results = Vec::new();
        let mut offset = 0usize;
        while offset + self.size <= data.len() {
            let (vals, next) = self.unpack(data, offset)?;
            results.push((vals, offset));
            offset = next;
        }
        if offset < data.len() {
            return Err(format!(
                "{} trailing byte(s) after structured unpack",
                data.len() - offset
            ));
        }
        Ok(results)
    }

    pub fn pack_into(
        &self,
        buf: &mut [u8],
        offset: usize,
        values: &[PackValue],
    ) -> Result<usize, String> {
        let packed = self.pack(values)?;
        let end = offset
            .checked_add(packed.len())
            .ok_or_else(|| "pack_into offset overflow".to_string())?;
        if end > buf.len() {
            return Err(format!(
                "pack_into needs {end} bytes, buffer has {}",
                buf.len()
            ));
        }
        buf[offset..end].copy_from_slice(&packed);
        Ok(end)
    }
}

fn parse_endian(format: &str) -> Result<(Endian, &str), String> {
    let mut chars = format.chars();
    match chars.next() {
        Some('@') => Ok((Endian::Native, chars.as_str())),
        Some('=') => Ok((Endian::NativeStandard, chars.as_str())),
        Some('<') => Ok((Endian::Little, chars.as_str())),
        Some('>') | Some('!') => Ok((Endian::Big, chars.as_str())),
        _ => Ok((Endian::Native, format)),
    }
}

fn parse_items(rest: &str) -> Result<Vec<FormatItem>, String> {
    let mut items = Vec::new();
    let bytes = rest.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let mut count: usize = 0;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            count = count
                .checked_mul(10)
                .and_then(|c| c.checked_add((bytes[i] - b'0') as usize))
                .ok_or_else(|| "repeat count overflow".to_string())?;
            i += 1;
        }
        if count == 0 {
            count = 1;
        }
        if i >= bytes.len() {
            return Err("format string ends with repeat count".into());
        }
        let code = bytes[i] as char;
        i += 1;
        let kind = match code {
            'x' => ItemKind::Pad,
            'c' => ItemKind::Char,
            'b' => ItemKind::I8,
            'B' => ItemKind::U8,
            '?' => ItemKind::Bool,
            'h' => ItemKind::I16,
            'H' => ItemKind::U16,
            'i' | 'l' => ItemKind::I32,
            'I' | 'L' => ItemKind::U32,
            'q' => ItemKind::I64,
            'Q' => ItemKind::U64,
            'n' => ItemKind::Isize,
            'N' => ItemKind::Usize,
            'e' => ItemKind::F16,
            'f' => ItemKind::F32,
            'd' => ItemKind::F64,
            's' => ItemKind::FixedString,
            'p' => ItemKind::PascalString,
            'P' => ItemKind::Pointer,
            other => return Err(format!("bad format char '{other}'")),
        };
        items.push(FormatItem { kind, count });
    }
    if items.is_empty() {
        return Err("empty format string".into());
    }
    Ok(items)
}

fn value_count(items: &[FormatItem]) -> usize {
    items
        .iter()
        .map(|item| match item.kind {
            ItemKind::Pad => 0,
            ItemKind::FixedString | ItemKind::PascalString => 1,
            _ => item.count,
        })
        .sum()
}

fn item_size(kind: ItemKind) -> usize {
    match kind {
        ItemKind::Pad => 1,
        ItemKind::Char | ItemKind::I8 | ItemKind::U8 | ItemKind::Bool => 1,
        ItemKind::I16 | ItemKind::U16 | ItemKind::F16 => 2,
        ItemKind::I32 | ItemKind::U32 | ItemKind::F32 => 4,
        ItemKind::I64 | ItemKind::U64 | ItemKind::F64 | ItemKind::Pointer => 8,
        ItemKind::Isize => ISIZE,
        ItemKind::Usize => USIZE,
        ItemKind::FixedString | ItemKind::PascalString => 1,
    }
}

fn item_align(kind: ItemKind) -> usize {
    match kind {
        ItemKind::Pad | ItemKind::Char | ItemKind::I8 | ItemKind::U8 | ItemKind::Bool => 1,
        ItemKind::I16 | ItemKind::U16 | ItemKind::F16 => 2,
        ItemKind::I32 | ItemKind::U32 | ItemKind::F32 => 4,
        ItemKind::I64
        | ItemKind::U64
        | ItemKind::F64
        | ItemKind::Pointer
        | ItemKind::Isize
        | ItemKind::Usize => 8,
        ItemKind::FixedString | ItemKind::PascalString => 1,
    }
}

fn align_offset(endian: Endian, offset: usize, align: usize) -> usize {
    if !endian.uses_alignment() || align <= 1 {
        return offset;
    }
    let rem = offset % align;
    if rem == 0 {
        offset
    } else {
        offset + align - rem
    }
}

fn calc_size(endian: Endian, items: &[FormatItem]) -> Result<usize, String> {
    let mut size = 0usize;
    for item in items {
        size = align_offset(endian, size, item_align(item.kind));
        match item.kind {
            ItemKind::Pad => size += item.count,
            ItemKind::FixedString | ItemKind::PascalString => size += item.count,
            _ => size += item_size(item.kind) * item.count,
        }
    }
    Ok(size)
}

fn type_mismatch(expected: &str, idx: usize) -> String {
    format!("value {idx} must be {expected}")
}

fn write_int<T: Copy>(endian: Endian, size: usize, v: T, out: &mut [u8]) -> usize
where
    i64: From<T>,
{
    let n: i64 = v.into();
    write_uint(endian, size, n as u64, out);
    size
}

fn write_uint(endian: Endian, size: usize, v: u64, out: &mut [u8]) {
    let le = endian.is_little();
    for i in 0..size {
        let b = (v >> (8 * i)) as u8;
        let j = if le { i } else { size - 1 - i };
        out[j] = b;
    }
}

fn read_int(endian: Endian, size: usize, data: &[u8]) -> i64 {
    let u = read_uint(endian, size, data);
    let sign_bit = 1i64 << (size * 8 - 1);
    if u as i64 & sign_bit != 0 {
        u as i64 | !((1i64 << (size * 8)) - 1)
    } else {
        u as i64
    }
}

fn read_uint(endian: Endian, size: usize, data: &[u8]) -> u64 {
    let le = endian.is_little();
    let mut v = 0u64;
    for i in 0..size {
        let j = if le { i } else { size - 1 - i };
        v |= u64::from(data[j]) << (8 * i);
    }
    v
}

fn f32_to_f16(f: f32) -> u16 {
    let bits = f.to_bits();
    let sign = (bits >> 31) & 1;
    let exp = (bits >> 23) & 0xFF;
    let frac = bits & 0x7F_FFFF;
    if exp == 0xFF {
        return ((sign << 15) | 0x7C00 | if frac != 0 { 0x200 } else { 0 }) as u16;
    }
    if exp == 0 && frac == 0 {
        return (sign << 15) as u16;
    }
    let mut new_exp = exp as i32 - 127 + 15;
    if new_exp >= 31 {
        return ((sign << 15) | 0x7C00) as u16;
    }
    if new_exp <= 0 {
        if new_exp < -10 {
            return (sign << 15) as u16;
        }
        let mant = (frac | 0x80_0000) >> (1 - new_exp);
        return ((sign << 15) | (mant >> 13)) as u16;
    }
    ((sign << 15) | ((new_exp as u32) << 10) | (frac >> 13)) as u16
}

fn f16_to_f32(h: u16) -> f32 {
    let sign = u32::from((h >> 15) & 1);
    let exp = (h >> 10) & 0x1F;
    let frac = u32::from(h & 0x3FF);
    if exp == 0 {
        if frac == 0 {
            return f32::from_bits(sign << 31);
        }
        let mut e = -14i32;
        let mut f = frac;
        while f & 0x400 == 0 {
            f <<= 1;
            e -= 1;
        }
        f &= 0x3FF;
        let bits = (sign << 31) | (((e + 127) as u32) << 23) | (f << 13);
        return f32::from_bits(bits);
    }
    if exp == 31 {
        let bits = (sign << 31) | 0x7F80_0000 | (frac << 13);
        return f32::from_bits(bits);
    }
    let bits = (sign << 31) | (((exp as i32 - 15 + 127) as u32) << 23) | (frac << 13);
    f32::from_bits(bits)
}

fn pack_one(
    endian: Endian,
    kind: ItemKind,
    value: &PackValue,
    out: &mut [u8],
) -> Result<usize, String> {
    let size = item_size(kind);
    if out.len() < size {
        return Err("internal pack buffer too small".into());
    }
    match (kind, value) {
        (ItemKind::Char, PackValue::Char(c)) | (ItemKind::Char, PackValue::U8(c)) => {
            out[0] = *c;
        }
        (ItemKind::I8, PackValue::I8(v)) => out[0] = *v as u8,
        (ItemKind::U8, PackValue::U8(v)) => out[0] = *v,
        (ItemKind::Bool, PackValue::Bool(b)) => out[0] = if *b { 1 } else { 0 },
        (ItemKind::I16, PackValue::I16(v)) => {
            write_int(endian, 2, *v, out);
        }
        (ItemKind::U16, PackValue::U16(v)) => write_uint(endian, 2, u64::from(*v), out),
        (ItemKind::I32, PackValue::I32(v)) => {
            write_int(endian, 4, *v, out);
        }
        (ItemKind::U32, PackValue::U32(v)) => write_uint(endian, 4, u64::from(*v), out),
        (ItemKind::I64, PackValue::I64(v)) => {
            write_int(endian, 8, *v, out);
        }
        (ItemKind::U64, PackValue::U64(v)) => write_uint(endian, 8, *v, out),
        (ItemKind::Isize, PackValue::Isize(v)) => write_uint(endian, ISIZE, *v as u64, out),
        (ItemKind::Usize, PackValue::Usize(v)) => write_uint(endian, USIZE, *v as u64, out),
        (ItemKind::F16, PackValue::F32(v)) => {
            let h = f32_to_f16(*v);
            write_uint(endian, 2, u64::from(h), out);
        }
        (ItemKind::F32, PackValue::F32(v)) => {
            let bits = v.to_bits();
            write_uint(endian, 4, u64::from(bits), out);
        }
        (ItemKind::F64, PackValue::F64(v)) => {
            let bits = v.to_bits();
            write_uint(endian, 8, bits, out);
        }
        (ItemKind::Pointer, PackValue::Pointer(v)) | (ItemKind::Pointer, PackValue::U64(v)) => {
            write_uint(endian, 8, *v, out);
        }
        (k, v) => {
            let mut msg = String::new();
            let _ = write!(msg, "cannot pack {:?} as {:?}", v, k);
            return Err(msg);
        }
    }
    Ok(size)
}

fn unpack_one(endian: Endian, kind: ItemKind, data: &[u8]) -> Result<(UnpackValue, usize), String> {
    let size = item_size(kind);
    if data.len() < size {
        return Err("unexpected end of buffer".into());
    }
    let slice = &data[..size];
    let v = match kind {
        ItemKind::Char => UnpackValue::Char(slice[0]),
        ItemKind::I8 => UnpackValue::I8(read_int(endian, 1, slice) as i8),
        ItemKind::U8 => UnpackValue::U8(slice[0]),
        ItemKind::Bool => UnpackValue::Bool(slice[0] != 0),
        ItemKind::I16 => UnpackValue::I16(read_int(endian, 2, slice) as i16),
        ItemKind::U16 => UnpackValue::U16(read_uint(endian, 2, slice) as u16),
        ItemKind::I32 => UnpackValue::I32(read_int(endian, 4, slice) as i32),
        ItemKind::U32 => UnpackValue::U32(read_uint(endian, 4, slice) as u32),
        ItemKind::I64 => UnpackValue::I64(read_int(endian, 8, slice)),
        ItemKind::U64 => UnpackValue::U64(read_uint(endian, 8, slice)),
        ItemKind::Isize => UnpackValue::Isize(read_int(endian, ISIZE, slice) as isize),
        ItemKind::Usize => UnpackValue::Usize(read_uint(endian, USIZE, slice) as usize),
        ItemKind::F16 => {
            let h = read_uint(endian, 2, slice) as u16;
            UnpackValue::F32(f16_to_f32(h))
        }
        ItemKind::F32 => {
            let bits = read_uint(endian, 4, slice) as u32;
            UnpackValue::F32(f32::from_bits(bits))
        }
        ItemKind::F64 => {
            let bits = read_uint(endian, 8, slice);
            UnpackValue::F64(f64::from_bits(bits))
        }
        ItemKind::Pointer => UnpackValue::Pointer(read_uint(endian, 8, slice)),
        other => return Err(format!("cannot unpack {:?}", other)),
    };
    Ok((v, size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn big_endian_uint32() {
        let fmt = CompiledStruct::parse(">I").unwrap();
        assert_eq!(fmt.size(), 4);
        let buf = fmt.pack(&[PackValue::U32(0x0102_0304)]).unwrap();
        assert_eq!(buf, vec![1, 2, 3, 4]);
        let (vals, _) = fmt.unpack(&buf, 0).unwrap();
        assert!(matches!(vals[0], UnpackValue::U32(0x0102_0304)));
    }

    #[test]
    fn little_endian_int16() {
        let fmt = CompiledStruct::parse("<h").unwrap();
        let buf = fmt.pack(&[PackValue::I16(-1)]).unwrap();
        assert_eq!(buf, vec![0xFF, 0xFF]);
    }

    #[test]
    fn aligned_struct() {
        let fmt = CompiledStruct::parse(">bh").unwrap();
        assert_eq!(fmt.size(), 4);
        let buf = fmt.pack(&[PackValue::I8(1), PackValue::I16(2)]).unwrap();
        assert_eq!(buf, vec![1, 0, 0, 2]);
    }

    #[test]
    fn string_field() {
        let fmt = CompiledStruct::parse("5s").unwrap();
        let buf = fmt.pack(&[PackValue::Bytes(b"hi".to_vec())]).unwrap();
        assert_eq!(&buf[..2], b"hi");
    }
}
