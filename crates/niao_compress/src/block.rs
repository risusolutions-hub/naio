use crate::codec::{Codec, CompressOpts, DecompressOpts, FrameInfo};
use crate::error::{check_len, CompressError, CompressResult, MAX_BYTES};
use std::io::{Read, Write};

fn max_output(opts: &DecompressOpts) -> usize {
    if opts.max_output == 0 {
        MAX_BYTES
    } else {
        opts.max_output.min(MAX_BYTES)
    }
}

/// One-shot block compression.
pub fn compress(data: &[u8], codec: Codec, opts: &CompressOpts) -> CompressResult<Vec<u8>> {
    check_len(data.len())?;
    let level = codec.validate_level(opts.level)?;
    match codec {
        Codec::Zstd => compress_zstd(data, level, opts),
        Codec::Lz4 => compress_lz4(data, opts),
        Codec::Brotli => compress_brotli(data, level, opts),
        Codec::Xz => compress_xz(data, level),
    }
}

/// One-shot block decompression.
pub fn decompress(data: &[u8], codec: Codec, opts: &DecompressOpts) -> CompressResult<Vec<u8>> {
    check_len(data.len())?;
    match codec {
        Codec::Zstd => decompress_zstd(data, opts),
        Codec::Lz4 => decompress_lz4(data, opts),
        Codec::Brotli => decompress_brotli(data, opts),
        Codec::Xz => decompress_xz(data, opts),
    }
}

/// Decompress using auto-detected codec from frame header; falls back to `hint` when needed.
pub fn decompress_auto(
    data: &[u8],
    hint: Option<Codec>,
    opts: &DecompressOpts,
) -> CompressResult<Vec<u8>> {
    let codec = Codec::detect(data).or(hint).ok_or_else(|| {
        CompressError::Corrupt("cannot detect compression codec from frame header".into())
    })?;
    decompress(data, codec, opts)
}

/// Inspect compressed frame metadata without full decompression.
pub fn frame_info(data: &[u8], codec: Option<Codec>) -> CompressResult<FrameInfo> {
    check_len(data.len())?;
    let detected = codec.or_else(|| Codec::detect(data)).ok_or_else(|| {
        CompressError::Corrupt("cannot detect compression codec from frame header".into())
    })?;
    match detected {
        Codec::Zstd => Ok(FrameInfo {
            codec: Codec::Zstd,
            content_size: None,
            compressed_size: data.len(),
            has_checksum: false,
        }),
        Codec::Lz4 => Ok(FrameInfo {
            codec: Codec::Lz4,
            content_size: None,
            compressed_size: data.len(),
            has_checksum: false,
        }),
        Codec::Brotli => Ok(FrameInfo {
            codec: Codec::Brotli,
            content_size: None,
            compressed_size: data.len(),
            has_checksum: false,
        }),
        Codec::Xz => xz_frame_info(data),
    }
}

/// Quick validity check — trial decode succeeds.
pub fn is_valid(data: &[u8], codec: Codec) -> bool {
    if data.is_empty() {
        return false;
    }
    decompress(
        data,
        codec,
        &DecompressOpts {
            verify_content_size: false,
            ..DecompressOpts::default()
        },
    )
    .is_ok()
}

fn compress_zstd(data: &[u8], level: i32, opts: &CompressOpts) -> CompressResult<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut enc = zstd::stream::write::Encoder::new(&mut out, level)
            .map_err(|e| CompressError::Other(e.to_string()))?;
        if opts.content_size {
            enc.include_contentsize(true)
                .map_err(|e| CompressError::Other(e.to_string()))?;
        }
        if opts.checksum {
            enc.include_checksum(true)
                .map_err(|e| CompressError::Other(e.to_string()))?;
        }
        enc.write_all(data)
            .map_err(|e| CompressError::Other(e.to_string()))?;
        enc.finish()
            .map_err(|e| CompressError::Other(e.to_string()))?;
    }
    Ok(out)
}

