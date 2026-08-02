use crate::compression::CompressionName;
use zip::read::ZipFile;

/// Metadata for one archive entry (~`zipfile.ZipInfo`).
#[derive(Debug, Clone)]
pub struct EntryInfo {
    pub name: String,
    pub size: u64,
    pub compressed_size: u64,
    pub compression: CompressionName,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub crc32: u32,
    pub modified_unix: Option<i64>,
    pub encrypted: bool,
    pub comment: Option<String>,
}

impl EntryInfo {
    pub fn from_file(name: String, file: &ZipFile<'_>) -> Self {
        let modified_unix = file
            .last_modified()
            .and_then(|dt| dt.to_time().ok().map(|t| t.unix_timestamp()));
        Self {
            name,
            size: file.size(),
            compressed_size: file.compressed_size(),
            compression: CompressionName::from_method(file.compression()),
            is_dir: file.is_dir(),
            is_symlink: file.is_symlink(),
            crc32: file.crc32(),
            modified_unix,
            encrypted: file.encrypted(),
            comment: {
                let c = file.comment();
                if c.is_empty() {
                    None
                } else {
                    Some(c.to_string())
                }
            },
        }
    }
}
