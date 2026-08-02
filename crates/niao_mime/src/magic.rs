//! Magic-byte signature database and matching.

use crate::error::{MimeError, MimeResult};
use crate::types::{FileKind, MimeMatch};

/// Default bytes read when sniffing a file on disk.
pub const DEFAULT_SNIFF_BYTES: usize = 4096;

/// Maximum sniff window (guards memory on hostile inputs).
pub const MAX_SNIFF_BYTES: usize = 64 * 1024;

/// A compiled magic signature.
#[derive(Debug, Clone, Copy)]
pub struct MagicSignature {
    pub bytes: &'static [u8],
    pub mask: Option<&'static [u8]>,
    pub offset: usize,
    pub mime: &'static str,
    pub ext: &'static str,
    pub kind: FileKind,
    pub priority: u8,
}

/// Built-in magic signatures (~infer + filetype coverage).
pub const BUILTIN_SIGNATURES: &[MagicSignature] = &[
    // Images
    MagicSignature {
        bytes: b"\x89PNG\r\n\x1a\n",
        mask: None,
        offset: 0,
        mime: "image/png",
        ext: "png",
        kind: FileKind::Image,
        priority: 100,
    },
    MagicSignature {
        bytes: b"\xff\xd8\xff",
        mask: None,
        offset: 0,
        mime: "image/jpeg",
        ext: "jpg",
        kind: FileKind::Image,
        priority: 100,
    },
    MagicSignature {
        bytes: b"GIF87a",
        mask: None,
        offset: 0,
        mime: "image/gif",
        ext: "gif",
        kind: FileKind::Image,
        priority: 100,
    },
    MagicSignature {
        bytes: b"GIF89a",
        mask: None,
        offset: 0,
        mime: "image/gif",
        ext: "gif",
        kind: FileKind::Image,
        priority: 100,
    },
    MagicSignature {
        bytes: b"RIFF",
        mask: None,
        offset: 0,
        mime: "image/webp",
        ext: "webp",
        kind: FileKind::Image,
        priority: 90,
    },
    MagicSignature {
        bytes: b"BM",
        mask: None,
        offset: 0,
        mime: "image/bmp",
        ext: "bmp",
        kind: FileKind::Image,
        priority: 90,
    },
    MagicSignature {
        bytes: b"II*\x00",
        mask: None,
        offset: 0,
        mime: "image/tiff",
        ext: "tiff",
        kind: FileKind::Image,
        priority: 90,
    },
    MagicSignature {
        bytes: b"MM\x00*",
        mask: None,
        offset: 0,
        mime: "image/tiff",
        ext: "tiff",
        kind: FileKind::Image,
        priority: 90,
    },
    MagicSignature {
        bytes: b"\x00\x00\x01\x00",
        mask: None,
        offset: 0,
        mime: "image/x-icon",
        ext: "ico",
        kind: FileKind::Image,
        priority: 85,
    },
    MagicSignature {
        bytes: b"\x00\x00\x02\x00",
        mask: None,
        offset: 0,
        mime: "image/x-icon",
        ext: "ico",
        kind: FileKind::Image,
        priority: 85,
    },
    MagicSignature {
        bytes: b"\x00\x00\x00\x0c\x6a\x50\x20\x20\x0d\x0a\x87\x0a",
        mask: None,
        offset: 0,
        mime: "image/jp2",
        ext: "jp2",
        kind: FileKind::Image,
        priority: 95,
    },
    MagicSignature {
        bytes: b"\x00\x00\x00\x20\x66\x74\x79\x70\x61\x76\x69\x66",
        mask: None,
        offset: 0,
        mime: "image/avif",
        ext: "avif",
        kind: FileKind::Image,
        priority: 95,
    },
    MagicSignature {
        bytes: b"\x00\x00\x00\x18\x66\x74\x79\x70\x68\x65\x69\x63",
        mask: None,
        offset: 0,
        mime: "image/heic",
        ext: "heic",
        kind: FileKind::Image,
        priority: 95,
    },
    MagicSignature {
        bytes: b"\x00\x00\x00\x1c\x66\x74\x79\x70\x6d\x69\x66\x31",
        mask: None,
        offset: 0,
        mime: "image/heif",
        ext: "heif",
        kind: FileKind::Image,
        priority: 95,
    },
    MagicSignature {
        bytes: b"<svg",
        mask: None,
        offset: 0,
        mime: "image/svg+xml",
        ext: "svg",
        kind: FileKind::Image,
        priority: 70,
    },
    MagicSignature {
        bytes: b"<?xml",
        mask: None,
        offset: 0,
        mime: "application/xml",
        ext: "xml",
        kind: FileKind::Text,
        priority: 40,
    },
    // Video / audio containers
    MagicSignature {
        bytes: b"\x00\x00\x00\x18\x66\x74\x79\x70\x6d\x70\x34\x32",
        mask: None,
        offset: 0,
        mime: "video/mp4",
        ext: "mp4",
        kind: FileKind::Video,
        priority: 95,
    },
    MagicSignature {
        bytes: b"\x00\x00\x00\x1c\x66\x74\x79\x70\x69\x73\x6f\x6d",
        mask: None,
        offset: 0,
        mime: "video/mp4",
        ext: "mp4",
        kind: FileKind::Video,
        priority: 95,
    },
    MagicSignature {
        bytes: b"\x00\x00\x00\x14\x66\x74\x79\x70\x71\x74\x20\x20",
        mask: None,
        offset: 0,
        mime: "video/quicktime",
        ext: "mov",
        kind: FileKind::Video,
        priority: 95,
    },
    MagicSignature {
        bytes: b"\x1a\x45\xdf\xa3",
        mask: None,
        offset: 0,
        mime: "video/webm",
        ext: "webm",
        kind: FileKind::Video,
        priority: 95,
    },
    MagicSignature {
        bytes: b"RIFF",
        mask: None,
        offset: 0,
        mime: "video/avi",
        ext: "avi",
        kind: FileKind::Video,
        priority: 80,
    },
    MagicSignature {
        bytes: b"\x1f\x8b",
        mask: None,
        offset: 0,
        mime: "application/gzip",
        ext: "gz",
        kind: FileKind::Archive,
        priority: 90,
    },
    MagicSignature {
        bytes: b"ID3",
        mask: None,
        offset: 0,
        mime: "audio/mpeg",
        ext: "mp3",
        kind: FileKind::Audio,
        priority: 90,
    },
    MagicSignature {
        bytes: b"\xff\xfb",
        mask: None,
        offset: 0,
        mime: "audio/mpeg",
        ext: "mp3",
        kind: FileKind::Audio,
        priority: 85,
    },
    MagicSignature {
        bytes: b"\xff\xf3",
        mask: None,
        offset: 0,
        mime: "audio/mpeg",
        ext: "mp3",
        kind: FileKind::Audio,
        priority: 85,
    },
    MagicSignature {
        bytes: b"\xff\xf2",
        mask: None,
        offset: 0,
        mime: "audio/mpeg",
        ext: "mp3",
        kind: FileKind::Audio,
        priority: 85,
    },
    MagicSignature {
        bytes: b"OggS",
        mask: None,
        offset: 0,
        mime: "audio/ogg",
        ext: "ogg",
        kind: FileKind::Audio,
        priority: 90,
    },
    MagicSignature {
        bytes: b"fLaC",
        mask: None,
        offset: 0,
        mime: "audio/flac",
        ext: "flac",
        kind: FileKind::Audio,
        priority: 95,
    },
    MagicSignature {
        bytes: b"RIFF",
        mask: None,
        offset: 0,
        mime: "audio/wav",
        ext: "wav",
        kind: FileKind::Audio,
        priority: 80,
    },
    MagicSignature {
        bytes: b"FORM",
        mask: None,
        offset: 0,
        mime: "audio/aiff",
        ext: "aiff",
        kind: FileKind::Audio,
        priority: 85,
    },
    MagicSignature {
        bytes: b"MThd",
        mask: None,
        offset: 0,
        mime: "audio/midi",
        ext: "mid",
        kind: FileKind::Audio,
        priority: 90,
    },
    // Documents
    MagicSignature {
        bytes: b"%PDF",
        mask: None,
        offset: 0,
        mime: "application/pdf",
        ext: "pdf",
        kind: FileKind::Application,
        priority: 100,
    },
    MagicSignature {
        bytes: b"PK\x03\x04",
        mask: None,
        offset: 0,
        mime: "application/zip",
        ext: "zip",
        kind: FileKind::Archive,
        priority: 100,
    },
    MagicSignature {
        bytes: b"PK\x05\x06",
        mask: None,
        offset: 0,
        mime: "application/zip",
        ext: "zip",
        kind: FileKind::Archive,
        priority: 95,
    },
    MagicSignature {
        bytes: b"PK\x07\x08",
        mask: None,
        offset: 0,
        mime: "application/zip",
        ext: "zip",
        kind: FileKind::Archive,
        priority: 95,
    },
    MagicSignature {
        bytes: b"Rar!\x1a\x07\x00",
        mask: None,
        offset: 0,
        mime: "application/vnd.rar",
        ext: "rar",
        kind: FileKind::Archive,
        priority: 100,
    },
    MagicSignature {
        bytes: b"Rar!\x1a\x07\x01\x00",
        mask: None,
        offset: 0,
        mime: "application/vnd.rar",
        ext: "rar",
        kind: FileKind::Archive,
        priority: 100,
    },
    MagicSignature {
        bytes: b"7z\xbc\xaf\x27\x1c",
        mask: None,
        offset: 0,
        mime: "application/x-7z-compressed",
        ext: "7z",
        kind: FileKind::Archive,
        priority: 100,
    },
    MagicSignature {
        bytes: b"\xfd7zXZ\x00",
        mask: None,
        offset: 0,
        mime: "application/x-xz",
        ext: "xz",
        kind: FileKind::Archive,
        priority: 100,
    },
    MagicSignature {
        bytes: b"BZh",
        mask: None,
        offset: 0,
        mime: "application/x-bzip2",
        ext: "bz2",
        kind: FileKind::Archive,
        priority: 95,
    },
    MagicSignature {
        bytes: b"ustar\x00",
        mask: None,
        offset: 257,
        mime: "application/x-tar",
        ext: "tar",
        kind: FileKind::Archive,
        priority: 90,
    },
    MagicSignature {
        bytes: b"ustar\x20",
        mask: None,
        offset: 257,
        mime: "application/x-tar",
        ext: "tar",
        kind: FileKind::Archive,
        priority: 90,
    },
    MagicSignature {
        bytes: b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1",
        mask: None,
        offset: 0,
        mime: "application/msword",
        ext: "doc",
        kind: FileKind::Application,
        priority: 95,
    },
    MagicSignature {
        bytes: b"PK\x03\x04",
        mask: None,
        offset: 0,
        mime: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        ext: "docx",
        kind: FileKind::Application,
        priority: 70,
    },
    MagicSignature {
        bytes: b"PK\x03\x04",
        mask: None,
        offset: 0,
        mime: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        ext: "xlsx",
        kind: FileKind::Application,
        priority: 65,
    },
    MagicSignature {
        bytes: b"PK\x03\x04",
        mask: None,
        offset: 0,
        mime: "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        ext: "pptx",
        kind: FileKind::Application,
        priority: 65,
    },
    MagicSignature {
        bytes: b"PK\x03\x04",
        mask: None,
        offset: 0,
        mime: "application/epub+zip",
        ext: "epub",
        kind: FileKind::Application,
        priority: 60,
    },
    // Text / data
    MagicSignature {
        bytes: b"<!DOCTYPE html",
        mask: None,
        offset: 0,
        mime: "text/html",
        ext: "html",
        kind: FileKind::Text,
        priority: 85,
    },
    MagicSignature {
        bytes: b"<html",
        mask: None,
        offset: 0,
        mime: "text/html",
        ext: "html",
        kind: FileKind::Text,
        priority: 80,
    },
    MagicSignature {
        bytes: b"<HTML",
        mask: None,
        offset: 0,
        mime: "text/html",
        ext: "html",
        kind: FileKind::Text,
        priority: 80,
    },
    MagicSignature {
        bytes: b"{\"",
        mask: None,
        offset: 0,
        mime: "application/json",
        ext: "json",
        kind: FileKind::Text,
        priority: 75,
    },
    MagicSignature {
        bytes: b"[",
        mask: None,
        offset: 0,
        mime: "application/json",
        ext: "json",
        kind: FileKind::Text,
        priority: 50,
    },
    MagicSignature {
        bytes: b"---",
        mask: None,
        offset: 0,
        mime: "text/yaml",
        ext: "yaml",
        kind: FileKind::Text,
        priority: 60,
    },
    MagicSignature {
        bytes: b"BEGIN:",
        mask: None,
        offset: 0,
        mime: "text/calendar",
        ext: "ics",
        kind: FileKind::Text,
        priority: 70,
    },
    MagicSignature {
        bytes: b"<?xml",
        mask: None,
        offset: 0,
        mime: "text/xml",
        ext: "xml",
        kind: FileKind::Text,
        priority: 75,
    },
    MagicSignature {
        bytes: b"\xef\xbb\xbf",
        mask: None,
        offset: 0,
        mime: "text/plain",
        ext: "txt",
        kind: FileKind::Text,
        priority: 30,
    },
    // Executables / wasm
    MagicSignature {
        bytes: b"\x7fELF",
        mask: None,
        offset: 0,
        mime: "application/x-elf",
        ext: "elf",
        kind: FileKind::Application,
        priority: 100,
    },
    MagicSignature {
        bytes: b"\xcf\xfa\xed\xfe",
        mask: None,
        offset: 0,
        mime: "application/x-mach-binary",
        ext: "macho",
        kind: FileKind::Application,
        priority: 100,
    },
    MagicSignature {
        bytes: b"\xfe\xed\xfa\xcf",
        mask: None,
        offset: 0,
        mime: "application/x-mach-binary",
        ext: "macho",
        kind: FileKind::Application,
        priority: 100,
    },
    MagicSignature {
        bytes: b"MZ",
        mask: None,
        offset: 0,
        mime: "application/x-msdownload",
        ext: "exe",
        kind: FileKind::Application,
        priority: 95,
    },
    MagicSignature {
        bytes: b"\x00asm",
        mask: None,
        offset: 0,
        mime: "application/wasm",
        ext: "wasm",
        kind: FileKind::Application,
        priority: 100,
    },
    // Fonts
    MagicSignature {
        bytes: b"\x00\x01\x00\x00",
        mask: None,
        offset: 0,
        mime: "font/ttf",
        ext: "ttf",
        kind: FileKind::Font,
        priority: 90,
    },
    MagicSignature {
        bytes: b"OTTO",
        mask: None,
        offset: 0,
        mime: "font/otf",
        ext: "otf",
        kind: FileKind::Font,
        priority: 95,
    },
    MagicSignature {
        bytes: b"wOFF",
        mask: None,
        offset: 0,
        mime: "font/woff",
        ext: "woff",
        kind: FileKind::Font,
        priority: 95,
    },
    MagicSignature {
        bytes: b"wOF2",
        mask: None,
        offset: 0,
        mime: "font/woff2",
        ext: "woff2",
        kind: FileKind::Font,
        priority: 95,
    },
    // Database / misc
    MagicSignature {
        bytes: b"SQLite format 3\x00",
        mask: None,
        offset: 0,
        mime: "application/vnd.sqlite3",
        ext: "sqlite",
        kind: FileKind::Application,
        priority: 100,
    },
    MagicSignature {
        bytes: b"\x00\x00\x00\x0c\x6a\x50\x20\x20\x0d\x0a\x87\x0a",
        mask: None,
        offset: 0,
        mime: "image/jpx",
        ext: "jpx",
        kind: FileKind::Image,
        priority: 90,
    },
    MagicSignature {
        bytes: b"\x89HDF\r\n\x1a\n",
        mask: None,
        offset: 0,
        mime: "application/x-hdf",
        ext: "hdf",
        kind: FileKind::Application,
        priority: 100,
    },
    MagicSignature {
        bytes: b"\x93NUMPY",
        mask: None,
        offset: 0,
        mime: "application/octet-stream",
        ext: "npy",
        kind: FileKind::Application,
        priority: 90,
    },
    MagicSignature {
        bytes: b"\x84\x01",
        mask: None,
        offset: 0,
        mime: "application/x-parquet",
        ext: "parquet",
        kind: FileKind::Application,
        priority: 85,
    },
    MagicSignature {
        bytes: b"PAR1",
        mask: None,
        offset: 0,
        mime: "application/x-parquet",
        ext: "parquet",
        kind: FileKind::Application,
        priority: 95,
    },
    MagicSignature {
        bytes: b"\xd4\xc3\xb2\xa1",
        mask: None,
        offset: 0,
        mime: "application/zstd",
        ext: "zst",
        kind: FileKind::Archive,
        priority: 95,
    },
    MagicSignature {
        bytes: b"\x28\xb5\x2f\xfd",
        mask: None,
        offset: 0,
        mime: "application/zstd",
        ext: "zst",
        kind: FileKind::Archive,
        priority: 95,
    },
    MagicSignature {
        bytes: b"BC\xc0\xde",
        mask: None,
        offset: 0,
        mime: "application/pgp-signature",
        ext: "pgp",
        kind: FileKind::Application,
        priority: 80,
    },
    MagicSignature {
        bytes: b"\x95\x00",
        mask: None,
        offset: 0,
        mime: "application/x-msaccess",
        ext: "mdb",
        kind: FileKind::Application,
        priority: 85,
    },
    MagicSignature {
        bytes: b"Standard Jet DB",
        mask: None,
        offset: 0,
        mime: "application/x-msaccess",
        ext: "mdb",
        kind: FileKind::Application,
        priority: 85,
    },
];

