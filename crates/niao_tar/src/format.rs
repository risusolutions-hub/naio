use crate::error::{Result, TarError};
use std::path::Path;

/// Compression wrapper for a tar stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    None,
    Gz,
    Zst,
}

impl Compression {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "none" | "tar" => Some(Self::None),
            "gz" | "gzip" | "tgz" | "tar.gz" => Some(Self::Gz),
            "zst" | "zstd" | "tzst" | "tar.zst" => Some(Self::Zst),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Gz => "gz",
            Self::Zst => "zst",
        }
    }
}

/// Open mode for tar archives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenMode {
    Read,
    Write,
    Append,
}

impl OpenMode {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.starts_with('r') {
            return Some(Self::Read);
        }
        if s.starts_with('w') {
            return Some(Self::Write);
        }
        if s.starts_with('a') {
            return Some(Self::Append);
        }
        None
    }
}

/// Parse `mode` strings like Python `tarfile`: `r`, `r:gz`, `w:zst`, `a:gz`.
pub fn parse_mode(mode: &str) -> Result<(OpenMode, Compression)> {
    let mode = mode.trim();
    if mode.is_empty() {
        return Ok((OpenMode::Read, Compression::None));
    }
    let (base, comp) = if let Some((b, c)) = mode.split_once(':') {
        (b, Some(c))
    } else {
        (mode, None)
    };
    let open = OpenMode::parse(base).ok_or_else(|| {
        TarError::InvalidMode(format!(
            "unknown tar mode '{mode}' (expected r/w/a[:compression])"
        ))
    })?;
    let compression = if let Some(c) = comp {
        Compression::parse(c).ok_or_else(|| {
            TarError::InvalidMode(format!(
                "unknown tar compression '{c}' (expected gz or zst)"
            ))
        })?
    } else {
        Compression::None
    };
    Ok((open, compression))
}

/// Detect compression from a filesystem path extension.
pub fn detect_compression(path: impl AsRef<Path>) -> Compression {
    let path = path.as_ref();
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name.ends_with(".tar.zst") || name.ends_with(".tzst") {
        Compression::Zst
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") || name.ends_with(".tar.gzip") {
        Compression::Gz
    } else if name.ends_with(".tar") {
        Compression::None
    } else {
        Compression::None
    }
}

/// Heuristic: does `path` look like a tar archive by extension?
pub fn is_tar_path(path: impl AsRef<Path>) -> bool {
    let name = path
        .as_ref()
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    name.ends_with(".tar")
        || name.ends_with(".tar.gz")
        || name.ends_with(".tgz")
        || name.ends_with(".tar.zst")
        || name.ends_with(".tzst")
}

/// Resolve compression when caller did not specify one explicitly.
pub fn resolve_compression(
    path: &Path,
    explicit: Option<Compression>,
    _mode: OpenMode,
) -> Compression {
    if let Some(c) = explicit {
        return c;
    }
    detect_compression(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_modes() {
        assert_eq!(
            parse_mode("r").unwrap(),
            (OpenMode::Read, Compression::None)
        );
        assert_eq!(
            parse_mode("r:gz").unwrap(),
            (OpenMode::Read, Compression::Gz)
        );
        assert_eq!(
            parse_mode("w:zst").unwrap(),
            (OpenMode::Write, Compression::Zst)
        );
        assert_eq!(
            parse_mode("a:gz").unwrap(),
            (OpenMode::Append, Compression::Gz)
        );
    }

    #[test]
    fn detect_ext() {
        assert_eq!(detect_compression("x.tar.gz"), Compression::Gz);
        assert_eq!(detect_compression("x.tar.zst"), Compression::Zst);
        assert_eq!(detect_compression("x.tar"), Compression::None);
    }
}
