//! Native nzip standard library — ZIP archives: read/write, streaming,
//! per-entry compression, AES encryption (~Python `zipfile` subset).
//!
//! Import with `import "nzip"` (or `import "std/nzip"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_zip as zip;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

// ---------------------------------------------------------------------------
// Handle tables
// ---------------------------------------------------------------------------

enum ZipState {
    Read(zip::ZipReader),
    Write(zip::ZipWriterHandle),
}

struct ZipEntry {
    state: ZipState,
}

thread_local! {
    static ARCHIVES: RefCell<HashMap<i64, ZipEntry>> = RefCell::new(HashMap::new());
    static NEXT_HANDLE: RefCell<i64> = const { RefCell::new(1) };
}

fn alloc_handle() -> i64 {
    NEXT_HANDLE.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    })
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E4392_NZIP_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E4390_NZIP_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E4390_NZIP_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn nzip_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E4391_NZIP_ERROR, "nzip_error", msg.into(), span)
}

fn nzip_not_found(span: Span, name: impl Into<String>) -> ValueRef {
    error_value(
        codes::E4394_NZIP_NOT_FOUND,
        "nzip_error",
        format!("zip entry not found: {}", name.into()),
        span,
    )
}

fn nzip_password(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E4395_NZIP_PASSWORD, "nzip_error", msg.into(), span)
}

fn invalid_handle(span: Span, id: i64) -> ValueRef {
    error_value(
        codes::E4393_NZIP_INVALID_HANDLE,
        "nzip_error",
        format!("invalid or closed zip handle {id}"),
        span,
    )
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

fn optional_object(args: &[ValueRef], idx: usize) -> Option<HashMap<String, ValueRef>> {
    args.get(idx).and_then(|v| match &*v.borrow() {
        Value::Object(map) => Some(map.clone()),
        Value::Nil => None,
        _ => None,
    })
}

fn bool_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: bool) -> bool {
    let Some(map) = map else {
        return default;
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Bool(b)) => b,
        Some(Value::Int(n)) => n != 0,
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

fn string_field(map: &HashMap<String, ValueRef>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.clone()),
        _ => None,
    })
}

fn bytes_from_value(v: &ValueRef, name: &str, span: Span) -> NiaoResult<Vec<u8>> {
    match &*v.borrow() {
        Value::ByteArray(b) => Ok(b.clone()),
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        other => Err(type_err(
            span,
            format!("{name}() expects string or bytes, got {}", other.type_name()),
        )),
    }
}

fn bytes_result(bytes: Vec<u8>) -> ValueRef {
    Value::ByteArray(bytes).ref_cell()
}

fn ok_bool(b: bool) -> ValueRef {
    Value::Bool(b).ref_cell()
}

fn ok_int(n: i64) -> ValueRef {
    Value::Int(n).ref_cell()
}

fn ok_string(s: impl Into<String>) -> ValueRef {
    Value::String(s.into()).ref_cell()
}

fn ok_nil() -> ValueRef {
    Value::Nil.ref_cell()
}

fn handle_obj(id: i64, mode: &str) -> ValueRef {
    let mut map = HashMap::new();
    map.insert("handle".into(), ok_int(id));
    map.insert("mode".into(), ok_string(mode));
    Value::Object(map).ref_cell()
}

fn map_zip_err(span: Span, err: zip::ZipError) -> ValueRef {
    match err {
        zip::ZipError::NotFound(name) => nzip_not_found(span, name),
        zip::ZipError::PasswordRequired(name) => {
            nzip_password(span, format!("password required for entry: {name}"))
        }
        zip::ZipError::BadPassword(name) => {
            nzip_password(span, format!("bad password for entry: {name}"))
        }
        other => nzip_err(span, other.to_string()),
    }
}

fn parse_compression(map: Option<&HashMap<String, ValueRef>>, span: Span) -> NiaoResult<zip::CompressionName> {
    let Some(map) = map else {
        return Ok(zip::CompressionName::Deflated);
    };
    let Some(v) = map.get("compression").or_else(|| map.get("compress_type")) else {
        return Ok(zip::CompressionName::Deflated);
    };
    let s = match &*v.borrow() {
        Value::String(s) => s.clone(),
        Value::Int(n) => n.to_string(),
        other => {
            return Err(type_err(
                span,
                format!("opts.compression must be string or int, got {}", other.type_name()),
            ));
        }
    };
    zip::CompressionName::parse(&s).map_err(|e| type_err(span, e.to_string()))
}