/// User-defined magic rule (owned).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomMagic {
    pub bytes: Vec<u8>,
    pub mask: Option<Vec<u8>>,
    pub offset: usize,
    pub mime: String,
    pub ext: String,
    pub kind: FileKind,
    pub priority: u8,
}

impl CustomMagic {
    pub fn matches(&self, data: &[u8]) -> bool {
        signature_matches(data, self.offset, &self.bytes, self.mask.as_deref())
    }
}

#[inline]
fn signature_matches(data: &[u8], offset: usize, pattern: &[u8], mask: Option<&[u8]>) -> bool {
    let end = offset.saturating_add(pattern.len());
    if end > data.len() {
        return false;
    }
    let slice = &data[offset..end];
    match mask {
        Some(m) if m.len() == pattern.len() => slice
            .iter()
            .zip(pattern.iter().zip(m.iter()))
            .all(|(b, (p, msk))| (b & msk) == (p & msk)),
        _ => slice == pattern,
    }
}

/// Refine ambiguous RIFF/WEBP/AVI/WAV matches using the fourCC at offset 8.
fn refine_riff(data: &[u8], base: &MagicSignature) -> Option<MimeMatch> {
    if data.len() < 12 || &data[0..4] != b"RIFF" {
        return None;
    }
    let fourcc = &data[8..12];
    let (mime, ext, kind) = match fourcc {
        b"WEBP" => ("image/webp", "webp", FileKind::Image),
        b"AVI " => ("video/avi", "avi", FileKind::Video),
        b"WAVE" => ("audio/wav", "wav", FileKind::Audio),
        _ => return Some(MimeMatch::from_static(base)),
    };
    Some(MimeMatch {
        mime: mime.into(),
        extension: ext.into(),
        kind,
        source: crate::types::MatchSource::Magic,
        confidence: 0.95,
    })
}

