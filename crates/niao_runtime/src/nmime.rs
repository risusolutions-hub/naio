//! Native nmime standard library — file-type detection by magic bytes,
//! extension<->MIME maps (~python-magic, filetype, mimetypes subset).
//!
//! Import with `import "nmime"` (or `import "std/nmime"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_mime::{
    bytes_match_mime, from_bytes, from_path, guess_extension_from_bytes, guess_mime, is_archive_mime,
    is_audio_mime, is_font_mime, is_image_mime, is_text_mime, is_valid_mime, is_video_mime,
    kind_name, kind_of_mime, mime_matches, normalize_mime, parse_mime,
    parallel_detect, parallel_from_bytes, parallel_guess_types, signature_count, Detector,
    FileKind, MatchSource, MimeError, MimeMatch, MimeRegistry, SniffOpts, DEFAULT_SNIFF_BYTES,
    MAX_SNIFF_BYTES,
};
use niao_parallel::available_threads;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

const E3550: u32 = codes::E3550_NMIME_ARITY;
const E3551: u32 = codes::E3551_NMIME_ERROR;
const E3552: u32 = codes::E3552_NMIME_TYPE;
const E3553: u32 = codes::E3553_NMIME_INVALID_HANDLE;

thread_local! {
    static DETECTORS: RefCell<HashMap<i64, Detector>> = RefCell::new(HashMap::new());
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
    RuntimeError::at(span, E3552, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3550,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3550,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn nmime_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3551, "nmime_error", msg.into(), span)
}

fn invalid_handle(span: Span, id: i64) -> ValueRef {
    error_value(
        E3553,
        "nmime_error",
        format!("invalid or closed nmime handle {id}"),
        span,
    )
}

fn map_err(span: Span, e: MimeError) -> ValueRef {
    nmime_err(span, e.message())
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

fn bool_arg(args: &[ValueRef], idx: usize, default: bool) -> bool {
    if args.len() <= idx {
        return default;
    }
    match &*args[idx].borrow() {
        Value::Bool(b) => *b,
        Value::Int(n) => *n != 0,
        _ => default,
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

fn parse_opts(args: &[ValueRef], idx: usize, span: Span) -> NiaoResult<HashMap<String, ValueRef>> {
    if args.len() <= idx {
        return Ok(HashMap::new());
    }
    match &*args[idx].borrow() {
        Value::Object(map) => Ok(map.clone()),
        Value::Nil => Ok(HashMap::new()),
        other => Err(type_err(
            span,
            format!("opts must be an object, got {}", other.type_name()),
        )),
    }
}

fn obj_bool(map: &HashMap<String, ValueRef>, key: &str, default: bool) -> bool {
    map.get(key)
        .and_then(|v| match &*v.borrow() {
            Value::Bool(b) => Some(*b),
            Value::Int(n) => Some(*n != 0),
            _ => None,
        })
        .unwrap_or(default)
}

fn obj_int(map: &HashMap<String, ValueRef>, key: &str, default: i64) -> i64 {
    map.get(key)
        .and_then(|v| match &*v.borrow() {
            Value::Int(n) => Some(*n),
            _ => None,
        })
        .unwrap_or(default)
}

fn sniff_opts_from(map: &HashMap<String, ValueRef>) -> SniffOpts {
    SniffOpts {
        max_bytes: obj_int(map, "max_bytes", DEFAULT_SNIFF_BYTES as i64) as usize,
        prefer_magic: obj_bool(map, "prefer_magic", true),
    }
}

fn source_name(s: MatchSource) -> &'static str {
    match s {
        MatchSource::Magic => "magic",
        MatchSource::Extension => "extension",
        MatchSource::Combined => "combined",
    }
}

fn match_object(m: &MimeMatch) -> ValueRef {
    let mut map = HashMap::new();
    map.insert("mime".into(), Value::String(m.mime.clone()).ref_cell());
    map.insert(
        "extension".into(),
        Value::String(m.extension.clone()).ref_cell(),
    );
    map.insert(
        "kind".into(),
        Value::String(kind_name(m.kind).into()).ref_cell(),
    );
    map.insert(
        "source".into(),
        Value::String(source_name(m.source).into()).ref_cell(),
    );
    map.insert("confidence".into(), Value::Float(m.confidence).ref_cell());
    Value::Object(map).ref_cell()
}

fn optional_match(m: Option<MimeMatch>) -> ValueRef {
    match m {
        Some(m) => match_object(&m),
        None => Value::Nil.ref_cell(),
    }
}

fn guess_type_object(mime: Option<String>, encoding: Option<String>) -> ValueRef {
    let mut map = HashMap::new();
    map.insert(
        "mime".into(),
        mime.map(|s| Value::String(s).ref_cell())
            .unwrap_or_else(|| Value::Nil.ref_cell()),
    );
    map.insert(
        "encoding".into(),
        encoding
            .map(|s| Value::String(s).ref_cell())
            .unwrap_or_else(|| Value::Nil.ref_cell()),
    );
    Value::Object(map).ref_cell()
}

fn string_list_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<String>> {
    match &*args[idx].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::String(s) => out.push(s.clone()),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() expects string array; item {} is {}",
                                i + 1,
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(out)
        }
        Value::Nil => Ok(Vec::new()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
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
                                "{name}() expects byte[] array; item {} is {}",
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
                "{name}() expects an array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn with_detector<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&Detector) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    DETECTORS.with(|m| {
        match m.borrow().get(&id) {
            Some(d) => Ok(Ok(f(d))),
            None => Ok(Err(invalid_handle(span, id))),
        }
    })
}

