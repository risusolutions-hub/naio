//! Native ncompress standard library — modern compression: zstd, lz4, brotli, xz.
//! Block and stream APIs (~zstandard, lz4, brotli subset; extends archive gzip/deflate).
//!
//! Import with `import "ncompress"` (or `import "std/ncompress"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_compress::{
    compress, compress_file, decompress, decompress_auto, decompress_file, frame_info, is_valid,
    parallel_compress, parallel_decompress, Codec, CompressError, CompressOpts, CompressStream,
    DecompressOpts, DecompressStream, FrameInfo, MAX_BYTES,
};
use niao_errors::codes;
use niao_parallel::available_threads;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

enum StreamHandle {
    Compress(CompressStream),
    Decompress(DecompressStream),
}

thread_local! {
    static STREAMS: RefCell<HashMap<i64, StreamHandle>> = RefCell::new(HashMap::new());
    static NEXT_HANDLE: RefCell<i64> = const { RefCell::new(1) };
}

fn new_handle() -> i64 {
    NEXT_HANDLE.with(|h| {
        let mut h = h.borrow_mut();
        let id = *h;
        *h += 1;
        id
    })
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E3556_NCOMPRESS_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E3554_NCOMPRESS_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E3554_NCOMPRESS_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn ncompress_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E3555_NCOMPRESS_ERROR, "ncompress_error", msg.into(), span)
}

fn corrupt_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E3558_NCOMPRESS_CORRUPT, "ncompress_error", msg.into(), span)
}

fn invalid_handle(span: Span, id: i64) -> ValueRef {
    error_value(
        codes::E3557_NCOMPRESS_INVALID_HANDLE,
        "ncompress_error",
        format!("invalid or closed ncompress stream handle {id}"),
        span,
    )
}

fn map_err(span: Span, err: CompressError) -> ValueRef {
    let code = match &err {
        CompressError::Corrupt(_) | CompressError::SizeMismatch { .. } => codes::E3558_NCOMPRESS_CORRUPT,
        _ => codes::E3555_NCOMPRESS_ERROR,
    };
    error_value(code, "ncompress_error", err.message(), span)
}

fn bytes_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<u8>> {
    match &*args[idx].borrow() {
        Value::ByteArray(b) => Ok(b.clone()),
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects byte[] or string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn string_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn int_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an int as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn optional_object_arg(args: &[ValueRef], idx: usize) -> Option<HashMap<String, ValueRef>> {
    if args.len() <= idx {
        return None;
    }
    match &*args[idx].borrow() {
        Value::Object(map) => Some(map.clone()),
        Value::Nil => None,
        _ => None,
    }
}

fn bool_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: bool) -> bool {
    let Some(map) = map else {
        return default;
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Bool(b)) => b,
        Some(Value::Int(n)) => n != 0,
        Some(Value::String(s)) => matches!(s.as_str(), "true" | "1" | "yes" | "on"),
        _ => default,
    }
}

fn int_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: i64) -> i64 {
    let Some(map) = map else {
        return default;
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Int(n)) => n,
        _ => default,
    }
}

fn parse_codec(s: &str, span: Span) -> Result<Codec, ValueRef> {
    Codec::parse(s).map_err(|e| map_err(span, e))
}

fn compress_opts_from_map(codec: Codec, map: Option<&HashMap<String, ValueRef>>) -> CompressOpts {
    let mut opts = CompressOpts::for_codec(codec);
    opts.level = int_field(map, "level", opts.level as i64) as i32;
    opts.content_size = bool_field(map, "content_size", opts.content_size);
    opts.checksum = bool_field(map, "checksum", opts.checksum);
    opts.window_log = int_field(map, "window_log", opts.window_log as i64) as u8;
    opts.independent_blocks = bool_field(map, "independent_blocks", opts.independent_blocks);
    opts
}

fn decompress_opts_from_map(map: Option<&HashMap<String, ValueRef>>) -> DecompressOpts {
    DecompressOpts {
        max_output: int_field(map, "max_output", 0).max(0) as usize,
        verify_content_size: bool_field(map, "verify_content_size", true),
    }
}

