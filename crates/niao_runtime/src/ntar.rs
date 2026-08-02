//! Native ntar standard library — tar archives read/write incl. .tar.gz / .tar.zst.
//!
//! Import with `import "ntar"` (or `import "std/ntar"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_tar::{
    create_archive, detect_compression, extract_all, extract_member, is_tar_file, is_tar_path,
    pack_tree, parse_mode, unpack, AddOpts, Compression, EntryInfo, ExtractOpts, OpenMode,
    ReadOpts, TarReader, TarWriter, WriteOpts, MAX_ENTRY_BYTES,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

enum HandleState {
    Reader(TarReader),
    Writer(TarWriter),
}

struct TarEntry {
    state: HandleState,
}

thread_local! {
    static ARCHIVES: RefCell<HashMap<i64, TarEntry>> = RefCell::new(HashMap::new());
    static NEXT_HANDLE: RefCell<i64> = const { RefCell::new(1) };
}

fn new_handle() -> i64 {
    NEXT_HANDLE.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    })
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E4366_NTAR_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E4364_NTAR_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E4364_NTAR_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn ntar_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E4365_NTAR_ERROR, "ntar_error", msg.into(), span)
}

fn not_found(span: Span, name: impl Into<String>) -> ValueRef {
    error_value(codes::E4368_NTAR_NOT_FOUND, "ntar_error", name.into(), span)
}