fn parse_write_opts(map: Option<HashMap<String, ValueRef>>, span: Span) -> NiaoResult<zip::WriteOptions> {
    let map_ref = map.as_ref();
    let mut opts = zip::WriteOptions::default();
    opts.compression = parse_compression(map_ref, span)?;
    opts.level = int_field(map_ref, "level", zip::DEFAULT_LEVEL as i64) as i32;
    opts.large_file = bool_field(map_ref, "large_file", true);
    if let Some(m) = map_ref {
        if let Some(pwd) = string_field(m, "password") {
            opts.password = Some(pwd);
        }
        if let Some(c) = string_field(m, "comment") {
            opts.comment = Some(c);
        }
    }
    Ok(opts)
}

fn parse_open_opts(map: Option<HashMap<String, ValueRef>>) -> zip::OpenOptions {
    let mut opts = zip::OpenOptions::default();
    if let Some(m) = map.as_ref() {
        if let Some(pwd) = string_field(m, "password") {
            opts.password = Some(pwd.into_bytes());
        }
    }
    opts
}

fn parse_entry_write_opts(
    map: Option<HashMap<String, ValueRef>>,
    span: Span,
) -> NiaoResult<zip::EntryWriteOptions> {
    let map_ref = map.as_ref();
    let mut opts = zip::EntryWriteOptions::default();
    if let Some(m) = map_ref {
        opts.arcname = string_field(m, "arcname").or_else(|| string_field(m, "name"));
        if m.contains_key("compression") || m.contains_key("compress_type") {
            opts.compression = Some(parse_compression(map_ref, span)?);
        }
        if m.contains_key("level") {
            opts.level = Some(int_field(map_ref, "level", zip::DEFAULT_LEVEL as i64) as i32);
        }
        opts.comment = string_field(m, "comment");
    }
    Ok(opts)
}

fn parse_extract_opts(map: Option<HashMap<String, ValueRef>>) -> zip::ExtractOptions {
    let map_ref = map.as_ref();
    let mut opts = zip::ExtractOptions::default();
    if let Some(m) = map_ref {
        if let Some(pwd) = string_field(m, "password") {
            opts.password = Some(pwd.into_bytes());
        }
        let threads = int_field(map_ref, "threads", 0);
        if threads > 0 {
            opts.threads = Some(threads as usize);
        }
        opts.overwrite = bool_field(map_ref, "overwrite", false);
    }
    opts
}

fn entry_info_to_niao(info: &zip::EntryInfo) -> ValueRef {
    let mut map = HashMap::new();
    map.insert("name".into(), ok_string(&info.name));
    map.insert("size".into(), ok_int(info.size as i64));
    map.insert("compressed_size".into(), ok_int(info.compressed_size as i64));
    map.insert("compression".into(), ok_string(info.compression.as_str()));
    map.insert("is_dir".into(), ok_bool(info.is_dir));
    map.insert("is_symlink".into(), ok_bool(info.is_symlink));
    map.insert("crc32".into(), ok_int(info.crc32 as i64));
    map.insert(
        "modified".into(),
        info.modified_unix
            .map(ok_int)
            .unwrap_or_else(ok_nil),
    );
    map.insert("encrypted".into(), ok_bool(info.encrypted));
    map.insert(
        "comment".into(),
        info.comment
            .as_ref()
            .map(|c| ok_string(c.clone()))
            .unwrap_or_else(ok_nil),
    );
    Value::Object(map).ref_cell()
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

// >>> nzip.is_zipfile("archive.zip")
fn nzip_is_zipfile(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nzip_is_zipfile", span)?;
    let path = string_arg(args, 0, "nzip_is_zipfile", span)?;
    Ok(ok_bool(zip::is_zipfile_path(PathBuf::from(path).as_path())))
}

// >>> nzip.is_zipfile_bytes(bytes)
fn nzip_is_zipfile_bytes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nzip_is_zipfile_bytes", span)?;
    let data = bytes_from_value(&args[0], "nzip_is_zipfile_bytes", span)?;
    Ok(ok_bool(zip::is_zipfile_bytes(&data)))
}

