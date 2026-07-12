//! POSIX ustar + PAX tar read/write.

use crate::error::{Error, Result};
use crate::gzip;
use std::fs;
use std::path::{Component, Path, PathBuf};

const BLOCK: usize = 512;

#[derive(Debug, Clone)]
pub struct Entry {
    pub path: String,
    pub data: Vec<u8>,
    pub is_dir: bool,
    pub mode: u32,
}

pub struct Archive {
    entries: Vec<Entry>,
}

impl Archive {
    pub fn open(data: &[u8]) -> Result<Self> {
        let mut entries = Vec::new();
        let mut pos = 0usize;
        let mut pax: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        while pos + BLOCK <= data.len() {
            let block = &data[pos..pos + BLOCK];
            if block.iter().all(|&b| b == 0) {
                break;
            }
            let hdr = parse_header(block)?;
            pos += BLOCK;
            if hdr.typeflag == b'x' || hdr.typeflag == b'g' {
                if pos + hdr.size > data.len() {
                    return Err(Error::Truncated);
                }
                let body = &data[pos..pos + hdr.size];
                pos += hdr.size;
                pos = pad512(pos);
                if hdr.typeflag == b'x' {
                    pax = parse_pax(body);
                }
                continue;
            }
            let path = if let Some(p) = pax.get("path") {
                p.clone()
            } else {
                hdr.full_path()
            };
            let size = pax
                .get("size")
                .and_then(|s| s.parse().ok())
                .unwrap_or(hdr.size);
            pax.clear();
            if hdr.typeflag == b'5' || path.ends_with('/') {
                entries.push(Entry {
                    path: path.trim_end_matches('/').to_string(),
                    data: Vec::new(),
                    is_dir: true,
                    mode: hdr.mode,
                });
                continue;
            }
            if pos + size > data.len() {
                return Err(Error::Truncated);
            }
            let file_data = data[pos..pos + size].to_vec();
            pos += size;
            pos = pad512(pos);
            entries.push(Entry {
                path,
                data: file_data,
                is_dir: false,
                mode: hdr.mode,
            });
        }
        Ok(Self { entries })
    }

    pub fn open_gz(data: &[u8]) -> Result<Self> {
        let raw = gzip::decode(data)?;
        Self::open(&raw)
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn unpack(&self, dest: &Path) -> Result<()> {
        fs::create_dir_all(dest)?;
        for entry in &self.entries {
            let path = safe_join(dest, &entry.path)?;
            if entry.is_dir {
                fs::create_dir_all(&path)?;
                continue;
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, &entry.data)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if entry.mode != 0 {
                    let _ = fs::set_permissions(&path, fs::Permissions::from_mode(entry.mode));
                }
            }
        }
        Ok(())
    }

    pub fn write(entries: &[Entry]) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        for entry in entries {
            write_entry(&mut out, entry)?;
        }
        out.extend(std::iter::repeat(0u8).take(BLOCK));
        Ok(out)
    }

    pub fn write_gz(entries: &[Entry]) -> Result<Vec<u8>> {
        gzip::encode(&Self::write(entries)?)
    }
}

struct Header {
    mode: u32,
    size: usize,
    typeflag: u8,
    name: String,
    prefix: String,
}

impl Header {
    fn full_path(&self) -> String {
        if self.prefix.is_empty() {
            self.name.clone()
        } else {
            format!("{}/{}", self.prefix, self.name)
        }
    }
}

fn parse_header(block: &[u8]) -> Result<Header> {
    let checksum = parse_octal(&block[148..156]);
    let mut sum = 0u32;
    for (i, &b) in block.iter().enumerate() {
        sum += u32::from(if (148..156).contains(&i) { b' ' } else { b });
    }
    if u64::from(sum) != checksum {
        return Err(Error::Message("tar header checksum mismatch".into()));
    }
    Ok(Header {
        name: c_string(&block[0..100]),
        mode: parse_octal(&block[100..108]) as u32,
        size: parse_octal(&block[124..136]) as usize,
        typeflag: block[156],
        prefix: c_string(&block[345..500]),
    })
}

