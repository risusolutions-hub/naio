//! `niao_tar` — POSIX tar read/write with gzip and zstd compression (~Python `tarfile`).

pub mod error;
pub mod format;
pub mod info;
mod io_util;
pub mod read;
pub mod write;

pub use error::{Result, TarError};
pub use format::{detect_compression, is_tar_path, parse_mode, Compression, OpenMode};
pub use info::{EntryInfo, EntryKind};
pub use read::{is_tar_bytes, is_tar_file, ReadOpts, TarReader, TarStreamReader, MAX_ENTRY_BYTES};
pub use write::{
    create_archive, extract_all, extract_member, pack_tree, unpack, AddOpts, ExtractOpts,
    TarWriter, WriteOpts,
};
