//! Extension <-> MIME bidirectional maps (Apache mime.types + IANA common types).

use crate::error::{MimeError, MimeResult};
use crate::types::GuessTypeResult;
use std::collections::HashMap;
use std::path::Path;

/// Normalize extension: strip leading dot, lowercase.
pub fn normalize_ext(ext: &str) -> MimeResult<String> {
    let t = ext.trim().trim_start_matches('.');
    if t.is_empty() || t.len() > 32 || !t.bytes().all(valid_ext_byte) {
        return Err(MimeError::InvalidExtension(ext.into()));
    }
    Ok(t.to_ascii_lowercase())
}

fn valid_ext_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'+' || b == b'-'
}

fn builtin_pairs() -> &'static [(&'static str, &'static str)] {
    &[
        ("323", "text/h323"),
        ("3gp", "video/3gpp"),
        ("7z", "application/x-7z-compressed"),
        ("aac", "audio/aac"),
        ("ai", "application/postscript"),
        ("aif", "audio/aiff"),
        ("aiff", "audio/aiff"),
        ("apk", "application/vnd.android.package-archive"),
        ("avi", "video/x-msvideo"),
        ("avif", "image/avif"),
        ("bin", "application/octet-stream"),
        ("bmp", "image/bmp"),
        ("bz", "application/x-bzip"),
        ("bz2", "application/x-bzip2"),
        ("c", "text/x-c"),
        ("cab", "application/vnd.ms-cab-compressed"),
        ("cbor", "application/cbor"),
        ("cer", "application/pkix-cert"),
        ("class", "application/java-vm"),
        ("cjs", "application/node"),
        ("conf", "text/plain"),
        ("cpp", "text/x-c++"),
        ("crt", "application/x-x509-ca-cert"),
        ("css", "text/css"),
        ("csv", "text/csv"),
        ("deb", "application/vnd.debian.binary-package"),
        ("dmg", "application/x-apple-diskimage"),
        ("doc", "application/msword"),
        (
            "docx",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        ),
        ("dot", "application/msword"),
        ("dtd", "application/xml-dtd"),
        ("dwg", "image/vnd.dwg"),
        ("eml", "message/rfc822"),
        ("eot", "application/vnd.ms-fontobject"),
        ("eps", "application/postscript"),
        ("epub", "application/epub+zip"),
        ("exe", "application/vnd.microsoft.portable-executable"),
        ("flac", "audio/flac"),
        ("flv", "video/x-flv"),
        ("gif", "image/gif"),
        ("gz", "application/gzip"),
        ("h", "text/x-c"),
        ("heic", "image/heic"),
        ("heif", "image/heif"),
        ("htm", "text/html"),
        ("html", "text/html"),
        ("ico", "image/vnd.microsoft.icon"),
        ("ics", "text/calendar"),
        ("ini", "text/plain"),
        ("jar", "application/java-archive"),
        ("java", "text/x-java-source"),
        ("jpe", "image/jpeg"),
        ("jpeg", "image/jpeg"),
        ("jpg", "image/jpeg"),
        ("js", "text/javascript"),
        ("json", "application/json"),
        ("jsonl", "application/jsonl"),
        ("jsx", "text/jsx"),
        ("key", "application/vnd.apple.keynote"),
        ("log", "text/plain"),
        ("m4a", "audio/mp4"),
        ("m4v", "video/mp4"),
        ("manifest", "text/cache-manifest"),
        ("md", "text/markdown"),
        ("mid", "audio/midi"),
        ("midi", "audio/midi"),
        ("mjs", "text/javascript"),
        ("mkv", "video/x-matroska"),
        ("mov", "video/quicktime"),
        ("mp3", "audio/mpeg"),
        ("mp4", "video/mp4"),
        ("mpeg", "video/mpeg"),
        ("mpg", "video/mpeg"),
        ("msi", "application/x-msdownload"),
        ("numbers", "application/vnd.apple.numbers"),
        ("odp", "application/vnd.oasis.opendocument.presentation"),
        ("ods", "application/vnd.oasis.opendocument.spreadsheet"),
        ("odt", "application/vnd.oasis.opendocument.text"),
        ("oga", "audio/ogg"),
        ("ogg", "audio/ogg"),
        ("ogv", "video/ogg"),
        ("opus", "audio/opus"),
        ("otf", "font/otf"),
        ("pages", "application/vnd.apple.pages"),
        ("parquet", "application/vnd.apache.parquet"),
        ("pb", "application/vnd.google.protobuf"),
        ("pdf", "application/pdf"),
        ("pem", "application/x-pem-file"),
        ("pgp", "application/pgp-encrypted"),
        ("png", "image/png"),
        ("ppt", "application/vnd.ms-powerpoint"),
        (
            "pptx",
            "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        ),
        ("ps", "application/postscript"),
        ("rar", "application/vnd.rar"),
        ("rpm", "application/x-rpm"),
        ("rs", "text/x-rust"),
        ("rss", "application/rss+xml"),
        ("rtf", "application/rtf"),
        ("sh", "application/x-sh"),
        ("sql", "application/sql"),
        ("sqlite", "application/vnd.sqlite3"),
        ("svg", "image/svg+xml"),
        ("swf", "application/x-shockwave-flash"),
        ("tar", "application/x-tar"),
        ("tcl", "application/x-tcl"),
        ("tex", "application/x-tex"),
        ("tgz", "application/gzip"),
        ("tif", "image/tiff"),
        ("tiff", "image/tiff"),
        ("toml", "application/toml"),
        ("ts", "text/typescript"),
        ("tsv", "text/tab-separated-values"),
        ("tsx", "text/tsx"),
        ("ttf", "font/ttf"),
        ("txt", "text/plain"),
        ("vcf", "text/vcard"),
        ("wasm", "application/wasm"),
        ("wav", "audio/wav"),
        ("weba", "audio/webm"),
        ("webm", "video/webm"),
        ("webp", "image/webp"),
        ("woff", "font/woff"),
        ("woff2", "font/woff2"),
        ("xhtml", "application/xhtml+xml"),
        ("xls", "application/vnd.ms-excel"),
        (
            "xlsx",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        ),
        ("xml", "application/xml"),
        ("xsd", "application/xml"),
        ("xsl", "application/xml"),
        ("yaml", "text/yaml"),
        ("yml", "text/yaml"),
        ("zip", "application/zip"),
        ("zst", "application/zstd"),
    ]
}

