//! `niao_zip` — ZIP archives: read/write, streaming, per-entry compression,
//! AES encryption (~Python `zipfile` subset).

mod archive;
mod compression;
mod error;
mod extract;
mod info;

pub use archive::{
    is_zipfile_bytes, is_zipfile_path, EntryWriteOptions, ExtractOptions, OpenOptions,
    WriteOptions, ZipHandle, ZipReader, ZipReaderMem, ZipWriterHandle,
};
pub use compression::{CompressionName, DEFAULT_LEVEL};
pub use error::{ZipError, ZipResult};
pub use extract::{extract_all, extract_one, safe_join};
pub use info::EntryInfo;