fn decompress_zstd(data: &[u8], opts: &DecompressOpts) -> CompressResult<Vec<u8>> {
    let limit = max_output(opts);
    let out = zstd::stream::decode_all(data).map_err(|e| CompressError::Corrupt(e.to_string()))?;
    if out.len() > limit {
        return Err(CompressError::TooLarge(out.len()));
    }
    Ok(out)
}

fn compress_lz4(data: &[u8], opts: &CompressOpts) -> CompressResult<Vec<u8>> {
    let mut frame_info = lz4_flex::frame::FrameInfo::new();
    if opts.content_size {
        frame_info = frame_info.content_size(Some(data.len() as u64));
    }
    if opts.checksum {
        frame_info = frame_info.block_checksums(true);
    }
    if opts.independent_blocks {
        frame_info = frame_info.block_mode(lz4_flex::frame::BlockMode::Independent);
    }

    let mut out = Vec::new();
    {
        let mut enc = lz4_flex::frame::FrameEncoder::with_frame_info(frame_info, &mut out);
        enc.write_all(data)
            .map_err(|e| CompressError::Other(e.to_string()))?;
        enc.finish()
            .map_err(|e| CompressError::Other(e.to_string()))?;
    }
    Ok(out)
}

fn decompress_lz4(data: &[u8], opts: &DecompressOpts) -> CompressResult<Vec<u8>> {
    let limit = max_output(opts);
    let mut dec = lz4_flex::frame::FrameDecoder::new(data);
    let mut out = Vec::new();
    dec.read_to_end(&mut out)
        .map_err(|e| CompressError::Corrupt(e.to_string()))?;
    if out.len() > limit {
        return Err(CompressError::TooLarge(out.len()));
    }
    Ok(out)
}

fn compress_brotli(data: &[u8], level: i32, opts: &CompressOpts) -> CompressResult<Vec<u8>> {
    let mut params = brotli::enc::BrotliEncoderParams::default();
    params.quality = level;
    if opts.window_log >= 10 && opts.window_log <= 24 {
        params.lgwin = opts.window_log as i32;
    }
    let mut out = Vec::new();
    brotli::BrotliCompress(&mut std::io::Cursor::new(data), &mut out, &params)
        .map_err(|e| CompressError::Other(format!("{e:?}")))?;
    Ok(out)
}

fn decompress_brotli(data: &[u8], opts: &DecompressOpts) -> CompressResult<Vec<u8>> {
    let limit = max_output(opts);
    let mut out = Vec::with_capacity(data.len().min(limit).max(4096));
    let mut input = std::io::Cursor::new(data);
    brotli::BrotliDecompress(&mut input, &mut out)
        .map_err(|e| CompressError::Corrupt(format!("{e:?}")))?;
    if out.len() > limit {
        return Err(CompressError::TooLarge(out.len()));
    }
    Ok(out)
}

fn compress_xz(data: &[u8], level: i32) -> CompressResult<Vec<u8>> {
    let mut out = Vec::new();
    {
        let preset = level.clamp(0, 9) as u32;
        let mut enc = xz2::write::XzEncoder::new(&mut out, preset);
        enc.write_all(data)
            .map_err(|e| CompressError::Other(e.to_string()))?;
        enc.finish()
            .map_err(|e| CompressError::Other(e.to_string()))?;
    }
    Ok(out)
}

fn decompress_xz(data: &[u8], opts: &DecompressOpts) -> CompressResult<Vec<u8>> {
    let limit = max_output(opts);
    let mut dec = xz2::read::XzDecoder::new(data);
    let mut out = Vec::new();
    let mut buf = [0u8; 64 * 1024];
    let mut total = 0usize;
    loop {
        let n = dec
            .read(&mut buf)
            .map_err(|e| CompressError::Corrupt(e.to_string()))?;
        if n == 0 {
            break;
        }
        total += n;
        if total > limit {
            return Err(CompressError::TooLarge(total));
        }
        out.extend_from_slice(&buf[..n]);
    }
    Ok(out)
}

