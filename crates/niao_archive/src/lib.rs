//! Native archive formats: deflate, gzip, tar, zip.

pub mod deflate;
pub mod error;
mod adler32;
mod bitstream;
mod crc32;
pub mod gzip;
pub mod tar;
pub mod zip;

pub use deflate::{deflate, inflate, zlib_encode};
pub use error::{Error, Result};
pub use gzip::{decode as gzip_decode, encode as gzip_encode};
pub use tar::{Archive as TarArchive, Entry as TarEntry};
pub use zip::{ZipArchive, ZipEntry, ZipMethod};