/// Refine ZIP-based Office/OpenXML by scanning for `[Content_Types].xml` marker in the first 4k.
fn refine_zip(data: &[u8], base: &MagicSignature) -> Option<MimeMatch> {
    if data.len() < 4 || &data[0..4] != b"PK\x03\x04" {
        return None;
    }
    let scan = data.len().min(8192);
    let hay = &data[..scan];
    let (mime, ext) = if hay.windows(5).any(|w| w == b"word/")
        || hay.windows(5).any(|w| w == b"word\\")
        || hay.windows(12).any(|w| w.starts_with(b"word/"))
    {
        (
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            "docx",
        )
    } else if hay.windows(8).any(|w| w == b"xl/" || w == b"worksheets/") {
        (
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            "xlsx",
        )
    } else if hay.windows(8).any(|w| w == b"ppt/" || w == b"slides/") {
        (
            "application/vnd.openxmlformats-officedocument.presentationml.presentation",
            "pptx",
        )
    } else if hay
        .windows(20)
        .any(|w| w.starts_with(b"application/epub+zip") || w == b"OEBPS/")
    {
        ("application/epub+zip", "epub")
    } else if hay.windows(12).any(|w| w == b"AndroidManifest") {
        ("application/vnd.android.package-archive", "apk")
    } else if hay.windows(9).any(|w| w == b"META-INF/") {
        ("application/java-archive", "jar")
    } else {
        return Some(MimeMatch::from_static(base));
    };
    Some(MimeMatch {
        mime: mime.into(),
        extension: ext.into(),
        kind: FileKind::Archive,
        source: crate::types::MatchSource::Magic,
        confidence: 0.9,
    })
}