fn xz_frame_info(data: &[u8]) -> CompressResult<FrameInfo> {
    if data.len() < 6 || data[0..6] != [0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00] {
        return Err(CompressError::Corrupt("invalid xz header".into()));
    }
    Ok(FrameInfo {
        codec: Codec::Xz,
        content_size: None,
        compressed_size: data.len(),
        has_checksum: true,
    })
}

/// Parallel batch compression of independent blocks.
pub fn parallel_compress(
    blocks: &[Vec<u8>],
    codec: Codec,
    opts: &CompressOpts,
    threads: usize,
) -> CompressResult<Vec<Vec<u8>>> {
    use niao_parallel::map;
    let results = map(blocks, threads, |b| compress(b, codec, opts));
    results.into_iter().collect()
}

/// Parallel batch decompression of independent blocks.
pub fn parallel_decompress(
    blocks: &[Vec<u8>],
    codec: Codec,
    opts: &DecompressOpts,
    threads: usize,
) -> CompressResult<Vec<Vec<u8>>> {
    use niao_parallel::map;
    let results = map(blocks, threads, |b| decompress(b, codec, opts));
    results.into_iter().collect()
}

/// Compress a file to another file.
pub fn compress_file(
    src: &str,
    dst: &str,
    codec: Codec,
    opts: &CompressOpts,
) -> CompressResult<()> {
    let data = std::fs::read(src).map_err(|e| CompressError::Io(e.to_string()))?;
    let out = compress(&data, codec, opts)?;
    std::fs::write(dst, out).map_err(|e| CompressError::Io(e.to_string()))?;
    Ok(())
}

/// Decompress a file to another file.
pub fn decompress_file(
    src: &str,
    dst: &str,
    codec: Codec,
    opts: &DecompressOpts,
) -> CompressResult<()> {
    let data = std::fs::read(src).map_err(|e| CompressError::Io(e.to_string()))?;
    let out = decompress(&data, codec, opts)?;
    std::fs::write(dst, out).map_err(|e| CompressError::Io(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(data: &[u8], codec: Codec) {
        let opts = CompressOpts::for_codec(codec);
        let c = compress(data, codec, &opts).unwrap();
        assert!(!c.is_empty());
        let d = decompress(&c, codec, &DecompressOpts::default()).unwrap();
        assert_eq!(d, data);
    }

    #[test]
    fn zstd_roundtrip() {
        roundtrip(b"hello zstd world", Codec::Zstd);
        roundtrip(&vec![0xABu8; 64 * 1024], Codec::Zstd);
    }

    #[test]
    fn lz4_roundtrip() {
        roundtrip(b"hello lz4", Codec::Lz4);
    }

    #[test]
    fn brotli_roundtrip() {
        roundtrip(b"hello brotli", Codec::Brotli);
    }

    #[test]
    fn xz_roundtrip() {
        roundtrip(b"hello xz", Codec::Xz);
    }

    #[test]
    fn detect_zstd() {
        let c = compress(b"x", Codec::Zstd, &CompressOpts::for_codec(Codec::Zstd)).unwrap();
        assert_eq!(Codec::detect(&c), Some(Codec::Zstd));
    }

    #[test]
    fn invalid_level() {
        let err = compress(
            b"x",
            Codec::Zstd,
            &CompressOpts {
                level: 99,
                ..Default::default()
            },
        );
        assert!(err.is_err());
    }

    #[test]
    fn parallel_blocks() {
        let blocks: Vec<Vec<u8>> = (0..8).map(|i| format!("block-{i}").into_bytes()).collect();
        let opts = CompressOpts::for_codec(Codec::Lz4);
        let compressed = parallel_compress(&blocks, Codec::Lz4, &opts, 4).unwrap();
        assert_eq!(compressed.len(), 8);
        let back =
            parallel_decompress(&compressed, Codec::Lz4, &DecompressOpts::default(), 4).unwrap();
        assert_eq!(back, blocks);
    }
}