// >>> nzip.open("a.zip")
fn nzip_open(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nzip_open", span)?;
    let path = string_arg(args, 0, "nzip_open", span)?;
    let opts = parse_open_opts(optional_object(args, 1));
    match zip::ZipReader::open(&path, &opts) {
        Ok(reader) => {
            let id = alloc_handle();
            ARCHIVES.with(|m| {
                m.borrow_mut().insert(
                    id,
                    ZipEntry {
                        state: ZipState::Read(reader),
                    },
                );
            });
            Ok(handle_obj(id, "r"))
        }
        Err(e) => Ok(map_zip_err(span, e)),
    }
}

// >>> nzip.create("out.zip")
fn nzip_create(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nzip_create", span)?;
    let path = string_arg(args, 0, "nzip_create", span)?;
    let opts = parse_write_opts(optional_object(args, 1), span)?;
    match zip::ZipWriterHandle::create(&path, &opts) {
        Ok(writer) => {
            let id = alloc_handle();
            ARCHIVES.with(|m| {
                m.borrow_mut().insert(
                    id,
                    ZipEntry {
                        state: ZipState::Write(writer),
                    },
                );
            });
            Ok(handle_obj(id, "w"))
        }
        Err(e) => Ok(map_zip_err(span, e)),
    }
}

// >>> nzip.append("out.zip")
fn nzip_append(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nzip_append", span)?;
    let path = string_arg(args, 0, "nzip_append", span)?;
    let opts = parse_write_opts(optional_object(args, 1), span)?;
    match zip::ZipWriterHandle::append(&path, &opts) {
        Ok(writer) => {
            let id = alloc_handle();
            ARCHIVES.with(|m| {
                m.borrow_mut().insert(
                    id,
                    ZipEntry {
                        state: ZipState::Write(writer),
                    },
                );
            });
            Ok(handle_obj(id, "a"))
        }
        Err(e) => Ok(map_zip_err(span, e)),
    }
}

// >>> nzip.close(h.handle)
fn nzip_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nzip_close", span)?;
    let id = int_arg(args, 0, "nzip_close", span)?;
    let removed = ARCHIVES.with(|m| {
        let mut m = m.borrow_mut();
        match m.remove(&id) {
            Some(ZipEntry {
                state: ZipState::Write(writer),
            }) => match writer.finish() {
                Ok(_) => Ok(ok_bool(true)),
                Err(e) => Ok(map_zip_err(span, e)),
            },
            Some(ZipEntry {
                state: ZipState::Read(_),
            }) => Ok(ok_bool(true)),
            None => Ok(invalid_handle(span, id)),
        }
    });
    removed
}

