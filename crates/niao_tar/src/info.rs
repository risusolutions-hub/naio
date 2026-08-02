use std::path::{Component, Path, PathBuf};

use crate::error::{Result, TarError};

/// Entry type in a tar archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
    HardLink,
    Fifo,
    CharDevice,
    BlockDevice,
    Unknown,
}

impl EntryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Directory => "dir",
            Self::Symlink => "symlink",
            Self::HardLink => "link",
            Self::Fifo => "fifo",
            Self::CharDevice => "chr",
            Self::BlockDevice => "blk",
            Self::Unknown => "unknown",
        }
    }
}

/// Metadata for one tar member (~`TarInfo` subset).
#[derive(Debug, Clone)]
pub struct EntryInfo {
    pub name: String,
    pub size: u64,
    pub mode: u32,
    pub mtime: i64,
    pub uid: u64,
    pub gid: u64,
    pub kind: EntryKind,
    pub link_target: Option<String>,
    pub index: usize,
}

pub fn entry_kind(header: &tar::Header) -> EntryKind {
    use tar::EntryType;
    match header.entry_type() {
        EntryType::Regular => EntryKind::File,
        EntryType::Directory => EntryKind::Directory,
        EntryType::Symlink => EntryKind::Symlink,
        EntryType::Link => EntryKind::HardLink,
        EntryType::Fifo => EntryKind::Fifo,
        EntryType::Char => EntryKind::CharDevice,
        EntryType::Block => EntryKind::BlockDevice,
        _ => EntryKind::Unknown,
    }
}

pub fn info_from_header(header: &tar::Header, index: usize) -> Result<EntryInfo> {
    let path = header
        .path()
        .map_err(|e| TarError::Format(e.to_string()))?
        .into_owned();
    let link_target = header
        .link_name()
        .ok()
        .flatten()
        .map(|p| p.to_string_lossy().into_owned());
    Ok(EntryInfo {
        name: path.to_string_lossy().into_owned(),
        size: header.size().unwrap_or(0),
        mode: header.mode().unwrap_or(0o644),
        mtime: header.mtime().unwrap_or(0) as i64,
        uid: header.uid().unwrap_or(0) as u64,
        gid: header.gid().unwrap_or(0) as u64,
        kind: entry_kind(header),
        link_target,
        index,
    })
}

/// Reject absolute paths and `..` traversal when extracting.
pub fn safe_join(base: &Path, member: &str) -> Result<PathBuf> {
    let rel = Path::new(member);
    for comp in rel.components() {
        match comp {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(TarError::UnsafePath(member.to_string()));
            }
        }
    }
    Ok(base.join(rel))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_parent() {
        assert!(safe_join(Path::new("/tmp"), "../etc/passwd").is_err());
    }
}
