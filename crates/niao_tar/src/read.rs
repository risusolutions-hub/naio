use std::collections::HashMap;
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use tar::Archive;

use crate::error::{Result, TarError};
use crate::format::Compression;
use crate::info::{info_from_header, EntryInfo};
use crate::io_util::{open_read_file, read_to_end};

pub const MAX_ENTRY_BYTES: usize = 512 * 1024 * 1024;

/// Options for reading tar archives.
#[derive(Debug, Clone)]
pub struct ReadOpts {
    pub compression: Option<Compression>,
    pub max_entry_bytes: usize,
}

impl Default for ReadOpts {
    fn default() -> Self {
        Self {
            compression: None,
            max_entry_bytes: MAX_ENTRY_BYTES,
        }
    }
}

/// In-memory index of tar members for random access by name.
pub struct TarReader {
    path: Option<PathBuf>,
    compression: Compression,
    members: Vec<EntryInfo>,
    data: Option<Vec<u8>>,
    iter_pos: usize,
}

impl TarReader {
    pub fn open_path(path: impl AsRef<Path>, opts: &ReadOpts) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let compression = opts
            .compression
            .unwrap_or_else(|| crate::format::detect_compression(&path));
        let mut file = open_read_file(&path, compression)?;
        let mut raw = Vec::new();
        file.read_to_end(&mut raw)?;
        Self::from_bytes(raw, Some(path), compression)
    }

    pub fn open_bytes(data: Vec<u8>, compression: Compression) -> Result<Self> {
        Self::from_bytes(data, None, compression)
    }

    fn from_bytes(data: Vec<u8>, path: Option<PathBuf>, compression: Compression) -> Result<Self> {
        let members = index_members(&data)?;
        Ok(Self {
            path,
            compression,
            members,
            data: Some(data),
            iter_pos: 0,
        })
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn compression(&self) -> Compression {
        self.compression
    }

    pub fn members(&self) -> &[EntryInfo] {
        &self.members
    }

    pub fn names(&self) -> Vec<String> {
        self.members.iter().map(|m| m.name.clone()).collect()
    }

    pub fn get(&self, name: &str) -> Result<&EntryInfo> {
        self.members
            .iter()
            .find(|m| m.name == name)
            .ok_or_else(|| TarError::NotFound(name.to_string()))
    }

    pub fn contains(&self, name: &str) -> bool {
        self.members.iter().any(|m| m.name == name)
    }

    pub fn read(&self, name: &str, max_bytes: usize) -> Result<Vec<u8>> {
        let info = self.get(name)?;
        if info.kind != crate::info::EntryKind::File && info.kind != crate::info::EntryKind::Unknown
        {
            return Ok(Vec::new());
        }
        let data = self.data.as_ref().ok_or(TarError::Closed)?;
        read_member_payload(data, info, max_bytes)
    }

    pub fn next_info(&mut self) -> Option<&EntryInfo> {
        if self.iter_pos >= self.members.len() {
            return None;
        }
        let idx = self.iter_pos;
        self.iter_pos += 1;
        self.members.get(idx)
    }

    pub fn rewind(&mut self) {
        self.iter_pos = 0;
    }

    pub fn raw_data(&self) -> Result<&[u8]> {
        self.data.as_deref().ok_or(TarError::Closed)
    }
}

fn index_members(data: &[u8]) -> Result<Vec<EntryInfo>> {
    let cursor = Cursor::new(data);
    let mut archive = Archive::new(cursor);
    let mut members = Vec::new();
    let mut index = 0usize;
    for entry in archive.entries()? {
        let entry = entry?;
        let header = entry.header();
        members.push(info_from_header(header, index)?);
        index += 1;
    }
    Ok(members)
}

fn read_member_payload(data: &[u8], info: &EntryInfo, max_bytes: usize) -> Result<Vec<u8>> {
    let cursor = Cursor::new(data);
    let mut archive = Archive::new(cursor);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry
            .path()
            .map_err(|e| TarError::Format(e.to_string()))?
            .into_owned();
        if path.to_string_lossy() == info.name {
            return read_to_end(&mut entry, max_bytes);
        }
    }
    Err(TarError::NotFound(info.name.clone()))
}