/// In-memory MIME registry with optional user overrides.
#[derive(Debug, Clone, Default)]
pub struct MimeRegistry {
    ext_to_mime: HashMap<String, String>,
    mime_to_exts: HashMap<String, Vec<String>>,
    strict_ext_to_mime: HashMap<String, String>,
    strict_mime_to_exts: HashMap<String, Vec<String>>,
}

impl MimeRegistry {
    pub fn builtin() -> Self {
        let mut reg = Self::default();
        for (ext, mime) in builtin_pairs() {
            reg.insert(*ext, *mime, false);
            reg.insert(*ext, *mime, true);
        }
        reg
    }

    fn map_for(&self, strict: bool) -> (&HashMap<String, String>, &HashMap<String, Vec<String>>) {
        if strict {
            (&self.strict_ext_to_mime, &self.strict_mime_to_exts)
        } else {
            (&self.ext_to_mime, &self.mime_to_exts)
        }
    }

    pub fn extension_to_mime(&self, ext: &str, strict: bool) -> Option<String> {
        let norm = normalize_ext(ext).ok()?;
        let (fwd, _) = self.map_for(strict);
        fwd.get(&norm).cloned()
    }

    pub fn mime_to_extensions(&self, mime: &str, strict: bool) -> Vec<String> {
        let norm = mime.trim().to_ascii_lowercase();
        let (_, rev) = self.map_for(strict);
        rev.get(&norm).cloned().unwrap_or_default()
    }

    pub fn guess_extension(&self, mime: &str, strict: bool) -> Option<String> {
        self.mime_to_extensions(mime, strict).into_iter().next()
    }

    pub fn guess_type(&self, filename: &str, strict: bool) -> GuessTypeResult {
        let path = Path::new(filename);
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(|e| normalize_ext(e).ok());
        let encoding = path
            .extension()
            .and_then(|e| e.to_str())
            .filter(|e| e.eq_ignore_ascii_case("gz") || e.eq_ignore_ascii_case("bz2"))
            .map(|_| "gzip".to_string());
        let mime = ext.as_ref().and_then(|e| self.extension_to_mime(e, strict));
        GuessTypeResult { mime, encoding }
    }

    pub fn add_type(&mut self, mime: &str, ext: &str, strict: bool) -> MimeResult<bool> {
        let norm_ext = normalize_ext(ext)?;
        let norm_mime = mime.trim().to_ascii_lowercase();
        if norm_mime.is_empty() || !norm_mime.contains('/') {
            return Err(MimeError::InvalidMime(mime.into()));
        }
        let (fwd, rev) = if strict {
            (&mut self.strict_ext_to_mime, &mut self.strict_mime_to_exts)
        } else {
            (&mut self.ext_to_mime, &mut self.mime_to_exts)
        };
        let replaced = fwd.insert(norm_ext.clone(), norm_mime.clone()).is_some();
        let list = rev.entry(norm_mime).or_default();
        if !list.iter().any(|x| x == &norm_ext) {
            list.push(norm_ext);
            list.sort();
        }
        Ok(replaced)
    }

    fn insert(&mut self, ext: &str, mime: &str, strict: bool) {
        let _ = self.add_type(mime, ext, strict);
    }

    pub fn known_extensions(&self, strict: bool) -> Vec<String> {
        let (fwd, _) = self.map_for(strict);
        let mut out: Vec<String> = fwd.keys().cloned().collect();
        out.sort();
        out
    }

    pub fn known_types(&self, strict: bool) -> Vec<String> {
        let (_, rev) = self.map_for(strict);
        let mut out: Vec<String> = rev.keys().cloned().collect();
        out.sort();
        out
    }

    pub fn common_types(&self) -> HashMap<String, String> {
        self.ext_to_mime.clone()
    }

    pub fn builtin_extension_count(&self) -> usize {
        builtin_pairs().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guess_type_pdf() {
        let reg = MimeRegistry::builtin();
        let g = reg.guess_type("report.Q4.pdf", false);
        assert_eq!(g.mime.as_deref(), Some("application/pdf"));
    }
}