fn with_detector_mut<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut Detector) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    DETECTORS.with(|m| {
        match m.borrow_mut().get_mut(&id) {
            Some(d) => Ok(Ok(f(d))),
            None => Ok(Err(invalid_handle(span, id))),
        }
    })
}

fn builtin_registry() -> MimeRegistry {
    MimeRegistry::builtin()
}

fn bytes_kind_is(data: &[u8], pred: fn(&str) -> bool) -> bool {
    from_bytes(data, &[]).map(|m| pred(&m.mime)).unwrap_or(false)
}

// >>> nmime.from_bytes(byte_array[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])["mime"]
// => "image/png"
fn nmime_from_bytes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmime_from_bytes", span)?;
    let data = bytes_arg(args, 0, "nmime_from_bytes", span)?;
    Ok(optional_match(from_bytes(&data, &[])))
}

// >>> nmime.guess_mime(byte_array[0x25, 0x50, 0x44, 0x46])
// => "application/pdf"
fn nmime_guess_mime(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmime_guess_mime", span)?;
    let data = bytes_arg(args, 0, "nmime_guess_mime", span)?;
    Ok(guess_mime(&data, &[])
        .map(|s| Value::String(s).ref_cell())
        .unwrap_or_else(|| Value::Nil.ref_cell()))
}

// >>> nmime.guess_extension(byte_array[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])
// => "png"
fn nmime_guess_extension(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmime_guess_extension", span)?;
    let data = bytes_arg(args, 0, "nmime_guess_extension", span)?;
    Ok(guess_extension_from_bytes(&data, &[])
        .map(|s| Value::String(s).ref_cell())
        .unwrap_or_else(|| Value::Nil.ref_cell()))
}

// >>> nmime.match_mime(byte_array[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A], "image/png")
// => true
fn nmime_match_mime(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmime_match_mime", span)?;
    let data = bytes_arg(args, 0, "nmime_match_mime", span)?;
    let mime = string_arg(args, 1, "nmime_match_mime", span)?;
    Ok(Value::Bool(bytes_match_mime(&data, &mime, &[])).ref_cell())
}

fn nmime_is_image(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmime_is_image", span)?;
    let data = bytes_arg(args, 0, "nmime_is_image", span)?;
    Ok(Value::Bool(bytes_kind_is(&data, is_image_mime)).ref_cell())
}

fn nmime_is_video(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmime_is_video", span)?;
    let data = bytes_arg(args, 0, "nmime_is_video", span)?;
    Ok(Value::Bool(bytes_kind_is(&data, is_video_mime)).ref_cell())
}

fn nmime_is_audio(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmime_is_audio", span)?;
    let data = bytes_arg(args, 0, "nmime_is_audio", span)?;
    Ok(Value::Bool(bytes_kind_is(&data, is_audio_mime)).ref_cell())
}

fn nmime_is_archive(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmime_is_archive", span)?;
    let data = bytes_arg(args, 0, "nmime_is_archive", span)?;
    Ok(Value::Bool(bytes_kind_is(&data, is_archive_mime)).ref_cell())
}