fn invalid_handle(span: Span, id: i64) -> ValueRef {
    error_value(
        codes::E4367_NTAR_INVALID_HANDLE,
        "ntar_error",
        format!("invalid or closed ntar handle {id}"),
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

fn bool_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<bool> {
    match &*args[idx].borrow() {
        Value::Bool(b) => Ok(*b),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a bool as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn optional_object(args: &[ValueRef], idx: usize) -> Option<HashMap<String, ValueRef>> {
    args.get(idx).and_then(|v| match &*v.borrow() {
        Value::Object(map) => Some(map.clone()),
        _ => None,
    })
}

fn path_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<PathBuf> {
    Ok(PathBuf::from(string_arg(args, idx, name, span)?))
}

fn bytes_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<u8>> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        Value::ByteArray(b) => Ok(b.iter().map(|&x| x as u8).collect()),
        Value::IntArray(b) => Ok(b.iter().map(|&x| x as u8).collect()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects string or bytes as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn bytes_val(bytes: Vec<u8>) -> NiaoResult<ValueRef> {
    Ok(Value::ByteArray(bytes).ref_cell())
}

fn ok_nil() -> ValueRef {
    Value::Nil.ref_cell()
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

fn compression_from_obj(map: &HashMap<String, ValueRef>, span: Span) -> NiaoResult<Option<Compression>> {
    let Some(v) = map.get("compression").or_else(|| map.get("format")) else {
        return Ok(None);
    };
    match &*v.borrow() {
        Value::String(s) => Compression::parse(s).map(Some).ok_or_else(|| {
            type_err(span, format!("unknown compression '{s}' (expected none, gz, zst)"))
        }),
        other => Err(type_err(
            span,
            format!("compression must be a string, got {}", other.type_name()),
        )),
    }
}

fn parse_open_opts(map: Option<HashMap<String, ValueRef>>, span: Span) -> NiaoResult<(OpenMode, Option<Compression>, i32)> {
    let mut mode = OpenMode::Read;
    let mut compression: Option<Compression> = None;
    let mut level = 6i32;
    if let Some(map) = map {
        if let Some(v) = map.get("mode") {
            let s = match &*v.borrow() {
                Value::String(s) => s.clone(),
                other => {
                    return Err(type_err(
                        span,
                        format!("mode must be a string, got {}", other.type_name()),
                    ));
                }
            };
            let (m, c) = parse_mode(&s).map_err(|e| type_err(span, e.to_string()))?;
            mode = m;
            if c != Compression::None {
                compression = Some(c);
            }
        }
        if let Some(c) = compression_from_obj(&map, span)? {
            compression = Some(c);
        }
        if let Some(v) = map.get("level") {
            level = int_arg(&[v.clone()], 0, "level", span)? as i32;
        }
    }
    Ok((mode, compression, level))
}

fn parse_add_opts(map: Option<HashMap<String, ValueRef>>, span: Span) -> NiaoResult<AddOpts> {
    let mut opts = AddOpts::default();
    if let Some(map) = map {
        if let Some(v) = map.get("arcname") {
            opts.arcname = Some(string_arg(&[v.clone()], 0, "arcname", span)?);
        }
        if let Some(v) = map.get("mode") {
            opts.mode = Some(int_arg(&[v.clone()], 0, "mode", span)? as u32);
        }
        if let Some(v) = map.get("mtime") {
            opts.mtime = Some(int_arg(&[v.clone()], 0, "mtime", span)?);
        }
        if let Some(v) = map.get("recursive") {
            opts.recursive = bool_arg(&[v.clone()], 0, "recursive", span)?;
        }
    }
    Ok(opts)
}

fn parse_extract_opts(map: Option<HashMap<String, ValueRef>>, span: Span) -> NiaoResult<ExtractOpts> {
    let mut opts = ExtractOpts::default();
    opts.max_entry_bytes = MAX_ENTRY_BYTES;
    if let Some(map) = map {
        if let Some(v) = map.get("members") {
            match &*v.borrow() {
                Value::Array(items) => {
                    let mut names = Vec::with_capacity(items.len());
                    for item in items {
                        match &*item.borrow() {
                            Value::String(s) => names.push(s.clone()),
                            other => {
                                return Err(type_err(
                                    span,
                                    format!("members must be string array, got {}", other.type_name()),
                                ));
                            }
                        }
                    }
                    opts.members = Some(names);
                }
                other => {
                    return Err(type_err(
                        span,
                        format!("members must be an array, got {}", other.type_name()),
                    ));
                }
            }
        }
        if let Some(v) = map.get("numeric_owner") {
            opts.numeric_owner = bool_arg(&[v.clone()], 0, "numeric_owner", span)?;
        }
        if let Some(v) = map.get("max_entry_bytes") {
            opts.max_entry_bytes = int_arg(&[v.clone()], 0, "max_entry_bytes", span)? as usize;
        }
        if let Some(v) = map.get("threads") {
            opts.threads = int_arg(&[v.clone()], 0, "threads", span)? as usize;
        }
    }
    Ok(opts)
}

fn info_object(info: &EntryInfo) -> Value {
    let mut map = HashMap::new();
    map.insert("name".into(), ok_string(&info.name));
    map.insert("size".into(), ok_int(info.size as i64));
    map.insert("mode".into(), ok_int(info.mode as i64));
    map.insert("mtime".into(), ok_int(info.mtime));
    map.insert("uid".into(), ok_int(info.uid as i64));
    map.insert("gid".into(), ok_int(info.gid as i64));
    map.insert("type".into(), ok_string(info.kind.as_str()));
    map.insert("index".into(), ok_int(info.index as i64));
    if let Some(ref target) = info.link_target {
        map.insert("link_target".into(), ok_string(target));
    }
    Value::Object(map)
}

fn handle_object(id: i64, mode: &str, compression: Compression, path: Option<&Path>) -> Value {
    let mut map = HashMap::new();
    map.insert("handle".into(), ok_int(id));
    map.insert("mode".into(), ok_string(mode));
    map.insert("compression".into(), ok_string(compression.as_str()));
    if let Some(p) = path {
        map.insert("path".into(), ok_string(p.to_string_lossy()));
    }
    Value::Object(map)
}

fn with_reader<F>(id: i64, span: Span, f: F) -> NiaoResult<ValueRef>
where
    F: FnOnce(&TarReader) -> NiaoResult<ValueRef>,
{
    ARCHIVES.with(|a| {
        let a = a.borrow();
        match a.get(&id) {
            Some(TarEntry {
                state: HandleState::Reader(r),
            }) => f(r),
            Some(_) => Ok(ntar_err(span, "handle is open for writing, not reading")),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

fn with_writer<F>(id: i64, span: Span, f: F) -> NiaoResult<ValueRef>
where
    F: FnOnce(&mut TarWriter) -> NiaoResult<ValueRef>,
{
    ARCHIVES.with(|a| {
        let mut a = a.borrow_mut();
        match a.get_mut(&id) {
            Some(TarEntry {
                state: HandleState::Writer(w),
            }) => f(w),
            Some(_) => Ok(ntar_err(span, "handle is open for reading, not writing")),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

// >>> ntar.open("pkg.tar.gz").handle > 0
// => true
fn ntar_open(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ntar_open", span)?;
    let path = path_arg(args, 0, "open", span)?;
    let (mode, compression, level) = parse_open_opts(optional_object(args, 1), span)?;
    let id = new_handle();
    match mode {
        OpenMode::Read => {
            let comp = compression.unwrap_or_else(|| detect_compression(&path));
            let opts = ReadOpts {
                compression: Some(comp),
                ..Default::default()
            };
            match TarReader::open_path(&path, &opts) {
                Ok(reader) => {
                    ARCHIVES.with(|a| {
                        a.borrow_mut().insert(
                            id,
                            TarEntry {
                                state: HandleState::Reader(reader),
                            },
                        );
                    });
                    Ok(handle_object(id, "r", comp, Some(&path)).ref_cell())
                }
                Err(e) => Ok(ntar_err(span, e.to_string())),
            }
        }
        OpenMode::Write => {
            let write_opts = WriteOpts {
                compression,
                level,
                mode: "w".into(),
            };
            match TarWriter::create_path(&path, &write_opts) {
                Ok(writer) => {
                    let comp = writer.compression();
                    ARCHIVES.with(|a| {
                        a.borrow_mut().insert(
                            id,
                            TarEntry {
                                state: HandleState::Writer(writer),
                            },
                        );
                    });
                    Ok(handle_object(id, "w", comp, Some(&path)).ref_cell())
                }
                Err(e) => Ok(ntar_err(span, e.to_string())),
            }
        }
        OpenMode::Append => {
            match TarWriter::append_path(&path) {
                Ok(writer) => {
                    ARCHIVES.with(|a| {
                        a.borrow_mut().insert(
                            id,
                            TarEntry {
                                state: HandleState::Writer(writer),
                            },
                        );
                    });
                    Ok(handle_object(id, "a", Compression::None, Some(&path)).ref_cell())
                }
                Err(e) => Ok(ntar_err(span, e.to_string())),
            }
        }
    }
}

// >>> ntar.close(handle)
// => nil
fn ntar_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntar_close", span)?;
    let id = int_arg(args, 0, "close", span)?;
    ARCHIVES.with(|a| {
        let mut a = a.borrow_mut();
        match a.remove(&id) {
            Some(TarEntry {
                state: HandleState::Writer(mut w),
            }) => match w.finish() {
                Ok(()) => Ok(ok_nil()),
                Err(e) => Ok(ntar_err(span, e.to_string())),
            },
            Some(TarEntry {
                state: HandleState::Reader(_),
            }) => Ok(ok_nil()),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

// >>> len(ntar.names(handle))
// => 1
fn ntar_names(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntar_names", span)?;
    let id = int_arg(args, 0, "names", span)?;
    with_reader(id, span, |r| {
        let names = r.names();
        let arr = names.into_iter().map(|n| ok_string(n)).collect();
        Ok(Value::Array(arr).ref_cell())
    })
}

// >>> ntar.get(handle, "a.txt").name
// => "a.txt"
fn ntar_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ntar_get", span)?;
    let id = int_arg(args, 0, "get", span)?;
    let name = string_arg(args, 1, "get", span)?;
    with_reader(id, span, |r| match r.get(&name) {
        Ok(info) => Ok(info_object(info).ref_cell()),
        Err(e) => Ok(not_found(span, e.to_string())),
    })
}

// >>> ntar.contains(handle, "a.txt")
// => true
fn ntar_contains(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ntar_contains", span)?;
    let id = int_arg(args, 0, "contains", span)?;
    let name = string_arg(args, 1, "contains", span)?;
    with_reader(id, span, |r| Ok(ok_bool(r.contains(&name))))
}

// >>> len(ntar.read(handle, "a.txt"))
// => 5
fn ntar_read(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntar_read", span)?;
    let id = int_arg(args, 0, "read", span)?;
    let name = string_arg(args, 1, "read", span)?;
    let max = if args.len() > 2 {
        int_arg(args, 2, "read", span)? as usize
    } else {
        MAX_ENTRY_BYTES
    };
    with_reader(id, span, |r| match r.read(&name, max) {
        Ok(data) => bytes_val(data),
        Err(e) => Ok(not_found(span, e.to_string())),
    })
}

// >>> ntar.members(handle)[0].name
fn ntar_members(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntar_members", span)?;
    let id = int_arg(args, 0, "members", span)?;
    with_reader(id, span, |r| {
        let arr = r.members().iter().map(info_object).map(|v| v.ref_cell()).collect();
        Ok(Value::Array(arr).ref_cell())
    })
}

// >>> ntar.next(handle).name
fn ntar_next(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntar_next", span)?;
    let id = int_arg(args, 0, "next", span)?;
    ARCHIVES.with(|a| {
        let mut a = a.borrow_mut();
        match a.get_mut(&id) {
            Some(TarEntry {
                state: HandleState::Reader(r),
            }) => match r.next_info() {
                Some(info) => Ok(info_object(info).ref_cell()),
                None => Ok(Value::Nil.ref_cell()),
            },
            Some(_) => Ok(ntar_err(span, "handle is open for writing")),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

// >>> ntar.rewind(handle)
fn ntar_rewind(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntar_rewind", span)?;
    let id = int_arg(args, 0, "rewind", span)?;
    ARCHIVES.with(|a| {
        let mut a = a.borrow_mut();
        match a.get_mut(&id) {
            Some(TarEntry {
                state: HandleState::Reader(r),
            }) => {
                r.rewind();
                Ok(ok_nil())
            }
            Some(_) => Ok(ntar_err(span, "handle is open for writing")),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

// >>> ntar.extract(handle, "a.txt", "out/")
fn ntar_extract(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "ntar_extract", span)?;
    let id = int_arg(args, 0, "extract", span)?;
    let member = string_arg(args, 1, "extract", span)?;
    let dest = path_arg(args, 2, "extract", span)?;
    let opts = parse_extract_opts(optional_object(args, 3), span)?;
    ARCHIVES.with(|a| {
        let a = a.borrow();
        match a.get(&id) {
            Some(TarEntry {
                state: HandleState::Reader(r),
            }) => match extract_member(r, &member, &dest, &opts) {
                Ok(()) => Ok(ok_nil()),
                Err(e) => Ok(ntar_err(span, e.to_string())),
            },
            Some(_) => Ok(ntar_err(span, "handle is open for writing")),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

// >>> len(ntar.extract_all(handle, "out/"))
fn ntar_extract_all(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntar_extract_all", span)?;
    let id = int_arg(args, 0, "extract_all", span)?;
    let dest = path_arg(args, 1, "extract_all", span)?;
    let opts = parse_extract_opts(optional_object(args, 2), span)?;
    ARCHIVES.with(|a| {
        let a = a.borrow();
        match a.get(&id) {
            Some(TarEntry {
                state: HandleState::Reader(r),
            }) => match extract_all(r, &dest, &opts) {
                Ok(names) => {
                    let arr = names.into_iter().map(ok_string).collect();
                    Ok(Value::Array(arr).ref_cell())
                }
                Err(e) => Ok(ntar_err(span, e.to_string())),
            },
            Some(_) => Ok(ntar_err(span, "handle is open for writing")),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

// >>> ntar.add(handle, "src.txt", {arcname: "pkg/src.txt"})
fn ntar_add(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntar_add", span)?;
    let id = int_arg(args, 0, "add", span)?;
    let path = path_arg(args, 1, "add", span)?;
    let opts = parse_add_opts(optional_object(args, 2), span)?;
    with_writer(id, span, |w| match w.add_path(&path, &opts) {
        Ok(()) => Ok(ok_nil()),
        Err(e) => Ok(ntar_err(span, e.to_string())),
    })
}

// >>> ntar.add_bytes(handle, "hello.txt", [104, 105])
fn ntar_add_bytes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 4, "ntar_add_bytes", span)?;
    let id = int_arg(args, 0, "add_bytes", span)?;
    let arcname = string_arg(args, 1, "add_bytes", span)?;
    let data = bytes_arg(args, 2, "add_bytes", span)?;
    let mode = if args.len() > 3 {
        Some(int_arg(args, 3, "add_bytes", span)? as u32)
    } else {
        None
    };
    with_writer(id, span, |w| match w.add_bytes(&arcname, &data, mode) {
        Ok(()) => Ok(ok_nil()),
        Err(e) => Ok(ntar_err(span, e.to_string())),
    })
}

// >>> ntar.add_dir(handle, "pkg", {arcname: "pkg"})
fn ntar_add_dir(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntar_add_dir", span)?;
    let id = int_arg(args, 0, "add_dir", span)?;
    let path = path_arg(args, 1, "add_dir", span)?;
    let opts = parse_add_opts(optional_object(args, 2), span)?;
    with_writer(id, span, |w| match w.add_dir(&path, &opts) {
        Ok(()) => Ok(ok_nil()),
        Err(e) => Ok(ntar_err(span, e.to_string())),
    })
}

// >>> ntar.add_tree(handle, "src/", {arcname: "pkg"})
fn ntar_add_tree(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntar_add_tree", span)?;
    let id = int_arg(args, 0, "add_tree", span)?;
    let path = path_arg(args, 1, "add_tree", span)?;
    let opts = parse_add_opts(optional_object(args, 2), span)?;
    with_writer(id, span, |w| match w.add_tree(&path, &opts) {
        Ok(()) => Ok(ok_nil()),
        Err(e) => Ok(ntar_err(span, e.to_string())),
    })
}

// >>> ntar.is_tar("pkg.tar.gz")
// => true
fn ntar_is_tar(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntar_is_tar", span)?;
    let path = path_arg(args, 0, "is_tar", span)?;
    if is_tar_path(&path) {
        return Ok(ok_bool(true));
    }
    match is_tar_file(&path) {
        Ok(b) => Ok(ok_bool(b)),
        Err(e) => Ok(ntar_err(span, e.to_string())),
    }
}

// >>> ntar.detect("pkg.tar.zst")
// => "zst"
fn ntar_detect(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntar_detect", span)?;
    let path = path_arg(args, 0, "detect", span)?;
    Ok(ok_string(detect_compression(&path).as_str()))
}

// >>> ntar.unpack("pkg.tar.gz", "out/")
fn ntar_unpack(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntar_unpack", span)?;
    let archive = path_arg(args, 0, "unpack", span)?;
    let dest = path_arg(args, 1, "unpack", span)?;
    let opts = parse_extract_opts(optional_object(args, 2), span)?;
    match unpack(&archive, &dest, &opts) {
        Ok(names) => {
            let arr = names.into_iter().map(ok_string).collect();
            Ok(Value::Array(arr).ref_cell())
        }
        Err(e) => Ok(ntar_err(span, e.to_string())),
    }
}

// >>> ntar.pack_tree("src/", "out.tar.gz", {arcname: "pkg"})
fn ntar_pack_tree(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntar_pack_tree", span)?;
    let src = path_arg(args, 0, "pack_tree", span)?;
    let archive = path_arg(args, 1, "pack_tree", span)?;
    let map = optional_object(args, 2);
    let arcname = map
        .as_ref()
        .and_then(|m| m.get("arcname"))
        .map(|v| string_arg(&[v.clone()], 0, "arcname", span))
        .transpose()?;
    let mut write_opts = WriteOpts::default();
    if let Some(ref m) = map {
        write_opts.compression = compression_from_obj(m, span)?;
        if let Some(v) = m.get("level") {
            write_opts.level = int_arg(&[v.clone()], 0, "level", span)? as i32;
        }
    }
    match pack_tree(&src, &archive, arcname.as_deref(), &write_opts) {
        Ok(()) => Ok(ok_nil()),
        Err(e) => Ok(ntar_err(span, e.to_string())),
    }
}

// >>> ntar.create(["a.txt"], "out.tar")
fn ntar_create(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntar_create", span)?;
    let paths_val = &args[0];
    let archive = path_arg(args, 1, "create", span)?;
    let paths: Vec<PathBuf> = match &*paths_val.borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match &*item.borrow() {
                    Value::String(s) => out.push(PathBuf::from(s)),
                    other => {
                        return Err(type_err(
                            span,
                            format!("create() expects string paths, got {}", other.type_name()),
                        ));
                    }
                }
            }
            out
        }
        other => {
            return Err(type_err(
                span,
                format!(
                    "create() expects an array as argument 1, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    let mut write_opts = WriteOpts::default();
    if let Some(m) = optional_object(args, 2) {
        write_opts.compression = compression_from_obj(&m, span)?;
        if let Some(v) = m.get("level") {
            write_opts.level = int_arg(&[v.clone()], 0, "level", span)? as i32;
        }
    }
    match create_archive(&paths, &archive, &write_opts) {
        Ok(()) => Ok(ok_nil()),
        Err(e) => Ok(ntar_err(span, e.to_string())),
    }
}

macro_rules! ntar_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ntar_fns![
    ("ntar_open", "open", ntar_open),
    ("ntar_close", "close", ntar_close),
    ("ntar_names", "names", ntar_names),
    ("ntar_get", "get", ntar_get),
    ("ntar_contains", "contains", ntar_contains),
    ("ntar_read", "read", ntar_read),
    ("ntar_members", "members", ntar_members),
    ("ntar_next", "next", ntar_next),
    ("ntar_rewind", "rewind", ntar_rewind),
    ("ntar_extract", "extract", ntar_extract),
    ("ntar_extract_all", "extract_all", ntar_extract_all),
    ("ntar_add", "add", ntar_add),
    ("ntar_add_bytes", "add_bytes", ntar_add_bytes),
    ("ntar_add_dir", "add_dir", ntar_add_dir),
    ("ntar_add_tree", "add_tree", ntar_add_tree),
    ("ntar_is_tar", "is_tar", ntar_is_tar),
    ("ntar_detect", "detect", ntar_detect),
    ("ntar_unpack", "unpack", ntar_unpack),
    ("ntar_pack_tree", "pack_tree", ntar_pack_tree),
    ("ntar_create", "create", ntar_create),
];

pub const MODULE_NAME: &str = "ntar";
pub const MODULE_PATHS: &[&str] = &["ntar", "std/ntar"];

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
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    fn s(v: &str) -> ValueRef {
        Value::String(v.to_string()).ref_cell()
    }

    #[test]
    fn detect_tar_gz() {
        let out = ntar_detect(&[s("archive.tar.gz")], span()).unwrap();
        match &*out.borrow() {
            Value::String(c) => assert_eq!(c, "gz"),
            other => panic!("expected string, got {other:?}"),
        }
    }
}
