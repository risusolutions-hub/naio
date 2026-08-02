//! Combined sniff / guess logic.

use crate::categories::kind_of_mime;
use crate::error::{MimeError, MimeResult};
use crate::extmap::MimeRegistry;
use crate::magic::{match_bytes, CustomMagic, DEFAULT_SNIFF_BYTES, MAX_SNIFF_BYTES};
use crate::types::{MatchSource, MimeMatch};
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct SniffOpts {
    pub max_bytes: usize,
    pub prefer_magic: bool,
}

impl Default for SniffOpts {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_SNIFF_BYTES,
            prefer_magic: true,
        }
    }
}

fn clamp_max(n: usize) -> usize {
    n.max(16).min(MAX_SNIFF_BYTES)
}

/// Detect type from raw bytes using magic signatures.
pub fn from_bytes(data: &[u8], custom: &[CustomMagic]) -> Option<MimeMatch> {
    match_bytes(data, custom)
}

pub fn guess_mime(data: &[u8], custom: &[CustomMagic]) -> Option<String> {
    from_bytes(data, custom).map(|m| m.mime)
}

pub fn guess_extension_from_bytes(data: &[u8], custom: &[CustomMagic]) -> Option<String> {
    from_bytes(data, custom).map(|m| m.extension)
}

/// Read up to `max_bytes` from a file and detect via magic.
pub fn from_path(
    path: impl AsRef<Path>,
    opts: &SniffOpts,
    custom: &[CustomMagic],
) -> MimeResult<Option<MimeMatch>> {
    let path = path.as_ref();
    if !path.exists() {
        return Err(MimeError::PathNotFound(path.display().to_string()));
    }
    let mut f = File::open(path).map_err(|e| MimeError::Io(e.to_string()))?;
    let max = clamp_max(opts.max_bytes);
    let mut buf = vec![0u8; max];
    let n = f.read(&mut buf).map_err(|e| MimeError::Io(e.to_string()))?;
    buf.truncate(n);
    Ok(from_bytes(&buf, custom))
}

/// Combined path sniff: magic bytes + filename extension.
pub fn sniff_path(
    path: impl AsRef<Path>,
    registry: &MimeRegistry,
    opts: &SniffOpts,
    custom: &[CustomMagic],
) -> MimeResult<Option<MimeMatch>> {
    let path = path.as_ref();
    let magic = from_path(path, opts, custom)?;
    let ext_guess = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| registry.guess_type(n, false));
    let ext_match = ext_guess.as_ref().and_then(|g| {
        g.mime.as_ref().map(|mime| {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            MimeMatch {
                mime: mime.clone(),
                extension: ext,
                kind: kind_of_mime(mime),
                source: MatchSource::Extension,
                confidence: 0.6,
            }
        })
    });
    Ok(merge_matches(magic, ext_match, opts.prefer_magic))
}

fn merge_matches(
    magic: Option<MimeMatch>,
    ext: Option<MimeMatch>,
    prefer_magic: bool,
) -> Option<MimeMatch> {
    match (magic, ext) {
        (Some(m), Some(e)) if m.mime == e.mime => Some(MimeMatch {
            source: MatchSource::Combined,
            confidence: 0.98,
            ..m
        }),
        (Some(m), Some(_e)) if prefer_magic => Some(m),
        (Some(m), Some(e)) if e.confidence > m.confidence => Some(e),
        (Some(m), Some(_)) => Some(m),
        (Some(m), None) => Some(m),
        (None, Some(e)) => Some(e),
        (None, None) => None,
    }
}

pub fn read_sniff_bytes(path: &Path, max: usize) -> MimeResult<Vec<u8>> {
    if !path.exists() {
        return Err(MimeError::PathNotFound(path.display().to_string()));
    }
    let mut f = File::open(path).map_err(|e| MimeError::Io(e.to_string()))?;
    let max = clamp_max(max);
    let mut buf = vec![0u8; max];
    let n = f.read(&mut buf).map_err(|e| MimeError::Io(e.to_string()))?;
    buf.truncate(n);
    Ok(buf)
}

pub fn extension_from_path(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
}

pub fn filename_from_path(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff_bytes_png() {
        let data = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0];
        let m = from_bytes(&data, &[]).unwrap();
        assert_eq!(m.mime, "image/png");
    }

    #[test]
    fn merge_agrees() {
        let magic = Some(MimeMatch {
            mime: "image/png".into(),
            extension: "png".into(),
            kind: kind_of_mime("image/png"),
            source: MatchSource::Magic,
            confidence: 0.95,
        });
        let ext = Some(MimeMatch {
            mime: "image/png".into(),
            extension: "png".into(),
            kind: kind_of_mime("image/png"),
            source: MatchSource::Extension,
            confidence: 0.6,
        });
        let m = merge_matches(magic, ext, true).unwrap();
        assert_eq!(m.source, MatchSource::Combined);
    }
}