fn bytes_result(bytes: Vec<u8>) -> ValueRef {
    Value::ByteArray(bytes).ref_cell()
}

fn frame_info_object(info: FrameInfo) -> Value {
    let mut m = HashMap::new();
    m.insert("codec".into(), Value::String(info.codec.as_str().into()).ref_cell());
    m.insert(
        "compressed_size".into(),
        Value::Int(info.compressed_size as i64).ref_cell(),
    );
    m.insert(
        "has_checksum".into(),
        Value::Bool(info.has_checksum).ref_cell(),
    );
    match info.content_size {
        Some(n) => m.insert("content_size".into(), Value::Int(n as i64).ref_cell()),
        None => m.insert("content_size".into(), Value::Nil.ref_cell()),
    };
    Value::Object(m)
}

fn bytes_list_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<Vec<u8>>> {
    match &*args[idx].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::ByteArray(b) => out.push(b.clone()),
                    Value::String(s) => out.push(s.as_bytes().to_vec()),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() expects byte[][] at argument {}; item {} is {}",
                                idx + 1,
                                i + 1,
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(out)
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() expects byte[][] as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn bytes_list_result(blocks: Vec<Vec<u8>>) -> ValueRef {
    Value::Array(blocks.into_iter().map(|b| bytes_result(b)).collect()).ref_cell()
}

// >>> ncompress.compress(byte_array[1, 2, 3], "zstd")
// => compressed bytes
fn ncompress_compress(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ncompress_compress", span)?;
    let data = bytes_arg(args, 0, "ncompress_compress", span)?;
    let codec = match parse_codec(&string_arg(args, 1, "ncompress_compress", span)?, span) {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    let opts = compress_opts_from_map(codec, optional_object_arg(args, 2).as_ref());
    match compress(&data, codec, &opts) {
        Ok(out) => Ok(bytes_result(out)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> ncompress.decompress(compressed, "zstd")
// => original bytes
fn ncompress_decompress(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ncompress_decompress", span)?;
    let data = bytes_arg(args, 0, "ncompress_decompress", span)?;
    let codec = match parse_codec(&string_arg(args, 1, "ncompress_decompress", span)?, span) {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    let opts = decompress_opts_from_map(optional_object_arg(args, 2).as_ref());
    match decompress(&data, codec, &opts) {
        Ok(out) => Ok(bytes_result(out)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> ncompress.decompress_auto(compressed)
// => original bytes (codec auto-detected)
fn ncompress_decompress_auto(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "ncompress_decompress_auto", span)?;
    let data = bytes_arg(args, 0, "ncompress_decompress_auto", span)?;
    let hint = if args.len() >= 2 {
        match &*args[1].borrow() {
            Value::Nil => None,
            Value::String(s) => match parse_codec(s, span) {
                Ok(c) => Some(c),
                Err(e) => return Ok(e),
            },
            other => {
                return Err(type_err(
                    span,
                    format!(
                        "ncompress_decompress_auto() expects nil or codec string as argument 2, got {}",
                        other.type_name()
                    ),
                ));
            }
        }
    } else {
        None
    };
    let opts = decompress_opts_from_map(optional_object_arg(args, 2).as_ref());
    match decompress_auto(&data, hint, &opts) {
        Ok(out) => Ok(bytes_result(out)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> ncompress.detect(compressed)
// => "zstd"
fn ncompress_detect(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncompress_detect", span)?;
    let data = bytes_arg(args, 0, "ncompress_detect", span)?;
    match Codec::detect(&data) {
        Some(c) => Ok(Value::String(c.as_str().into()).ref_cell()),
        None => Ok(Value::Nil.ref_cell()),
    }
}

fn ncompress_frame_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ncompress_frame_info", span)?;
    let data = bytes_arg(args, 0, "ncompress_frame_info", span)?;
    let codec = if args.len() >= 2 {
        match &*args[1].borrow() {
            Value::Nil => None,
            Value::String(s) => match parse_codec(s, span) {
                Ok(c) => Some(c),
                Err(e) => return Ok(e),
            },
            other => {
                return Err(type_err(
                    span,
                    format!(
                        "ncompress_frame_info() expects nil or codec string as argument 2, got {}",
                        other.type_name()
                    ),
                ));
            }
        }
    } else {
        None
    };
    match frame_info(&data, codec) {
        Ok(info) => Ok(frame_info_object(info).ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> ncompress.is_valid(compressed, "zstd")
// => true
fn ncompress_is_valid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncompress_is_valid", span)?;
    let data = bytes_arg(args, 0, "ncompress_is_valid", span)?;
    let codec = match parse_codec(&string_arg(args, 1, "ncompress_is_valid", span)?, span) {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    Ok(Value::Bool(is_valid(&data, codec)).ref_cell())
}

fn ncompress_compress_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "ncompress_compress_file", span)?;
    let src = string_arg(args, 0, "ncompress_compress_file", span)?;
    let dst = string_arg(args, 1, "ncompress_compress_file", span)?;
    let codec = match parse_codec(&string_arg(args, 2, "ncompress_compress_file", span)?, span) {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    let opts = compress_opts_from_map(codec, optional_object_arg(args, 3).as_ref());
    match compress_file(&src, &dst, codec, &opts) {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    }
}

fn ncompress_decompress_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "ncompress_decompress_file", span)?;
    let src = string_arg(args, 0, "ncompress_decompress_file", span)?;
    let dst = string_arg(args, 1, "ncompress_decompress_file", span)?;
    let codec = match parse_codec(&string_arg(args, 2, "ncompress_decompress_file", span)?, span) {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    let opts = decompress_opts_from_map(optional_object_arg(args, 3).as_ref());
    match decompress_file(&src, &dst, codec, &opts) {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> ncompress.parallel_compress([block1, block2], "lz4")
// => [compressed1, compressed2]
fn ncompress_parallel_compress(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ncompress_parallel_compress", span)?;
    let blocks = bytes_list_arg(args, 0, "ncompress_parallel_compress", span)?;
    let codec = match parse_codec(&string_arg(args, 1, "ncompress_parallel_compress", span)?, span) {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    let map = optional_object_arg(args, 2);
    let opts = compress_opts_from_map(codec, map.as_ref());
    let threads = int_field(map.as_ref(), "threads", available_threads() as i64).max(1) as usize;
    match parallel_compress(&blocks, codec, &opts, threads) {
        Ok(out) => Ok(bytes_list_result(out)),
        Err(e) => Ok(map_err(span, e)),
    }
}

fn ncompress_parallel_decompress(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ncompress_parallel_decompress", span)?;
    let blocks = bytes_list_arg(args, 0, "ncompress_parallel_decompress", span)?;
    let codec = match parse_codec(
        &string_arg(args, 1, "ncompress_parallel_decompress", span)?,
        span,
    ) {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    let map = optional_object_arg(args, 2);
    let opts = decompress_opts_from_map(map.as_ref());
    let threads = int_field(map.as_ref(), "threads", available_threads() as i64).max(1) as usize;
    match parallel_decompress(&blocks, codec, &opts, threads) {
        Ok(out) => Ok(bytes_list_result(out)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> ncompress.stream_open("compress", "zstd")
// => handle int
fn ncompress_stream_open(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ncompress_stream_open", span)?;
    let mode = string_arg(args, 0, "ncompress_stream_open", span)?;
    let codec = match parse_codec(&string_arg(args, 1, "ncompress_stream_open", span)?, span) {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    let map = optional_object_arg(args, 2);
    let handle = match mode.to_ascii_lowercase().as_str() {
        "compress" | "enc" | "encoder" => {
            let opts = compress_opts_from_map(codec, map.as_ref());
            match CompressStream::new(codec, opts) {
                Ok(s) => StreamHandle::Compress(s),
                Err(e) => return Ok(map_err(span, e)),
            }
        }
        "decompress" | "dec" | "decoder" => {
            let opts = decompress_opts_from_map(map.as_ref());
            match DecompressStream::new(codec, opts) {
                Ok(s) => StreamHandle::Decompress(s),
                Err(e) => return Ok(map_err(span, e)),
            }
        }
        other => {
            return Err(type_err(
                span,
                format!(
                    "ncompress_stream_open() mode must be 'compress' or 'decompress', got '{other}'"
                ),
            ));
        }
    };
    let id = new_handle();
    STREAMS.with(|m| m.borrow_mut().insert(id, handle));
    Ok(Value::Int(id).ref_cell())
}

fn ncompress_stream_write(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncompress_stream_write", span)?;
    let id = int_arg(args, 0, "ncompress_stream_write", span)?;
    let chunk = bytes_arg(args, 1, "ncompress_stream_write", span)?;
    STREAMS.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(StreamHandle::Compress(s)) => match s.write(&chunk) {
                Ok(emitted) => Ok(bytes_result(emitted)),
                Err(e) => Ok(map_err(span, e)),
            },
            Some(StreamHandle::Decompress(s)) => match s.write(&chunk) {
                Ok(()) => Ok(Value::Bool(true).ref_cell()),
                Err(e) => Ok(map_err(span, e)),
            },
            Some(_) => Ok(invalid_handle(span, id)),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

fn ncompress_stream_read(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ncompress_stream_read", span)?;
    let id = int_arg(args, 0, "ncompress_stream_read", span)?;
    let max = if args.len() >= 2 {
        int_arg(args, 1, "ncompress_stream_read", span)?.max(1) as usize
    } else {
        64 * 1024
    };
    STREAMS.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(StreamHandle::Decompress(s)) => match s.read(max) {
                Ok(out) => Ok(bytes_result(out)),
                Err(e) => Ok(map_err(span, e)),
            },
            Some(StreamHandle::Compress(_)) => Ok(ncompress_err(
                span,
                "ncompress_stream_read() requires a decompress stream handle",
            )),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

fn ncompress_stream_finish(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncompress_stream_finish", span)?;
    let id = int_arg(args, 0, "ncompress_stream_finish", span)?;
    STREAMS.with(|m| {
        let mut m = m.borrow_mut();
        match m.remove(&id) {
            Some(StreamHandle::Compress(mut s)) => match s.finish() {
                Ok(out) => Ok(bytes_result(out)),
                Err(e) => Ok(map_err(span, e)),
            },
            Some(StreamHandle::Decompress(mut s)) => match s.finish() {
                Ok(out) => Ok(bytes_result(out)),
                Err(e) => Ok(map_err(span, e)),
            },
            None => Ok(invalid_handle(span, id)),
        }
    })
}

fn ncompress_stream_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncompress_stream_close", span)?;
    let id = int_arg(args, 0, "ncompress_stream_close", span)?;
    let removed = STREAMS.with(|m| m.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

macro_rules! codec_alias {
    ($prefix:ident, $codec:expr, $level:expr) => {
        fn $prefix(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
            arity_range(args, 1, 2, stringify!($prefix), span)?;
            let data = bytes_arg(args, 0, stringify!($prefix), span)?;
            let mut opts = CompressOpts::for_codec($codec);
            opts.level = int_field(optional_object_arg(args, 1).as_ref(), "level", $level as i64) as i32;
            opts.content_size = bool_field(optional_object_arg(args, 1).as_ref(), "content_size", true);
            match compress(&data, $codec, &opts) {
                Ok(out) => Ok(bytes_result(out)),
                Err(e) => Ok(map_err(span, e)),
            }
        }
    };
}

codec_alias!(ncompress_zstd_compress, Codec::Zstd, 3);
codec_alias!(ncompress_lz4_compress, Codec::Lz4, 0);
codec_alias!(ncompress_brotli_compress, Codec::Brotli, 6);
codec_alias!(ncompress_xz_compress, Codec::Xz, 6);

macro_rules! codec_dealias {
    ($prefix:ident, $codec:expr) => {
        fn $prefix(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
            arity_range(args, 1, 2, stringify!($prefix), span)?;
            let data = bytes_arg(args, 0, stringify!($prefix), span)?;
            let opts = decompress_opts_from_map(optional_object_arg(args, 1).as_ref());
            match decompress(&data, $codec, &opts) {
                Ok(out) => Ok(bytes_result(out)),
                Err(e) => Ok(map_err(span, e)),
            }
        }
    };
}

codec_dealias!(ncompress_zstd_decompress, Codec::Zstd);
codec_dealias!(ncompress_lz4_decompress, Codec::Lz4);
codec_dealias!(ncompress_brotli_decompress, Codec::Brotli);
codec_dealias!(ncompress_xz_decompress, Codec::Xz);

macro_rules! ncompress_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ncompress_fns![
    ("ncompress_compress", "compress", ncompress_compress),
    ("ncompress_decompress", "decompress", ncompress_decompress),
    ("ncompress_decompress_auto", "decompress_auto", ncompress_decompress_auto),
    ("ncompress_detect", "detect", ncompress_detect),
    ("ncompress_frame_info", "frame_info", ncompress_frame_info),
    ("ncompress_is_valid", "is_valid", ncompress_is_valid),
    ("ncompress_compress_file", "compress_file", ncompress_compress_file),
    ("ncompress_decompress_file", "decompress_file", ncompress_decompress_file),
    ("ncompress_parallel_compress", "parallel_compress", ncompress_parallel_compress),
    ("ncompress_parallel_decompress", "parallel_decompress", ncompress_parallel_decompress),
    ("ncompress_stream_open", "stream_open", ncompress_stream_open),
    ("ncompress_stream_write", "stream_write", ncompress_stream_write),
    ("ncompress_stream_read", "stream_read", ncompress_stream_read),
    ("ncompress_stream_finish", "stream_finish", ncompress_stream_finish),
    ("ncompress_stream_close", "stream_close", ncompress_stream_close),
    ("ncompress_zstd_compress", "zstd_compress", ncompress_zstd_compress),
    ("ncompress_zstd_decompress", "zstd_decompress", ncompress_zstd_decompress),
    ("ncompress_lz4_compress", "lz4_compress", ncompress_lz4_compress),
    ("ncompress_lz4_decompress", "lz4_decompress", ncompress_lz4_decompress),
    ("ncompress_brotli_compress", "brotli_compress", ncompress_brotli_compress),
    ("ncompress_brotli_decompress", "brotli_decompress", ncompress_brotli_decompress),
    ("ncompress_xz_compress", "xz_compress", ncompress_xz_compress),
    ("ncompress_xz_decompress", "xz_decompress", ncompress_xz_decompress),
];

pub const MODULE_NAME: &str = "ncompress";
pub const MODULE_PATHS: &[&str] = &["ncompress", "std/ncompress"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs()
        .into_iter()
        .map(|(flat, _, f)| (flat, f))
        .collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    let mut codecs = HashMap::new();
    for c in [Codec::Zstd, Codec::Lz4, Codec::Brotli, Codec::Xz] {
        codecs.insert(c.as_str().to_uppercase(), Value::String(c.as_str().into()).ref_cell());
    }
    map.insert("codecs".into(), Value::Object(codecs).ref_cell());
    map.insert("MAX_BYTES".into(), Value::Int(MAX_BYTES as i64).ref_cell());
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    #[test]
    fn compress_doctest() {
        let out = ncompress_compress(
            &[
                Value::ByteArray(vec![1, 2, 3]).ref_cell(),
                Value::String("zstd".into()).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        match &*out.borrow() {
            Value::ByteArray(b) => assert!(!b.is_empty()),
            other => panic!("expected bytes, got {other:?}"),
        }
    }

    #[test]
    fn roundtrip_zstd() {
        let raw = Value::ByteArray(b"hello ncompress".to_vec()).ref_cell();
        let c = ncompress_compress(
            &[raw, Value::String("zstd".into()).ref_cell()],
            span(),
        )
        .unwrap();
        let d = ncompress_decompress(
            &[c, Value::String("zstd".into()).ref_cell()],
            span(),
        )
        .unwrap();
        match &*d.borrow() {
            Value::ByteArray(b) => assert_eq!(b, b"hello ncompress"),
            other => panic!("expected bytes, got {other:?}"),
        }
    }
}