// >>> nzip.namelist(h.handle)
fn nzip_namelist(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nzip_namelist", span)?;
    let id = int_arg(args, 0, "nzip_namelist", span)?;
    ARCHIVES.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(ZipEntry {
                state: ZipState::Read(reader),
            }) => match reader.namelist() {
                Ok(names) => {
                    let items: Vec<ValueRef> = names.into_iter().map(ok_string).collect();
                    Ok(Value::Array(items).ref_cell())
                }
                Err(e) => Ok(map_zip_err(span, e)),
            },
            Some(_) => Ok(nzip_err(span, "namelist requires a read-mode zip handle")),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

// >>> nzip.infolist(h.handle)
fn nzip_infolist(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nzip_infolist", span)?;
    let id = int_arg(args, 0, "nzip_infolist", span)?;
    ARCHIVES.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(ZipEntry {
                state: ZipState::Read(reader),
            }) => match reader.infolist() {
                Ok(list) => {
                    let items: Vec<ValueRef> = list.iter().map(entry_info_to_niao).collect();
                    Ok(Value::Array(items).ref_cell())
                }
                Err(e) => Ok(map_zip_err(span, e)),
            },
            Some(_) => Ok(nzip_err(span, "infolist requires a read-mode zip handle")),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

// >>> nzip.getinfo(h.handle, "a.txt")
fn nzip_getinfo(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nzip_getinfo", span)?;
    let id = int_arg(args, 0, "nzip_getinfo", span)?;
    let name = string_arg(args, 1, "nzip_getinfo", span)?;
    ARCHIVES.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(ZipEntry {
                state: ZipState::Read(reader),
            }) => match reader.getinfo(&name) {
                Ok(info) => Ok(entry_info_to_niao(&info)),
                Err(e) => Ok(map_zip_err(span, e)),
            },
            Some(_) => Ok(nzip_err(span, "getinfo requires a read-mode zip handle")),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

// >>> nzip.read(h.handle, "a.txt")
fn nzip_read(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nzip_read", span)?;
    let id = int_arg(args, 0, "nzip_read", span)?;
    let name = string_arg(args, 1, "nzip_read", span)?;
    ARCHIVES.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(ZipEntry {
                state: ZipState::Read(reader),
            }) => match reader.read(&name) {
                Ok(bytes) => Ok(bytes_result(bytes)),
                Err(e) => Ok(map_zip_err(span, e)),
            },
            Some(_) => Ok(nzip_err(span, "read requires a read-mode zip handle")),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

// >>> nzip.comment(h.handle)
fn nzip_comment(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nzip_comment", span)?;
    let id = int_arg(args, 0, "nzip_comment", span)?;
    ARCHIVES.with(|m| {
        let m = m.borrow();
        match m.get(&id) {
            Some(ZipEntry {
                state: ZipState::Read(reader),
            }) => Ok(reader
                .comment()
                .map(ok_string)
                .unwrap_or_else(ok_nil)),
            Some(_) => Ok(nzip_err(span, "comment requires a read-mode zip handle")),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

// >>> nzip.set_password(h.handle, "secret")
fn nzip_set_password(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nzip_set_password", span)?;
    let id = int_arg(args, 0, "nzip_set_password", span)?;
    let pwd = if args.len() > 1 {
        Some(bytes_from_value(&args[1], "nzip_set_password", span)?)
    } else {
        None
    };
    ARCHIVES.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(ZipEntry {
                state: ZipState::Read(reader),
            }) => {
                reader.set_password(pwd);
                Ok(ok_bool(true))
            }
            Some(_) => Ok(nzip_err(span, "set_password requires a read-mode zip handle")),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

// >>> nzip.open_entry(h.handle, "a.txt")
fn nzip_open_entry(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nzip_open_entry", span)?;
    let id = int_arg(args, 0, "nzip_open_entry", span)?;
    let name = string_arg(args, 1, "nzip_open_entry", span)?;
    ARCHIVES.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(ZipEntry {
                state: ZipState::Read(reader),
            }) => match reader.open_entry(&name) {
                Ok(()) => Ok(ok_bool(true)),
                Err(e) => Ok(map_zip_err(span, e)),
            },
            Some(_) => Ok(nzip_err(span, "open_entry requires a read-mode zip handle")),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

// >>> nzip.entry_read(h.handle, 4096)
fn nzip_entry_read(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nzip_entry_read", span)?;
    let id = int_arg(args, 0, "nzip_entry_read", span)?;
    let max = if args.len() > 1 {
        int_arg(args, 1, "nzip_entry_read", span)? as usize
    } else {
        65536
    };
    ARCHIVES.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(ZipEntry {
                state: ZipState::Read(reader),
            }) => match reader.entry_read(max) {
                Ok(chunk) => Ok(bytes_result(chunk)),
                Err(e) => Ok(map_zip_err(span, e)),
            },
            Some(_) => Ok(nzip_err(span, "entry_read requires a read-mode zip handle")),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

// >>> nzip.entry_close(h.handle)
fn nzip_entry_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nzip_entry_close", span)?;
    let id = int_arg(args, 0, "nzip_entry_close", span)?;
    ARCHIVES.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(ZipEntry {
                state: ZipState::Read(reader),
            }) => match reader.entry_close() {
                Ok(()) => Ok(ok_bool(true)),
                Err(e) => Ok(map_zip_err(span, e)),
            },
            Some(_) => Ok(nzip_err(span, "entry_close requires a read-mode zip handle")),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

// >>> nzip.test(h.handle)
fn nzip_test(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nzip_test", span)?;
    let id = int_arg(args, 0, "nzip_test", span)?;
    ARCHIVES.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(ZipEntry {
                state: ZipState::Read(reader),
            }) => match reader.test() {
                Ok(()) => Ok(ok_bool(true)),
                Err(e) => Ok(map_zip_err(span, e)),
            },
            Some(_) => Ok(nzip_err(span, "test requires a read-mode zip handle")),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

// >>> nzip.extract(h.handle, "a.txt", "./out")
fn nzip_extract(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 4, "nzip_extract", span)?;
    let id = int_arg(args, 0, "nzip_extract", span)?;
    let name = string_arg(args, 1, "nzip_extract", span)?;
    let dest = if args.len() > 2 {
        string_arg(args, 2, "nzip_extract", span)?
    } else {
        ".".into()
    };
    let opts = parse_extract_opts(optional_object(args, 3));
    ARCHIVES.with(|m| {
        let m = m.borrow();
        match m.get(&id) {
            Some(ZipEntry {
                state: ZipState::Read(reader),
            }) => {
                let path = reader.path().to_path_buf();
                drop(m);
                match zip::extract_one(
                    &path,
                    &name,
                    PathBuf::from(dest).as_path(),
                    opts.password.as_deref(),
                    opts.overwrite,
                ) {
                    Ok(p) => Ok(ok_string(p.display().to_string())),
                    Err(e) => Ok(map_zip_err(span, e)),
                }
            }
            Some(_) => Ok(nzip_err(span, "extract requires a read-mode zip handle")),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

// >>> nzip.extract_all(h.handle, "./out")
fn nzip_extract_all(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nzip_extract_all", span)?;
    let id = int_arg(args, 0, "nzip_extract_all", span)?;
    let dest = if args.len() > 1 {
        string_arg(args, 1, "nzip_extract_all", span)?
    } else {
        ".".into()
    };
    let opts = parse_extract_opts(optional_object(args, 2));
    ARCHIVES.with(|m| {
        let m = m.borrow();
        match m.get(&id) {
            Some(ZipEntry {
                state: ZipState::Read(reader),
            }) => {
                let path = reader.path().to_path_buf();
                drop(m);
                match zip::extract_all(&path, PathBuf::from(dest).as_path(), &opts) {
                    Ok(paths) => {
                        let items: Vec<ValueRef> = paths
                            .into_iter()
                            .map(|p| ok_string(p.display().to_string()))
                            .collect();
                        Ok(Value::Array(items).ref_cell())
                    }
                    Err(e) => Ok(map_zip_err(span, e)),
                }
            }
            Some(_) => Ok(nzip_err(span, "extract_all requires a read-mode zip handle")),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

// >>> nzip.write_file(h.handle, "local.txt")
fn nzip_write_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nzip_write_file", span)?;
    let id = int_arg(args, 0, "nzip_write_file", span)?;
    let src = string_arg(args, 1, "nzip_write_file", span)?;
    let entry_opts = parse_entry_write_opts(optional_object(args, 2), span)?;
    ARCHIVES.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(ZipEntry {
                state: ZipState::Write(writer),
            }) => match writer.write_file(PathBuf::from(src).as_path(), &entry_opts) {
                Ok(n) => Ok(ok_int(n as i64)),
                Err(e) => Ok(map_zip_err(span, e)),
            },
            Some(_) => Ok(nzip_err(span, "write_file requires a write-mode zip handle")),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

// >>> nzip.write_bytes(h.handle, "a.txt", bytes)
fn nzip_write_bytes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nzip_write_bytes", span)?;
    let id = int_arg(args, 0, "nzip_write_bytes", span)?;
    let arcname = string_arg(args, 1, "nzip_write_bytes", span)?;
    let data = bytes_from_value(&args[2], "nzip_write_bytes", span)?;
    let entry_opts = parse_entry_write_opts(optional_object(args, 3), span)?;
    ARCHIVES.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(ZipEntry {
                state: ZipState::Write(writer),
            }) => match writer.write_bytes(&arcname, &data, &entry_opts) {
                Ok(n) => Ok(ok_int(n as i64)),
                Err(e) => Ok(map_zip_err(span, e)),
            },
            Some(_) => Ok(nzip_err(span, "write_bytes requires a write-mode zip handle")),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

// >>> nzip.writestr(h.handle, "a.txt", "hi")
fn nzip_writestr(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    nzip_write_bytes(args, span)
}

// >>> nzip.mkdir(h.handle, "subdir")
fn nzip_mkdir(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nzip_mkdir", span)?;
    let id = int_arg(args, 0, "nzip_mkdir", span)?;
    let arcname = string_arg(args, 1, "nzip_mkdir", span)?;
    ARCHIVES.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(ZipEntry {
                state: ZipState::Write(writer),
            }) => match writer.mkdir(&arcname) {
                Ok(()) => Ok(ok_bool(true)),
                Err(e) => Ok(map_zip_err(span, e)),
            },
            Some(_) => Ok(nzip_err(span, "mkdir requires a write-mode zip handle")),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

// >>> nzip.set_comment(h.handle, "note")
fn nzip_set_comment(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nzip_set_comment", span)?;
    let id = int_arg(args, 0, "nzip_set_comment", span)?;
    let comment = string_arg(args, 1, "nzip_set_comment", span)?;
    ARCHIVES.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(ZipEntry {
                state: ZipState::Write(writer),
            }) => match writer.set_comment(&comment) {
                Ok(()) => Ok(ok_bool(true)),
                Err(e) => Ok(map_zip_err(span, e)),
            },
            Some(_) => Ok(nzip_err(span, "set_comment requires a write-mode zip handle")),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

// ---------------------------------------------------------------------------
// Module exports
// ---------------------------------------------------------------------------

macro_rules! nzip_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nzip_fns![
    ("nzip_is_zipfile", "is_zipfile", nzip_is_zipfile),
    ("nzip_is_zipfile_bytes", "is_zipfile_bytes", nzip_is_zipfile_bytes),
    ("nzip_open", "open", nzip_open),
    ("nzip_create", "create", nzip_create),
    ("nzip_append", "append", nzip_append),
    ("nzip_close", "close", nzip_close),
    ("nzip_namelist", "namelist", nzip_namelist),
    ("nzip_infolist", "infolist", nzip_infolist),
    ("nzip_getinfo", "getinfo", nzip_getinfo),
    ("nzip_read", "read", nzip_read),
    ("nzip_comment", "comment", nzip_comment),
    ("nzip_set_password", "set_password", nzip_set_password),
    ("nzip_open_entry", "open_entry", nzip_open_entry),
    ("nzip_entry_read", "entry_read", nzip_entry_read),
    ("nzip_entry_close", "entry_close", nzip_entry_close),
    ("nzip_test", "test", nzip_test),
    ("nzip_extract", "extract", nzip_extract),
    ("nzip_extract_all", "extract_all", nzip_extract_all),
    ("nzip_write_file", "write_file", nzip_write_file),
    ("nzip_write_bytes", "write_bytes", nzip_write_bytes),
    ("nzip_writestr", "writestr", nzip_writestr),
    ("nzip_mkdir", "mkdir", nzip_mkdir),
    ("nzip_set_comment", "set_comment", nzip_set_comment),
];

pub const MODULE_NAME: &str = "nzip";
pub const MODULE_PATHS: &[&str] = &["nzip", "std/nzip"];

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
    map.insert(
        "STORED".into(),
        ok_string(zip::CompressionName::Stored.as_str()),
    );
    map.insert(
        "DEFLATED".into(),
        ok_string(zip::CompressionName::Deflated.as_str()),
    );
    map.insert(
        "BZIP2".into(),
        ok_string(zip::CompressionName::Bzip2.as_str()),
    );
    map.insert(
        "LZMA".into(),
        ok_string(zip::CompressionName::Lzma.as_str()),
    );
    map.insert(
        "ZSTD".into(),
        ok_string(zip::CompressionName::Zstd.as_str()),
    );
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
    fn is_zipfile_false_for_missing() {
        let args = vec![Value::String("no_such_zip_zzz.zip".into()).ref_cell()];
        let out = nzip_is_zipfile(&args, span()).unwrap();
        match &*out.borrow() {
            Value::Bool(b) => assert!(!b),
            other => panic!("expected bool, got {other:?}"),
        }
    }
}
