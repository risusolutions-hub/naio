//! ZIP read/write (stored + deflate, zip64 read).

use crate::crc32;
use crate::deflate::{deflate, inflate};
use crate::error::{Error, Result};

const SIG_LOCAL: u32 = 0x0403_4b50;
const SIG_CENTRAL: u32 = 0x0201_4b50;
const SIG_EOCD: u32 = 0x0605_4b50;
const SIG_ZIP64_EOCD: u32 = 0x0606_4b50;
const SIG_ZIP64_LOCATOR: u32 = 0x0706_4b50;

#[derive(Debug, Clone)]
pub struct ZipEntry {
    pub name: String,
    pub data: Vec<u8>,
    pub method: u16,
}

pub struct ZipArchive {
    entries: Vec<ZipEntry>,
}

impl ZipArchive {
    pub fn open(data: &[u8]) -> Result<Self> {
        let eocd = find_eocd(data)?;
        let (cd_offset, total) = parse_eocd(data, eocd)?;
        let mut entries = Vec::with_capacity(total);
        let mut pos = cd_offset;
        for _ in 0..total {
            if pos + 46 > data.len() {
                return Err(Error::Truncated);
            }
            let sig = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap());
            if sig != SIG_CENTRAL {
                return Err(Error::Message("bad central directory".into()));
            }
            let method = u16::from_le_bytes(data[pos + 10..pos + 12].try_into().unwrap());
            let comp_size = u32::from_le_bytes(data[pos + 20..pos + 24].try_into().unwrap()) as u64;
            let uncomp_size =
                u32::from_le_bytes(data[pos + 24..pos + 28].try_into().unwrap()) as u64;
            let name_len =
                u16::from_le_bytes(data[pos + 28..pos + 30].try_into().unwrap()) as usize;
            let extra_len =
                u16::from_le_bytes(data[pos + 30..pos + 32].try_into().unwrap()) as usize;
            let comment_len =
                u16::from_le_bytes(data[pos + 32..pos + 34].try_into().unwrap()) as usize;
            let local_off = u32::from_le_bytes(data[pos + 42..pos + 46].try_into().unwrap()) as u64;
            let name_start = pos + 46;
            let name_end = name_start + name_len;
            if name_end > data.len() {
                return Err(Error::Truncated);
            }
            let name = String::from_utf8_lossy(&data[name_start..name_end]).into_owned();
            let extra = &data[name_end..name_end + extra_len];
            pos = name_end + extra_len + comment_len;
            let (comp_size, uncomp_size, local_off) =
                resolve_zip64(extra, comp_size, uncomp_size, local_off)?;
            let payload = read_local_entry(data, local_off, &name, comp_size, uncomp_size, method)?;
            entries.push(ZipEntry {
                name,
                data: payload,
                method,
            });
        }
        Ok(Self { entries })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn by_index(&self, index: usize) -> Result<&ZipEntry> {
        self.entries
            .get(index)
            .ok_or_else(|| Error::Message(format!("zip index {index} out of range")))
    }

    pub fn entries(&self) -> &[ZipEntry] {
        &self.entries
    }

