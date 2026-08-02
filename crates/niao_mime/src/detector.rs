//! Configurable MIME detector with custom magic rules and extension overrides.

use crate::error::{MimeError, MimeResult};
use crate::extmap::MimeRegistry;
use crate::guess::{from_bytes, sniff_path, SniffOpts};
use crate::magic::{parse_hex_magic, CustomMagic};
use crate::types::{FileKind, MimeMatch};

#[derive(Debug, Clone)]
pub struct Detector {
    pub registry: MimeRegistry,
    pub custom_magic: Vec<CustomMagic>,
    pub sniff_opts: SniffOpts,
}

impl Default for Detector {
    fn default() -> Self {
        Self {
            registry: MimeRegistry::builtin(),
            custom_magic: Vec::new(),
            sniff_opts: SniffOpts::default(),
        }
    }
}

impl Detector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_sniff_bytes(mut self, n: usize) -> Self {
        self.sniff_opts.max_bytes = n;
        self
    }

    pub fn detect_bytes(&self, data: &[u8]) -> Option<MimeMatch> {
        from_bytes(data, &self.custom_magic)
    }

    pub fn sniff_file(&self, path: &std::path::Path) -> MimeResult<Option<MimeMatch>> {
        sniff_path(path, &self.registry, &self.sniff_opts, &self.custom_magic)
    }

    pub fn add_type(&mut self, mime: &str, ext: &str, strict: bool) -> MimeResult<bool> {
        self.registry.add_type(mime, ext, strict)
    }

    pub fn add_magic(
        &mut self,
        bytes: &[u8],
        mime: &str,
        ext: Option<&str>,
        offset: usize,
        mask: Option<&[u8]>,
        kind: Option<FileKind>,
        priority: Option<u8>,
    ) -> MimeResult<()> {
        if offset.saturating_add(bytes.len()) > MAX_RULE_WINDOW {
            return Err(MimeError::OffsetOutOfRange {
                offset,
                len: bytes.len(),
            });
        }
        if let Some(m) = mask {
            if m.len() != bytes.len() {
                return Err(MimeError::InvalidMagic(
                    "mask length must match magic bytes".into(),
                ));
            }
        }
        let norm_mime = mime.trim().to_ascii_lowercase();
        if !norm_mime.contains('/') {
            return Err(MimeError::InvalidMime(mime.into()));
        }
        let ext = ext
            .map(|e| crate::extmap::normalize_ext(e))
            .transpose()?
            .unwrap_or_else(|| "bin".into());
        self.custom_magic.push(CustomMagic {
            bytes: bytes.to_vec(),
            mask: mask.map(|m| m.to_vec()),
            offset,
            mime: norm_mime,
            ext,
            kind: kind.unwrap_or(FileKind::Application),
            priority: priority.unwrap_or(80),
        });
        Ok(())
    }

    pub fn add_magic_hex(
        &mut self,
        hex: &str,
        mime: &str,
        ext: Option<&str>,
        offset: usize,
    ) -> MimeResult<()> {
        let bytes = parse_hex_magic(hex)?;
        self.add_magic(&bytes, mime, ext, offset, None, None, None)
    }

    pub fn magic_rule_count(&self) -> usize {
        self.custom_magic.len()
    }
}

const MAX_RULE_WINDOW: usize = 4096;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_magic() {
        let mut d = Detector::new();
        d.add_magic_hex("CAFEBABE", "application/x-custom", Some("cust"), 0)
            .unwrap();
        let data = [0xCA, 0xFE, 0xBA, 0xBE];
        let m = d.detect_bytes(&data).unwrap();
        assert_eq!(m.mime, "application/x-custom");
    }
}