/// Match bytes against built-in signatures and optional custom rules.
pub fn match_bytes(data: &[u8], custom: &[CustomMagic]) -> Option<MimeMatch> {
    if data.is_empty() {
        return None;
    }
    if data.len() >= 4 && &data[0..4] == b"PK\x03\x04" {
        let base = MagicSignature {
            bytes: b"PK\x03\x04",
            mask: None,
            offset: 0,
            mime: "application/zip",
            ext: "zip",
            kind: FileKind::Archive,
            priority: 100,
        };
        if let Some(m) = refine_zip(data, &base) {
            let mut best = Some(m);
            for rule in custom {
                if rule.matches(data) {
                    let candidate = MimeMatch {
                        mime: rule.mime.clone(),
                        extension: rule.ext.clone(),
                        kind: rule.kind,
                        source: crate::types::MatchSource::Magic,
                        confidence: 0.85,
                    };
                    if best
                        .as_ref()
                        .map(|b| candidate.beats(b, rule.priority))
                        .unwrap_or(true)
                    {
                        best = Some(candidate);
                    }
                }
            }
            return best;
        }
    }
    let mut best: Option<MimeMatch> = None;
    for sig in BUILTIN_SIGNATURES {
        if signature_matches(data, sig.offset, sig.bytes, sig.mask) {
            let candidate = match sig.mime {
                "image/webp" | "video/avi" | "audio/wav" if sig.bytes == b"RIFF" => {
                    refine_riff(data, sig).unwrap_or_else(|| MimeMatch::from_static(sig))
                }
                "application/zip"
                | "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                | "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
                | "application/vnd.openxmlformats-officedocument.presentationml.presentation"
                | "application/epub+zip"
                    if sig.bytes == b"PK\x03\x04" =>
                {
                    refine_zip(data, sig).unwrap_or_else(|| MimeMatch::from_static(sig))
                }
                _ => MimeMatch::from_static(sig),
            };
            if best
                .as_ref()
                .map(|b| candidate.beats(b, sig.priority))
                .unwrap_or(true)
            {
                best = Some(candidate);
            }
        }
    }
    for rule in custom {
        if rule.matches(data) {
            let candidate = MimeMatch {
                mime: rule.mime.clone(),
                extension: rule.ext.clone(),
                kind: rule.kind,
                source: crate::types::MatchSource::Magic,
                confidence: 0.85,
            };
            if best
                .as_ref()
                .map(|b| candidate.beats(b, rule.priority))
                .unwrap_or(true)
            {
                best = Some(candidate);
            }
        }
    }
    best
}