/// Streaming reader that does not buffer the whole archive — suitable for large files on disk.
pub struct TarStreamReader {
    path: PathBuf,
    compression: Compression,
    members: Vec<EntryInfo>,
    iter_pos: usize,
}

impl TarStreamReader {
    pub fn open_path(path: impl AsRef<Path>, opts: &ReadOpts) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let compression = opts
            .compression
            .unwrap_or_else(|| crate::format::detect_compression(&path));
        let file = File::open(&path)?;
        let reader = crate::io_util::wrap_reader(file, compression)?;
        let mut archive = Archive::new(reader);
        let mut members = Vec::new();
        let mut index = 0usize;
        for entry in archive.entries()? {
            let entry = entry?;
            members.push(info_from_header(entry.header(), index)?);
            index += 1;
        }
        Ok(Self {
            path,
            compression,
            members,
            iter_pos: 0,
        })
    }

    pub fn members(&self) -> &[EntryInfo] {
        &self.members
    }

    pub fn read_entry(&self, name: &str, max_bytes: usize) -> Result<Vec<u8>> {
        let file = File::open(&self.path)?;
        let reader = crate::io_util::wrap_reader(file, self.compression)?;
        let mut archive = Archive::new(reader);
        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry
                .path()
                .map_err(|e| TarError::Format(e.to_string()))?
                .into_owned();
            if path.to_string_lossy() == name {
                return read_to_end(&mut entry, max_bytes);
            }
        }
        Err(TarError::NotFound(name.to_string()))
    }

    pub fn next_info(&mut self) -> Option<&EntryInfo> {
        if self.iter_pos >= self.members.len() {
            return None;
        }
        let idx = self.iter_pos;
        self.iter_pos += 1;
        self.members.get(idx)
    }
}

pub fn is_tar_bytes(data: &[u8]) -> bool {
    if data.len() < 512 {
        return false;
    }
    looks_like_tar_header(&data[..512])
}

pub fn is_tar_file(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    if crate::format::is_tar_path(path) {
        return Ok(true);
    }
    let mut file = File::open(path)?;
    let mut hdr = [0u8; 512];
    if file.read_exact(&mut hdr).is_err() {
        return Ok(false);
    }
    if looks_like_tar_header(&hdr) {
        return Ok(true);
    }
    // Maybe gzip-wrapped tar — peek first 2 bytes then decode small prefix.
    if hdr[0] == 0x1f && hdr[1] == 0x8b {
        let mut file = File::open(path)?;
        let mut dec = flate2::read::GzDecoder::new(&mut file);
        let mut inner = [0u8; 512];
        if dec.read_exact(&mut inner).is_ok() {
            return Ok(looks_like_tar_header(&inner));
        }
    }
    Ok(false)
}

fn looks_like_tar_header(block: &[u8]) -> bool {
    if block.iter().all(|&b| b == 0) {
        return false;
    }
    let ustar = &block[257..263];
    ustar == b"ustar\0" || block[0] != 0
}

pub fn member_map(members: &[EntryInfo]) -> HashMap<String, usize> {
    members
        .iter()
        .enumerate()
        .map(|(i, m)| (m.name.clone(), i))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::write::{TarWriter, WriteOpts};
    use tempfile::TempDir;

    #[test]
    fn reader_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.tar");
        let mut w = TarWriter::create_path(&path, &WriteOpts::default()).unwrap();
        w.add_bytes("a/hello.txt", b"hello", None).unwrap();
        w.add_dir("a", &Default::default()).unwrap();
        w.finish().unwrap();

        let r = TarReader::open_path(&path, &ReadOpts::default()).unwrap();
        assert_eq!(r.names().len(), 2);
        assert!(r.contains("a/hello.txt"));
        assert_eq!(r.read("a/hello.txt", MAX_ENTRY_BYTES).unwrap(), b"hello");
    }
}
