use crate::error::{ZipError, ZipResult};
use zip::CompressionMethod;

/// Compression method names exposed to Niao (`nzip.STORED`, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionName {
    Stored,
    Deflated,
    Bzip2,
    Lzma,
    Zstd,
    Aes,
}

impl CompressionName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stored => "stored",
            Self::Deflated => "deflated",
            Self::Bzip2 => "bzip2",
            Self::Lzma => "lzma",
            Self::Zstd => "zstd",
            Self::Aes => "aes",
        }
    }

    pub fn parse(s: &str) -> ZipResult<Self> {
        match s.to_ascii_lowercase().as_str() {
            "stored" | "store" | "none" | "0" => Ok(Self::Stored),
            "deflated" | "deflate" | "8" => Ok(Self::Deflated),
            "bzip2" | "bzip" | "12" => Ok(Self::Bzip2),
            "lzma" | "14" => Ok(Self::Lzma),
            "zstd" | "zstandard" | "93" => Ok(Self::Zstd),
            "aes" => Ok(Self::Aes),
            other => Err(ZipError::Archive(format!(
                "unknown compression method: {other}"
            ))),
        }
    }

    pub fn to_method(self) -> CompressionMethod {
        match self {
            Self::Stored => CompressionMethod::Stored,
            Self::Deflated => CompressionMethod::Deflated,
            Self::Bzip2 => CompressionMethod::Bzip2,
            Self::Lzma => CompressionMethod::Lzma,
            Self::Zstd => CompressionMethod::Zstd,
            Self::Aes => CompressionMethod::Aes,
        }
    }

    pub fn from_method(method: CompressionMethod) -> Self {
        match method {
            CompressionMethod::Stored => Self::Stored,
            CompressionMethod::Deflated => Self::Deflated,
            CompressionMethod::Bzip2 => Self::Bzip2,
            CompressionMethod::Lzma => Self::Lzma,
            CompressionMethod::Zstd => Self::Zstd,
            CompressionMethod::Aes => Self::Aes,
            _ => Self::Stored,
        }
    }
}

pub const DEFAULT_LEVEL: i32 = 6;