    pub fn write(entries: &[(&str, &[u8])], method: ZipMethod) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        let mut offsets = Vec::new();
        for (name, data) in entries {
            offsets.push(out.len() as u32);
            let (method_id, payload) = match method {
                ZipMethod::Stored => (0u16, data.to_vec()),
                ZipMethod::Deflate => (8u16, deflate(data)?),
            };
            let crc = crc32::crc32(data);
            write_local(&mut out, name, method_id, &payload, data.len() as u32, crc)?;
            write_central(
                &mut central,
                name,
                method_id,
                &payload,
                data.len() as u32,
                crc,
                *offsets.last().unwrap(),
            )?;
        }
        let cd_start = out.len() as u32;
        out.extend_from_slice(&central);
        let cd_size = out.len() as u32 - cd_start;
        out.extend_from_slice(&SIG_EOCD.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_start.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        Ok(out)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ZipMethod {
    Stored,
    Deflate,
}

fn find_eocd(data: &[u8]) -> Result<usize> {
    let start = data.len().saturating_sub(65557);
    for i in (start..data.len()).rev() {
        if i + 4 <= data.len() && u32::from_le_bytes(data[i..i + 4].try_into().unwrap()) == SIG_EOCD
        {
            return Ok(i);
        }
    }
    Err(Error::Message("zip missing EOCD".into()))
}

fn parse_eocd(data: &[u8], pos: usize) -> Result<(usize, usize)> {
    if pos + 22 > data.len() {
        return Err(Error::Truncated);
    }
    let total = u16::from_le_bytes(data[pos + 10..pos + 12].try_into().unwrap()) as usize;
    let cd_size = u32::from_le_bytes(data[pos + 12..pos + 16].try_into().unwrap()) as usize;
    let cd_offset = u32::from_le_bytes(data[pos + 16..pos + 20].try_into().unwrap()) as usize;
    if cd_offset + cd_size <= data.len() {
        return Ok((cd_offset, total));
    }
    if pos >= 20 {
        let loc = pos - 20;
        if u32::from_le_bytes(data[loc..loc + 4].try_into().unwrap()) == SIG_ZIP64_LOCATOR {
            let z64_off = u64::from_le_bytes(data[loc + 8..loc + 16].try_into().unwrap()) as usize;
            return parse_zip64_eocd(data, z64_off);
        }
    }
    Ok((cd_offset, total))
}

fn parse_zip64_eocd(data: &[u8], pos: usize) -> Result<(usize, usize)> {
    if pos + 56 > data.len() {
        return Err(Error::Truncated);
    }
    if u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap()) != SIG_ZIP64_EOCD {
        return Err(Error::Message("bad zip64 eocd".into()));
    }
    let total = u64::from_le_bytes(data[pos + 32..pos + 40].try_into().unwrap()) as usize;
    let cd_size = u64::from_le_bytes(data[pos + 40..pos + 48].try_into().unwrap()) as usize;
    let cd_offset = u64::from_le_bytes(data[pos + 48..pos + 56].try_into().unwrap()) as usize;
    Ok((cd_offset, total))
}

fn resolve_zip64(
    extra: &[u8],
    comp_size: u64,
    uncomp_size: u64,
    local_off: u64,
) -> Result<(u64, u64, u64)> {
    let mut pos = 0usize;
    let mut cs = comp_size;
    let mut us = uncomp_size;
    let mut lo = local_off;
    while pos + 4 <= extra.len() {
        let id = u16::from_le_bytes(extra[pos..pos + 2].try_into().unwrap());
        let size = u16::from_le_bytes(extra[pos + 2..pos + 4].try_into().unwrap()) as usize;
        pos += 4;
        if pos + size > extra.len() {
            break;
        }
        if id == 0x0001 {
            let mut p = pos;
            if cs == 0xFFFF_FFFF {
                cs = u64::from_le_bytes(extra[p..p + 8].try_into().unwrap());
                p += 8;
            }
            if us == 0xFFFF_FFFF {
                us = u64::from_le_bytes(extra[p..p + 8].try_into().unwrap());
                p += 8;
            }
            if lo == 0xFFFF_FFFF {
                lo = u64::from_le_bytes(extra[p..p + 8].try_into().unwrap());
            }
        }
        pos += size;
    }
    Ok((cs, us, lo))
}

fn read_local_entry(
    data: &[u8],
    offset: u64,
    expected_name: &str,
    comp_size: u64,
    uncomp_size: u64,
    method: u16,
) -> Result<Vec<u8>> {
    let pos = offset as usize;
    if pos + 30 > data.len() {
        return Err(Error::Truncated);
    }
    if u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap()) != SIG_LOCAL {
        return Err(Error::Message("bad local header".into()));
    }
    let name_len = u16::from_le_bytes(data[pos + 26..pos + 28].try_into().unwrap()) as usize;
    let extra_len = u16::from_le_bytes(data[pos + 28..pos + 30].try_into().unwrap()) as usize;
    let name_start = pos + 30;
    let payload_start = name_start + name_len + extra_len;
    let name = String::from_utf8_lossy(&data[name_start..name_start + name_len]);
    if name != expected_name {
        return Err(Error::Message(format!(
            "zip name mismatch: {name} vs {expected_name}"
        )));
    }
    let comp = comp_size as usize;
    if payload_start + comp > data.len() {
        return Err(Error::Truncated);
    }
    let compressed = &data[payload_start..payload_start + comp];
    match method {
        0 => {
            if compressed.len() as u64 != uncomp_size {
                return Err(Error::Message("stored size mismatch".into()));
            }
            Ok(compressed.to_vec())
        }
        8 => {
            let out = inflate(compressed)?;
            if out.len() as u64 != uncomp_size {
                return Err(Error::Message("deflate size mismatch".into()));
            }
            Ok(out)
        }
        m => Err(Error::Unsupported(format!("zip method {m}"))),
    }
}

fn write_local(
    out: &mut Vec<u8>,
    name: &str,
    method: u16,
    payload: &[u8],
    uncomp: u32,
    crc: u32,
) -> Result<()> {
    out.extend_from_slice(&SIG_LOCAL.to_le_bytes());
    out.extend_from_slice(&20u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&method.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&uncomp.to_le_bytes());
    out.extend_from_slice(&(name.len() as u16).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(payload);
    Ok(())
}

fn write_central(
    out: &mut Vec<u8>,
    name: &str,
    method: u16,
    payload: &[u8],
    uncomp: u32,
    crc: u32,
    local_off: u32,
) -> Result<()> {
    out.extend_from_slice(&SIG_CENTRAL.to_le_bytes());
    out.extend_from_slice(&20u16.to_le_bytes());
    out.extend_from_slice(&20u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&method.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&uncomp.to_le_bytes());
    out.extend_from_slice(&(name.len() as u16).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&local_off.to_le_bytes());
    out.extend_from_slice(name.as_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_stored_roundtrip() {
        let items = [("a.txt", b"zip hello".as_slice())];
        let data = ZipArchive::write(&items, ZipMethod::Stored).unwrap();
        let arc = ZipArchive::open(&data).unwrap();
        assert_eq!(arc.len(), 1);
        assert_eq!(arc.by_index(0).unwrap().data, b"zip hello");
    }

    #[test]
    fn zip_deflate_roundtrip() {
        let items = [("b.txt", b"deflated content here".as_slice())];
        let data = ZipArchive::write(&items, ZipMethod::Deflate).unwrap();
        let arc = ZipArchive::open(&data).unwrap();
        assert_eq!(arc.by_index(0).unwrap().data, b"deflated content here");
    }
}