/// Return true when `data` matches the given MIME (magic or known alias).
pub fn bytes_match_mime(data: &[u8], mime: &str, custom: &[CustomMagic]) -> bool {
    let norm = mime.trim().to_ascii_lowercase();
    match_bytes(data, custom)
        .map(|m| m.mime == norm)
        .unwrap_or(false)
}

pub fn signature_count() -> usize {
    BUILTIN_SIGNATURES.len()
}

pub fn parse_hex_magic(s: &str) -> MimeResult<Vec<u8>> {
    let t = s.trim();
    let hex = t.strip_prefix("0x").unwrap_or(t);
    if hex.is_empty() || hex.len() % 2 != 0 {
        return Err(MimeError::InvalidMagic(s.into()));
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    let bytes = hex.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let hi = from_hex(bytes[i])?;
        let lo = from_hex(bytes[i + 1])?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Ok(out)
}

fn from_hex(b: u8) -> MimeResult<u8> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(MimeError::InvalidMagic("non-hex digit".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_magic() {
        let data = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
        let m = match_bytes(&data, &[]).unwrap();
        assert_eq!(m.mime, "image/png");
        assert_eq!(m.extension, "png");
    }

    #[test]
    fn zip_refine_docx() {
        let data = b"PK\x03\x04\x00\x00word/document.xml";
        let m = match_bytes(data, &[]).unwrap();
        assert!(m.mime.contains("wordprocessingml"));
        assert_eq!(m.extension, "docx");
    }
}
