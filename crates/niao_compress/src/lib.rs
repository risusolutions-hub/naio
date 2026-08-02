//! Modern compression for Niao: zstd, lz4, brotli, xz — block and stream APIs.
//!
//! ~zstandard, lz4, brotli subset (extends archive's gzip/deflate).

mod block;
mod codec;
mod error;
mod stream;

pub use block::{
    compress, compress_file, decompress, decompress_auto, decompress_file, frame_info, is_valid,
    parallel_compress, parallel_decompress,
};
pub use codec::{Codec, CompressOpts, DecompressOpts, FrameInfo};
pub use error::{check_len, CompressError, CompressResult, MAX_BYTES};
pub use stream::{CompressStream, DecompressStream};

#[cfg(test)]
mod integration {
    use super::*;

    #[test]
    fn stream_zstd_incremental() {
        let opts = CompressOpts::for_codec(Codec::Zstd);
        let mut enc = CompressStream::new(Codec::Zstd, opts).unwrap();
        enc.write(b"hello ").unwrap();
        enc.write(b"world").unwrap();
        let compressed = enc.finish().unwrap();

        let mut dec = DecompressStream::new(Codec::Zstd, DecompressOpts::default()).unwrap();
        dec.write(&compressed).unwrap();
        let out = dec.finish().unwrap();
        assert_eq!(out, b"hello world");
    }
}
