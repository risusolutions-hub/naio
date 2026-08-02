use crate::error::{CompressError, CompressResult};

/// Supported compression codecs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Codec {
    Zstd,
    Lz4,
    Brotli,
    Xz,
}

impl Codec {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Zstd => "zstd",
            Self::Lz4 => "lz4",
            Self::Brotli => "brotli",
            Self::Xz => "xz",
        }
    }

    pub fn parse(s: &str) -> CompressResult<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "zstd" | "zstandard" | "zst" => Ok(Self::Zstd),
            "lz4" | "lz4frame" => Ok(Self::Lz4),
            "brotli" | "br" => Ok(Self::Brotli),
            "xz" | "lzma" => Ok(Self::Xz),
            other => Err(CompressError::UnknownCodec(other.to_string())),
        }
    }

    pub fn default_level(self) -> i32 {
        match self {
            Self::Zstd => 3,
            Self::Lz4 => 0,
            Self::Brotli => 6,
            Self::Xz => 6,
        }
    }

    pub fn level_range(self) -> (i32, i32) {
        match self {
            Self::Zstd => (1, 22),
            Self::Lz4 => (0, 12),
            Self::Brotli => (0, 11),
            Self::Xz => (0, 9),
        }
    }

    pub fn validate_level(self, level: i32) -> CompressResult<i32> {
        let (lo, hi) = self.level_range();
        if level < lo || level > hi {
            return Err(CompressError::InvalidLevel {
                codec: self.as_str().into(),
                level,
            });
        }
        Ok(level)
    }

    /// Sniff codec from magic bytes; returns `None` when unknown.
    pub fn detect(data: &[u8]) -> Option<Self> {
        if data.len() >= 4 && data[0..4] == [0x28, 0xB5, 0x2F, 0xFD] {
            return Some(Self::Zstd);
        }
        if data.len() >= 4 && data[0..4] == [0x04, 0x22, 0x4D, 0x18] {
            return Some(Self::Lz4);
        }
        if !data.is_empty() {
            // Brotli has no fixed magic; common stream starts with window bits in low nibble.
            // XZ magic: FD 37 7A 58 5A 00
            if data.len() >= 6 && data[0..6] == [0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00] {
                return Some(Self::Xz);
            }
        }
        None
    }
}

/// Compression options shared across codecs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompressOpts {
    pub level: i32,
    /// Include uncompressed size in frame metadata when supported (zstd, lz4 frame).
    pub content_size: bool,
    /// Enable checksum in frame when supported (zstd, lz4 frame).
    pub checksum: bool,
    /// Brotli window log2 (10..=24); 0 = default (22).
    pub window_log: u8,
    /// LZ4 block mode (independent blocks) vs linked blocks.
    pub independent_blocks: bool,
}

impl Default for CompressOpts {
    fn default() -> Self {
        Self {
            level: Codec::Zstd.default_level(),
            content_size: true,
            checksum: false,
            window_log: 0,
            independent_blocks: false,
        }
    }
}

impl CompressOpts {
    pub fn for_codec(codec: Codec) -> Self {
        Self {
            level: codec.default_level(),
            ..Self::default()
        }
    }
}

/// Decompression options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecompressOpts {
    /// Maximum allowed decompressed output size (0 = use `MAX_BYTES`).
    pub max_output: usize,
    /// When set, verify output length matches declared content size.
    pub verify_content_size: bool,
}

impl Default for DecompressOpts {
    fn default() -> Self {
        Self {
            max_output: 0,
            verify_content_size: true,
        }
    }
}

/// Metadata extracted from a compressed frame header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameInfo {
    pub codec: Codec,
    pub content_size: Option<u64>,
    pub compressed_size: usize,
    pub has_checksum: bool,
}
