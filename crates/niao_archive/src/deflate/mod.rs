pub mod encode;
pub mod inflate;

pub use encode::{deflate, gzip_encode, zlib_encode};
pub use inflate::inflate;