fn parse_octal(field: &[u8]) -> u64 {
    let mut val = 0u64;
    for &b in field {
        if b == 0 || b == b' ' {
            break;
        }
        if (b'0'..=b'7').contains(&b) {
            val = val * 8 + u64::from(b - b'0');
        } else {
            break;
        }
    }
    val
}

fn c_string(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn parse_pax(body: &[u8]) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let text = String::from_utf8_lossy(body);
    for line in text.split('\n') {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(sp) = line.find(' ') {
            if let Some(eq) = line[sp + 1..].find('=') {
                let key = line[sp + 1..sp + 1 + eq].to_string();
                let val = line[sp + 2 + eq..].to_string();
                map.insert(key, val);
            }
        }
    }
    map
}

fn pad512(mut pos: usize) -> usize {
    let rem = pos % BLOCK;
    if rem != 0 {
        pos += BLOCK - rem;
    }
    pos
}

fn safe_join(base: &Path, path: &str) -> Result<PathBuf> {
    let rel = Path::new(path);
    for comp in rel.components() {
        match comp {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(Error::Message(format!("unsafe tar path: {path}")));
            }
        }
    }
    Ok(base.join(rel))
}

fn write_entry(out: &mut Vec<u8>, entry: &Entry) -> Result<()> {
    let mut block = [0u8; BLOCK];
    let name = entry.path.rsplit('/').next().unwrap_or(&entry.path);
    let prefix = if entry.path.contains('/') {
        entry.path.rsplitn(2, '/').nth(1).unwrap_or("")
    } else {
        ""
    };
    write_field(&mut block[0..100], name.as_bytes());
    write_octal(&mut block[100..108], entry.mode.max(0o644) as u64);
    write_octal(&mut block[124..136], entry.data.len() as u64);
    block[156] = if entry.is_dir { b'5' } else { b'0' };
    write_field(&mut block[257..263], b"ustar\0");
    block[263] = b'0';
    block[264] = b'0';
    write_field(&mut block[345..500], prefix.as_bytes());
    let mut sum = 0u32;
    for &b in &block {
        sum += u32::from(b);
    }
    for i in 148..156 {
        sum = sum.saturating_sub(u32::from(block[i])).saturating_add(32);
    }
    write_octal(&mut block[148..156], sum as u64);
    out.extend_from_slice(&block);
    if !entry.is_dir {
        out.extend_from_slice(&entry.data);
        let rem = entry.data.len() % BLOCK;
        if rem != 0 {
            out.extend(std::iter::repeat(0u8).take(BLOCK - rem));
        }
    }
    Ok(())
}

fn write_field(dst: &mut [u8], src: &[u8]) {
    let n = src.len().min(dst.len().saturating_sub(1));
    dst[..n].copy_from_slice(&src[..n]);
}

fn write_octal(dst: &mut [u8], val: u64) {
    let s = format!("{val:o}");
    let n = s.len().min(dst.len().saturating_sub(1));
    dst[..n].copy_from_slice(s.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_tar() {
        let entries = vec![
            Entry {
                path: "dir/file.txt".into(),
                data: b"hello tar".to_vec(),
                is_dir: false,
                mode: 0o644,
            },
            Entry {
                path: "dir".into(),
                data: Vec::new(),
                is_dir: true,
                mode: 0o755,
            },
        ];
        let raw = Archive::write(&entries).unwrap();
        let arc = Archive::open(&raw).unwrap();
        assert_eq!(arc.entries().len(), 2);
        assert_eq!(arc.entries()[0].data, b"hello tar");
    }

    #[test]
    fn fixture_package_tar_checksum() {
        let gz = include_bytes!("../tests/fixtures/package.tar.gz");
        let raw = crate::gzip::decode(gz).unwrap();
        let block = &raw[..512];
        let mut sum = 0u64;
        for (i, &b) in block.iter().enumerate() {
            sum += u64::from(if (148..156).contains(&i) { b' ' } else { b });
        }
        let field = &block[148..156];
        let checksum = parse_octal(field);
        assert_eq!(sum, checksum, "field={field:?}");
        Archive::open(&raw).unwrap();
    }
}