fn nmime_is_text(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmime_is_text", span)?;
    let data = bytes_arg(args, 0, "nmime_is_text", span)?;
    Ok(Value::Bool(bytes_kind_is(&data, is_text_mime)).ref_cell())
}

fn nmime_is_font(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmime_is_font", span)?;
    let data = bytes_arg(args, 0, "nmime_is_font", span)?;
    Ok(Value::Bool(bytes_kind_is(&data, is_font_mime)).ref_cell())
}

// >>> nmime.from_path("Cargo.toml")["mime"]
// => "text/plain" or similar
fn nmime_from_path(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmime_from_path", span)?;
    let path = string_arg(args, 0, "nmime_from_path", span)?;
    let opts = parse_opts(args, 1, span)?;
    let sniff = sniff_opts_from(&opts);
    match from_path(&path, &sniff, &[]) {
        Ok(m) => Ok(optional_match(m)),
        Err(e) => Ok(map_err(span, e)),
    }
}

fn nmime_from_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    nmime_from_path(args, span)
}

fn nmime_sniff(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmime_sniff", span)?;
    let path = string_arg(args, 0, "nmime_sniff", span)?;
    let opts = parse_opts(args, 1, span)?;
    let sniff = sniff_opts_from(&opts);
    let reg = builtin_registry();
    match niao_mime::sniff_path(&path, &reg, &sniff, &[]) {
        Ok(m) => Ok(optional_match(m)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> nmime.guess_type("photo.jpg")["mime"]
// => "image/jpeg"
fn nmime_guess_type(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmime_guess_type", span)?;
    let name = string_arg(args, 0, "nmime_guess_type", span)?;
    let strict = bool_arg(args, 1, false);
    let g = builtin_registry().guess_type(&name, strict);
    Ok(guess_type_object(g.mime, g.encoding))
}

// >>> nmime.guess_extension("image/png")
// => "png"
fn nmime_guess_extension_mime(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmime_guess_extension", span)?;
    let mime = string_arg(args, 0, "nmime_guess_extension", span)?;
    let strict = bool_arg(args, 1, false);
    Ok(builtin_registry()
        .guess_extension(&mime, strict)
        .map(|s| Value::String(s).ref_cell())
        .unwrap_or_else(|| Value::Nil.ref_cell()))
}

fn nmime_guess_all_extensions(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmime_guess_all_extensions", span)?;
    let mime = string_arg(args, 0, "nmime_guess_all_extensions", span)?;
    let strict = bool_arg(args, 1, false);
    let exts = builtin_registry().mime_to_extensions(&mime, strict);
    let arr = exts
        .into_iter()
        .map(|s| Value::String(s).ref_cell())
        .collect();
    Ok(Value::Array(arr).ref_cell())
}

// >>> nmime.extension_to_mime("png")
// => "image/png"
fn nmime_extension_to_mime(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmime_extension_to_mime", span)?;
    let ext = string_arg(args, 0, "nmime_extension_to_mime", span)?;
    let strict = bool_arg(args, 1, false);
    Ok(builtin_registry()
        .extension_to_mime(&ext, strict)
        .map(|s| Value::String(s).ref_cell())
        .unwrap_or_else(|| Value::Nil.ref_cell()))
}

fn nmime_mime_to_extensions(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmime_mime_to_extensions", span)?;
    let mime = string_arg(args, 0, "nmime_mime_to_extensions", span)?;
    let strict = bool_arg(args, 1, false);
    let exts = builtin_registry().mime_to_extensions(&mime, strict);
    let arr = exts
        .into_iter()
        .map(|s| Value::String(s).ref_cell())
        .collect();
    Ok(Value::Array(arr).ref_cell())
}

fn nmime_add_type(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nmime_add_type", span)?;
    let mime = string_arg(args, 0, "nmime_add_type", span)?;
    let ext = string_arg(args, 1, "nmime_add_type", span)?;
    let strict = bool_arg(args, 2, false);
    let mut reg = builtin_registry();
    match reg.add_type(&mime, &ext, strict) {
        Ok(replaced) => Ok(Value::Bool(replaced).ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    }
}

fn nmime_known_extensions(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nmime_known_extensions", span)?;
    let strict = bool_arg(args, 0, false);
    let exts = builtin_registry().known_extensions(strict);
    let arr = exts
        .into_iter()
        .map(|s| Value::String(s).ref_cell())
        .collect();
    Ok(Value::Array(arr).ref_cell())
}

fn nmime_known_types(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nmime_known_types", span)?;
    let strict = bool_arg(args, 0, false);
    let types = builtin_registry().known_types(strict);
    let arr = types
        .into_iter()
        .map(|s| Value::String(s).ref_cell())
        .collect();
    Ok(Value::Array(arr).ref_cell())
}

fn nmime_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmime_parse", span)?;
    let raw = string_arg(args, 0, "nmime_parse", span)?;
    match parse_mime(&raw) {
        Ok(p) => {
            let mut map = HashMap::new();
            map.insert("type".into(), Value::String(p.type_).ref_cell());
            map.insert("subtype".into(), Value::String(p.subtype).ref_cell());
            map.insert(
                "suffix".into(),
                p.suffix
                    .map(|s| Value::String(s).ref_cell())
                    .unwrap_or_else(|| Value::Nil.ref_cell()),
            );
            map.insert(
                "canonical".into(),
                Value::String(p.canonical).ref_cell(),
            );
            let mut params = HashMap::new();
            for (k, v) in p.parameters {
                params.insert(k, Value::String(v).ref_cell());
            }
            map.insert("parameters".into(), Value::Object(params).ref_cell());
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(map_err(span, e)),
    }
}

fn nmime_is_valid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmime_is_valid", span)?;
    let raw = string_arg(args, 0, "nmime_is_valid", span)?;
    Ok(Value::Bool(is_valid_mime(&raw)).ref_cell())
}

fn nmime_normalize(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmime_normalize", span)?;
    let raw = string_arg(args, 0, "nmime_normalize", span)?;
    match normalize_mime(&raw) {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> nmime.matches("image/png", "image/*")
// => true
fn nmime_matches(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmime_matches", span)?;
    let mime = string_arg(args, 0, "nmime_matches", span)?;
    let pattern = string_arg(args, 1, "nmime_matches", span)?;
    match mime_matches(&mime, &pattern) {
        Ok(b) => Ok(Value::Bool(b).ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    }
}

fn nmime_kind(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmime_kind", span)?;
    let mime = string_arg(args, 0, "nmime_kind", span)?;
    Ok(Value::String(kind_name(kind_of_mime(&mime)).into()).ref_cell())
}

fn nmime_is_mime_image(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmime_is_mime_image", span)?;
    let mime = string_arg(args, 0, "nmime_is_mime_image", span)?;
    Ok(Value::Bool(is_image_mime(&mime)).ref_cell())
}

fn nmime_is_mime_video(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmime_is_mime_video", span)?;
    let mime = string_arg(args, 0, "nmime_is_mime_video", span)?;
    Ok(Value::Bool(is_video_mime(&mime)).ref_cell())
}

fn nmime_is_mime_audio(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmime_is_mime_audio", span)?;
    let mime = string_arg(args, 0, "nmime_is_mime_audio", span)?;
    Ok(Value::Bool(is_audio_mime(&mime)).ref_cell())
}

fn nmime_is_mime_archive(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmime_is_mime_archive", span)?;
    let mime = string_arg(args, 0, "nmime_is_mime_archive", span)?;
    Ok(Value::Bool(is_archive_mime(&mime)).ref_cell())
}

fn nmime_is_mime_text(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmime_is_mime_text", span)?;
    let mime = string_arg(args, 0, "nmime_is_mime_text", span)?;
    Ok(Value::Bool(is_text_mime(&mime)).ref_cell())
}

fn nmime_is_mime_font(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmime_is_mime_font", span)?;
    let mime = string_arg(args, 0, "nmime_is_mime_font", span)?;
    Ok(Value::Bool(is_font_mime(&mime)).ref_cell())
}

fn nmime_compile(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nmime_compile", span)?;
    let opts = parse_opts(args, 0, span)?;
    let mut det = Detector::new();
    det.sniff_opts = sniff_opts_from(&opts);
    let id = new_handle();
    DETECTORS.with(|m| m.borrow_mut().insert(id, det));
    Ok(Value::Int(id).ref_cell())
}

fn nmime_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmime_close", span)?;
    let id = int_arg(args, 0, "nmime_close", span)?;
    let removed = DETECTORS.with(|m| m.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

fn nmime_detect(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmime_detect", span)?;
    let id = int_arg(args, 0, "nmime_detect", span)?;
    let data = bytes_arg(args, 1, "nmime_detect", span)?;
    match with_detector(id, span, |d| d.detect_bytes(&data))? {
        Ok(m) => Ok(optional_match(m)),
        Err(v) => Ok(v),
    }
}

fn nmime_sniff_handle(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmime_sniff_handle", span)?;
    let id = int_arg(args, 0, "nmime_sniff_handle", span)?;
    let path = string_arg(args, 1, "nmime_sniff_handle", span)?;
    match with_detector(id, span, |d| d.sniff_file(PathBuf::from(path).as_path()))? {
        Ok(Ok(m)) => Ok(optional_match(m)),
        Ok(Err(e)) => Ok(map_err(span, e)),
        Err(v) => Ok(v),
    }
}

fn nmime_add_magic(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 5, "nmime_add_magic", span)?;
    let id = int_arg(args, 0, "nmime_add_magic", span)?;
    let data = bytes_arg(args, 1, "nmime_add_magic", span)?;
    let mime = string_arg(args, 2, "nmime_add_magic", span)?;
    let ext = if args.len() > 3 {
        match &*args[3].borrow() {
            Value::String(s) => Some(s.as_str()),
            Value::Nil => None,
            other => {
                return Err(type_err(
                    span,
                    format!("ext must be string or nil, got {}", other.type_name()),
                ));
            }
        }
    } else {
        None
    };
    let offset = if args.len() > 4 {
        int_arg(args, 4, "nmime_add_magic", span)? as usize
    } else {
        0
    };
    match with_detector_mut(id, span, |d| {
        d.add_magic(&data, &mime, ext, offset, None, None, None)
    })? {
        Ok(Ok(())) => Ok(Value::Bool(true).ref_cell()),
        Ok(Err(e)) => Ok(map_err(span, e)),
        Err(v) => Ok(v),
    }
}

fn nmime_parallel_detect(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmime_parallel_detect", span)?;
    let paths = string_list_arg(args, 0, "nmime_parallel_detect", span)?;
    let opts = parse_opts(args, 1, span)?;
    let threads = obj_int(&opts, "threads", available_threads() as i64) as usize;
    let det = Detector::new();
    let path_bufs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    let refs: Vec<&PathBuf> = path_bufs.iter().collect();
    let results = parallel_detect(&det, &refs, threads);
    let mut arr = Vec::with_capacity(results.len());
    for r in results {
        arr.push(match r {
            Ok(m) => optional_match(m),
            Err(e) => map_err(span, e),
        });
    }
    Ok(Value::Array(arr).ref_cell())
}

fn nmime_parallel_from_bytes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmime_parallel_from_bytes", span)?;
    let batches = bytes_list_arg(args, 0, "nmime_parallel_from_bytes", span)?;
    let opts = parse_opts(args, 1, span)?;
    let threads = obj_int(&opts, "threads", available_threads() as i64) as usize;
    let results = parallel_from_bytes(&batches, &[], threads);
    let arr = results
        .into_iter()
        .map(|m| optional_match(m))
        .collect();
    Ok(Value::Array(arr).ref_cell())
}

fn nmime_parallel_guess_types(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nmime_parallel_guess_types", span)?;
    let names = string_list_arg(args, 0, "nmime_parallel_guess_types", span)?;
    let strict = bool_arg(args, 1, false);
    let opts = parse_opts(args, 2, span)?;
    let threads = obj_int(&opts, "threads", available_threads() as i64) as usize;
    let reg = builtin_registry();
    let results = parallel_guess_types(&names, &reg, strict, threads);
    let arr = results
        .into_iter()
        .map(|g| guess_type_object(g.mime, g.encoding))
        .collect();
    Ok(Value::Array(arr).ref_cell())
}

fn nmime_max_sniff_bytes(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    Ok(Value::Int(MAX_SNIFF_BYTES as i64).ref_cell())
}

fn nmime_default_sniff_bytes(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    Ok(Value::Int(DEFAULT_SNIFF_BYTES as i64).ref_cell())
}

fn nmime_signature_count(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    Ok(Value::Int(signature_count() as i64).ref_cell())
}

fn nmime_common_types(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    let map = builtin_registry().common_types();
    let mut out = HashMap::new();
    for (k, v) in map {
        out.insert(k, Value::String(v).ref_cell());
    }
    Ok(Value::Object(out).ref_cell())
}

// >>> nmime.extension("archive.tar.gz")
// => "gz"
fn nmime_extension(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmime_extension", span)?;
    let path = string_arg(args, 0, "nmime_extension", span)?;
    Ok(std::path::Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| Value::String(s.to_ascii_lowercase()).ref_cell())
        .unwrap_or_else(|| Value::Nil.ref_cell()))
}

macro_rules! nmime_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nmime_fns![
    ("nmime_from_bytes", "from_bytes", nmime_from_bytes),
    ("nmime_guess_mime", "guess_mime", nmime_guess_mime),
    ("nmime_guess_extension", "guess_extension", nmime_guess_extension),
    ("nmime_match_mime", "match_mime", nmime_match_mime),
    ("nmime_is_image", "is_image", nmime_is_image),
    ("nmime_is_video", "is_video", nmime_is_video),
    ("nmime_is_audio", "is_audio", nmime_is_audio),
    ("nmime_is_archive", "is_archive", nmime_is_archive),
    ("nmime_is_text", "is_text", nmime_is_text),
    ("nmime_is_font", "is_font", nmime_is_font),
    ("nmime_from_path", "from_path", nmime_from_path),
    ("nmime_from_file", "from_file", nmime_from_file),
    ("nmime_sniff", "sniff", nmime_sniff),
    ("nmime_guess_type", "guess_type", nmime_guess_type),
    ("nmime_guess_extension_mime", "extension_for", nmime_guess_extension_mime),
    ("nmime_guess_all_extensions", "guess_all_extensions", nmime_guess_all_extensions),
    ("nmime_extension_to_mime", "extension_to_mime", nmime_extension_to_mime),
    ("nmime_mime_to_extensions", "mime_to_extensions", nmime_mime_to_extensions),
    ("nmime_add_type", "add_type", nmime_add_type),
    ("nmime_known_extensions", "known_extensions", nmime_known_extensions),
    ("nmime_known_types", "known_types", nmime_known_types),
    ("nmime_parse", "parse", nmime_parse),
    ("nmime_is_valid", "is_valid", nmime_is_valid),
    ("nmime_normalize", "normalize", nmime_normalize),
    ("nmime_matches", "matches", nmime_matches),
    ("nmime_kind", "kind", nmime_kind),
    ("nmime_is_mime_image", "is_mime_image", nmime_is_mime_image),
    ("nmime_is_mime_video", "is_mime_video", nmime_is_mime_video),
    ("nmime_is_mime_audio", "is_mime_audio", nmime_is_mime_audio),
    ("nmime_is_mime_archive", "is_mime_archive", nmime_is_mime_archive),
    ("nmime_is_mime_text", "is_mime_text", nmime_is_mime_text),
    ("nmime_is_mime_font", "is_mime_font", nmime_is_mime_font),
    ("nmime_compile", "compile", nmime_compile),
    ("nmime_close", "close", nmime_close),
    ("nmime_detect", "detect", nmime_detect),
    ("nmime_sniff_handle", "sniff_handle", nmime_sniff_handle),
    ("nmime_add_magic", "add_magic", nmime_add_magic),
    ("nmime_parallel_detect", "parallel_detect", nmime_parallel_detect),
    ("nmime_parallel_from_bytes", "parallel_from_bytes", nmime_parallel_from_bytes),
    ("nmime_parallel_guess_types", "parallel_guess_types", nmime_parallel_guess_types),
    ("nmime_max_sniff_bytes", "max_sniff_bytes", nmime_max_sniff_bytes),
    ("nmime_default_sniff_bytes", "default_sniff_bytes", nmime_default_sniff_bytes),
    ("nmime_signature_count", "signature_count", nmime_signature_count),
    ("nmime_common_types", "common_types", nmime_common_types),
    ("nmime_extension", "extension", nmime_extension),
];

fn all_builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nmime";
pub const MODULE_PATHS: &[&str] = &["nmime", "std/nmime"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    #[test]
    fn from_bytes_doctest() {
        let data = vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
        let v = nmime_from_bytes(&[Value::ByteArray(data).ref_cell()], span()).unwrap();
        match &*v.borrow() {
            Value::Object(m) => {
                let mime = m.get("mime").unwrap();
                assert_eq!(*mime.borrow(), Value::String("image/png".into()));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }
}
