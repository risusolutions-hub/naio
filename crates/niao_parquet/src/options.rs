use parquet::basic::Compression;

/// Read options (column projection, row limit).
#[derive(Clone, Debug, Default)]
pub struct ReadOptions {
    pub columns: Option<Vec<String>>,
    pub rows: Option<usize>,
}

/// Write options (compression, row group size).
#[derive(Clone, Debug)]
pub struct WriteOptions {
    pub compression: Compression,
    pub row_group_size: usize,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            compression: Compression::SNAPPY,
            row_group_size: 1_048_576,
        }
    }
}

impl WriteOptions {
    pub fn compression_from_str(s: &str) -> Option<Compression> {
        match s.to_ascii_lowercase().as_str() {
            "snappy" | "snap" => Some(Compression::SNAPPY),
            "gzip" | "gz" => Some(Compression::GZIP(Default::default())),
            "zstd" | "zst" => Some(Compression::ZSTD(Default::default())),
            "none" | "uncompressed" | "raw" => Some(Compression::UNCOMPRESSED),
            "lz4" => Some(Compression::LZ4),
            "brotli" | "br" => Some(Compression::BROTLI(Default::default())),
            _ => None,
        }
    }

    pub fn compression_name(c: Compression) -> &'static str {
        match c {
            Compression::UNCOMPRESSED => "none",
            Compression::SNAPPY => "snappy",
            Compression::GZIP(_) => "gzip",
            Compression::LZO => "lzo",
            Compression::BROTLI(_) => "brotli",
            Compression::LZ4 => "lz4",
            Compression::ZSTD(_) => "zstd",
            _ => "other",
        }
    }
}
