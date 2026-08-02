//! Native `nblob` standard library — unified object-store VFS: local dir, S3,
//! Azure Blob, GCS behind one open/read/write/list API (~fsspec / smart_open).
//!
//! Import with `import "nblob"` (or `import "std/nblob"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_nblob::{
    fs_azure, fs_from_uri, fs_gcs, fs_local, fs_memory, fs_s3, global_vfs, join as uri_join,
    parse as uri_parse, scheme_of, AzureOpts, FsHandle, GcsOpts, OpenFile, OpenMode, S3Opts, Vfs,
};
use niao_codec::base64;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub const MODULE_NAME: &str = "nblob";
pub const MODULE_PATHS: &[&str] = &["nblob", "std/nblob"];

thread_local! {
    static FS: RefCell<HashMap<i64, FsHandle>> = RefCell::new(HashMap::new());
    static FILES: RefCell<HashMap<i64, OpenFile>> = RefCell::new(HashMap::new());
    static NEXT_FS: RefCell<i64> = const { RefCell::new(1) };
    static NEXT_FILE: RefCell<i64> = const { RefCell::new(1) };
    static VFS: RefCell<Vfs> = RefCell::new(Vfs::default());
}

fn alloc_fs(h: FsHandle) -> i64 {
    NEXT_FS.with(|n| {
        let id = *n.borrow();
        *n.borrow_mut() = id + 1;
        FS.with(|m| m.borrow_mut().insert(id, h));
        id
    })
}

fn alloc_file(f: OpenFile) -> i64 {
    NEXT_FILE.with(|n| {
        let id = *n.borrow();
        *n.borrow_mut() = id + 1;
        FILES.with(|m| m.borrow_mut().insert(id, f));
        id
    })
}

fn get_fs(id: i64, span: Span) -> NiaoResult<FsHandle> {
    FS.with(|m| {
        m.borrow()
            .get(&id)
            .cloned()
            .ok_or_else(|| {
                RuntimeError::at(
                    span,
                    codes::E4573_NBLOB_INVALID_HANDLE,
                    format!("nblob: invalid fs handle {id}"),
                )
            })
    })
}

fn blob_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E4571_NBLOB_ERROR, "nblob_error", msg.into(), span)
}

fn map_err(span: Span, e: niao_nblob::BlobError) -> ValueRef {
    blob_err(span, e.message)
}

fn arity_range(args: &[ValueRef], lo: usize, hi: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < lo || args.len() > hi {
        return Err(RuntimeError::at(
            span,
            codes::E4570_NBLOB_ARITY,
            format!("{name}() expects {lo}-{hi} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E4570_NBLOB_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E4572_NBLOB_TYPE, msg.into())
}

fn str_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects string at arg {}, got {}",
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
                "{name}() expects int at arg {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn opt_obj<'a>(args: &'a [ValueRef], idx: usize) -> Option<HashMap<String, ValueRef>> {
    args.get(idx).and_then(|v| match &*v.borrow() {
        Value::Object(m) => Some(m.clone()),
        _ => None,
    })
}

fn obj_str(m: &HashMap<String, ValueRef>, key: &str) -> Option<String> {
    m.get(key).and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.clone()),
        _ => None,
    })
}

fn obj_bool(m: &HashMap<String, ValueRef>, key: &str, default: bool) -> bool {
    m.get(key)
        .and_then(|v| match &*v.borrow() {
            Value::Bool(b) => Some(*b),
            _ => None,
        })
        .unwrap_or(default)
}

fn bytes_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<u8>> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        Value::ByteArray(b) => Ok(b.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects string|bytes at arg {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn ok_str(s: impl Into<String>) -> ValueRef {
    Value::String(s.into()).ref_cell()
}

fn ok_int(n: i64) -> ValueRef {
    Value::Int(n).ref_cell()
}

fn ok_bool(b: bool) -> ValueRef {
    Value::Bool(b).ref_cell()
}

fn entry_to_value(e: &niao_nblob::Entry) -> ValueRef {
    let mut m = HashMap::new();
    m.insert("name".into(), ok_str(e.name.clone()));
    m.insert("type".into(), ok_str(e.kind.clone()));
    m.insert("size".into(), ok_int(e.size as i64));
    if let Some(t) = e.mtime {
        m.insert("mtime".into(), ok_int(t));
    }
    Value::Object(m).ref_cell()
}

fn entries_to_value(ents: &[niao_nblob::Entry]) -> ValueRef {
    Value::Array(ents.iter().map(entry_to_value).collect()).ref_cell()
}

fn with_vfs<R>(f: impl FnOnce(&Vfs) -> R) -> R {
    VFS.with(|v| f(&v.borrow()))
}

fn set_vfs_s3(opts: S3Opts) {
    VFS.with(|v| v.borrow_mut().default_s3 = Some(opts.clone()));
    if let Ok(mut g) = global_vfs().lock() {
        g.default_s3 = Some(opts);
    }
}

fn set_vfs_azure(opts: AzureOpts) {
    VFS.with(|v| v.borrow_mut().default_azure = Some(opts.clone()));
    if let Ok(mut g) = global_vfs().lock() {
        g.default_azure = Some(opts);
    }
}

fn set_vfs_gcs(opts: GcsOpts) {
    VFS.with(|v| v.borrow_mut().default_gcs = Some(opts.clone()));
    if let Ok(mut g) = global_vfs().lock() {
        g.default_gcs = Some(opts);
    }
}

fn parse_s3_opts(m: &HashMap<String, ValueRef>, span: Span) -> NiaoResult<S3Opts> {
    let region = obj_str(m, "region").unwrap_or_else(|| "us-east-1".into());
    let access_key = obj_str(m, "access_key").ok_or_else(|| {
        type_err(span, "s3 opts require access_key")
    })?;
    let secret_key = obj_str(m, "secret_key").ok_or_else(|| {
        type_err(span, "s3 opts require secret_key")
    })?;
    Ok(S3Opts {
        region,
        access_key,
        secret_key,
        session_token: obj_str(m, "session_token"),
        endpoint: obj_str(m, "endpoint"),
        default_bucket: obj_str(m, "bucket"),
    })
}

fn parse_azure_opts(m: &HashMap<String, ValueRef>, span: Span) -> NiaoResult<AzureOpts> {
    let account = obj_str(m, "account").ok_or_else(|| type_err(span, "azure opts require account"))?;
    let key = obj_str(m, "key")
        .map(|s| base64::decode_standard(&s).unwrap_or_else(|_| s.into_bytes()));
    Ok(AzureOpts {
        account,
        key,
        sas: obj_str(m, "sas"),
        bearer: obj_str(m, "bearer").or_else(|| obj_str(m, "token")),
        default_container: obj_str(m, "container"),
    })
}

fn parse_gcs_opts(m: &HashMap<String, ValueRef>, span: Span) -> NiaoResult<GcsOpts> {
    let access_token = obj_str(m, "access_token")
        .or_else(|| obj_str(m, "token"))
        .ok_or_else(|| type_err(span, "gcs opts require access_token"))?;
    Ok(GcsOpts {
        access_token,
        project: obj_str(m, "project"),
        default_bucket: obj_str(m, "bucket"),
    })
}

// ---------------------------------------------------------------------------
// URI helpers
// ---------------------------------------------------------------------------

/// nblob.parse(uri) → {scheme, netloc, path, bucket, key, uri}
// >>> import "nblob"
// >>> let u = nblob.parse("s3://bucket/a/b.txt")
// >>> u.scheme
// => "s3"
fn nblob_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nblob_parse", span)?;
    let uri = str_arg(args, 0, "nblob_parse", span)?;
    match uri_parse(&uri) {
        Ok(u) => {
            let mut m = HashMap::new();
            m.insert("scheme".into(), ok_str(u.scheme.clone()));
            m.insert("netloc".into(), ok_str(u.netloc.clone()));
            m.insert("path".into(), ok_str(u.path.clone()));
            m.insert("bucket".into(), ok_str(u.netloc.clone()));
            m.insert("key".into(), ok_str(u.path.clone()));
            m.insert("uri".into(), ok_str(u.to_uri_string()));
            Ok(Value::Object(m).ref_cell())
        }
        Err(e) => Ok(map_err(span, e)),
    }
}

/// nblob.join(base, child) → string
// >>> import "nblob"
// >>> nblob.join("s3://b/pre", "x.txt")
// => "s3://b/pre/x.txt"
fn nblob_join(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nblob_join", span)?;
    let base = str_arg(args, 0, "nblob_join", span)?;
    let child = str_arg(args, 1, "nblob_join", span)?;
    match uri_join(&base, &child) {
        Ok(s) => Ok(ok_str(s)),
        Err(e) => Ok(map_err(span, e)),
    }
}

/// nblob.scheme(uri) → string
// >>> import "nblob"
// >>> nblob.scheme("gs://bucket/obj")
// => "gs"
fn nblob_scheme(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nblob_scheme", span)?;
    let uri = str_arg(args, 0, "nblob_scheme", span)?;
    match scheme_of(&uri) {
        Ok(s) => Ok(ok_str(s)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// ---------------------------------------------------------------------------
// FS factories
// ---------------------------------------------------------------------------

/// nblob.local(root?) → fs handle
// >>> import "nblob"
// >>> let fs = nblob.local()
// >>> type(fs)
fn nblob_local(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nblob_local", span)?;
    let root = if args.is_empty() {
        None
    } else {
        Some(str_arg(args, 0, "nblob_local", span)?)
    };
    Ok(ok_int(alloc_fs(fs_local(root.as_deref()))))
}

/// nblob.memory(name?) → fs handle
// >>> import "nblob"
// >>> let fs = nblob.memory("t")
// >>> nblob.fs_write(fs, "a.txt", "hi")
// >>> nblob.fs_read(fs, "a.txt")
// => "hi"
fn nblob_memory(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nblob_memory", span)?;
    let name = if args.is_empty() {
        None
    } else {
        Some(str_arg(args, 0, "nblob_memory", span)?)
    };
    Ok(ok_int(alloc_fs(fs_memory(name.as_deref()))))
}

/// nblob.s3(opts) → fs handle
fn nblob_s3(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nblob_s3", span)?;
    let m = opt_obj(args, 0).ok_or_else(|| type_err(span, "nblob_s3() expects opts object"))?;
    let opts = parse_s3_opts(&m, span)?;
    set_vfs_s3(opts.clone());
    let bucket = obj_str(&m, "bucket");
    match fs_s3(opts, bucket.as_deref()) {
        Ok(h) => Ok(ok_int(alloc_fs(h))),
        Err(e) => Ok(map_err(span, e)),
    }
}

/// nblob.azure(opts) → fs handle
fn nblob_azure(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nblob_azure", span)?;
    let m = opt_obj(args, 0).ok_or_else(|| type_err(span, "nblob_azure() expects opts object"))?;
    let opts = parse_azure_opts(&m, span)?;
    set_vfs_azure(opts.clone());
    let container = obj_str(&m, "container");
    match fs_azure(opts, container.as_deref()) {
        Ok(h) => Ok(ok_int(alloc_fs(h))),
        Err(e) => Ok(map_err(span, e)),
    }
}

/// nblob.gcs(opts) → fs handle
fn nblob_gcs(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nblob_gcs", span)?;
    let m = opt_obj(args, 0).ok_or_else(|| type_err(span, "nblob_gcs() expects opts object"))?;
    let opts = parse_gcs_opts(&m, span)?;
    set_vfs_gcs(opts.clone());
    let bucket = obj_str(&m, "bucket");
    match fs_gcs(opts, bucket.as_deref()) {
        Ok(h) => Ok(ok_int(alloc_fs(h))),
        Err(e) => Ok(map_err(span, e)),
    }
}

/// nblob.fs(uri_or_opts) → fs handle
// >>> import "nblob"
// >>> let fs = nblob.fs({scheme: "memory", name: "doc"})
// >>> type(fs)
fn nblob_fs(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nblob_fs", span)?;
    match &*args[0].borrow() {
        Value::String(uri) => with_vfs(|v| match fs_from_uri(v, uri) {
            Ok(h) => Ok(ok_int(alloc_fs(h))),
            Err(e) => Ok(map_err(span, e)),
        }),
        Value::Object(m) => {
            let scheme = obj_str(m, "scheme").unwrap_or_else(|| "local".into());
            match scheme.as_str() {
                "local" | "file" => {
                    let root = obj_str(m, "root");
                    Ok(ok_int(alloc_fs(fs_local(root.as_deref()))))
                }
                "memory" => {
                    let name = obj_str(m, "name");
                    Ok(ok_int(alloc_fs(fs_memory(name.as_deref()))))
                }
                "s3" => {
                    let opts = parse_s3_opts(m, span)?;
                    set_vfs_s3(opts.clone());
                    match fs_s3(opts, obj_str(m, "bucket").as_deref()) {
                        Ok(h) => Ok(ok_int(alloc_fs(h))),
                        Err(e) => Ok(map_err(span, e)),
                    }
                }
                "azure" | "az" => {
                    let opts = parse_azure_opts(m, span)?;
                    set_vfs_azure(opts.clone());
                    match fs_azure(opts, obj_str(m, "container").as_deref()) {
                        Ok(h) => Ok(ok_int(alloc_fs(h))),
                        Err(e) => Ok(map_err(span, e)),
                    }
                }
                "gcs" | "gs" => {
                    let opts = parse_gcs_opts(m, span)?;
                    set_vfs_gcs(opts.clone());
                    match fs_gcs(opts, obj_str(m, "bucket").as_deref()) {
                        Ok(h) => Ok(ok_int(alloc_fs(h))),
                        Err(e) => Ok(map_err(span, e)),
                    }
                }
                other => Ok(blob_err(span, format!("unsupported fs scheme: {other}"))),
            }
        }
        other => Err(type_err(
            span,
            format!("nblob_fs() expects string|object, got {}", other.type_name()),
        )),
    }
}

/// nblob.close_fs(fs) → bool
// >>> import "nblob"
// >>> let fs = nblob.memory()
// >>> nblob.close_fs(fs)
// => true
fn nblob_close_fs(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nblob_close_fs", span)?;
    let id = int_arg(args, 0, "nblob_close_fs", span)?;
    let removed = FS.with(|m| m.borrow_mut().remove(&id).is_some());
    Ok(ok_bool(removed))
}

// ---------------------------------------------------------------------------
// URI-level ops
// ---------------------------------------------------------------------------

/// nblob.read(uri) → string
// >>> import "nblob"
// >>> let fs = nblob.memory("r1")
// >>> nblob.fs_write(fs, "x", "abc")
// >>> nblob.read("memory://r1/x")
// => "abc"
fn nblob_read(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nblob_read", span)?;
    let uri = str_arg(args, 0, "nblob_read", span)?;
    with_vfs(|v| match v.read_uri(&uri) {
        Ok(b) => Ok(ok_str(String::from_utf8_lossy(&b).into_owned())),
        Err(e) => Ok(map_err(span, e)),
    })
}

/// nblob.write(uri, data, opts?) → int
// >>> import "nblob"
// >>> nblob.write("memory://w/x", "data")
// => 4
fn nblob_write(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nblob_write", span)?;
    let uri = str_arg(args, 0, "nblob_write", span)?;
    let data = bytes_arg(args, 1, "nblob_write", span)?;
    let ct = opt_obj(args, 2).and_then(|m| obj_str(&m, "content_type"));
    with_vfs(|v| match v.write_uri(&uri, &data, ct.as_deref()) {
        Ok(n) => Ok(ok_int(n as i64)),
        Err(e) => Ok(map_err(span, e)),
    })
}

/// nblob.exists(uri) → bool
// >>> import "nblob"
// >>> nblob.write("memory://ex/f", "1")
// >>> nblob.exists("memory://ex/f")
// => true
fn nblob_exists(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nblob_exists", span)?;
    let uri = str_arg(args, 0, "nblob_exists", span)?;
    with_vfs(|v| match v.exists_uri(&uri) {
        Ok(b) => Ok(ok_bool(b)),
        Err(e) => Ok(map_err(span, e)),
    })
}

/// nblob.info(uri) → {name, type, size, mtime?}
fn nblob_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nblob_info", span)?;
    let uri = str_arg(args, 0, "nblob_info", span)?;
    with_vfs(|v| match v.info_uri(&uri) {
        Ok(e) => Ok(entry_to_value(&e)),
        Err(e) => Ok(map_err(span, e)),
    })
}

/// nblob.ls(uri, opts?) → [{name, type, size}, ...]
fn nblob_ls(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nblob_ls", span)?;
    let uri = str_arg(args, 0, "nblob_ls", span)?;
    let detail = opt_obj(args, 1)
        .map(|m| obj_bool(&m, "detail", false))
        .unwrap_or(false);
    with_vfs(|v| match v.list_uri(&uri, detail) {
        Ok(e) => Ok(entries_to_value(&e)),
        Err(e) => Ok(map_err(span, e)),
    })
}

/// nblob.list(uri, opts?) — alias of ls
fn nblob_list(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    nblob_ls(args, span)
}

/// nblob.rm(uri) → true
fn nblob_rm(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nblob_rm", span)?;
    let uri = str_arg(args, 0, "nblob_rm", span)?;
    with_vfs(|v| match v.remove_uri(&uri) {
        Ok(()) => Ok(ok_bool(true)),
        Err(e) => Ok(map_err(span, e)),
    })
}

/// nblob.mkdir(uri) → true
fn nblob_mkdir(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nblob_mkdir", span)?;
    let uri = str_arg(args, 0, "nblob_mkdir", span)?;
    with_vfs(|v| match v.mkdir_uri(&uri) {
        Ok(()) => Ok(ok_bool(true)),
        Err(e) => Ok(map_err(span, e)),
    })
}

/// nblob.cp(src, dst) → true
fn nblob_cp(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nblob_cp", span)?;
    let src = str_arg(args, 0, "nblob_cp", span)?;
    let dst = str_arg(args, 1, "nblob_cp", span)?;
    with_vfs(|v| match v.copy_uri(&src, &dst) {
        Ok(()) => Ok(ok_bool(true)),
        Err(e) => Ok(map_err(span, e)),
    })
}

/// nblob.mv(src, dst) → true
fn nblob_mv(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nblob_mv", span)?;
    let src = str_arg(args, 0, "nblob_mv", span)?;
    let dst = str_arg(args, 1, "nblob_mv", span)?;
    with_vfs(|v| match v.move_uri(&src, &dst) {
        Ok(()) => Ok(ok_bool(true)),
        Err(e) => Ok(map_err(span, e)),
    })
}

/// nblob.put(local_path, uri) → true
fn nblob_put(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nblob_put", span)?;
    let local = str_arg(args, 0, "nblob_put", span)?;
    let uri = str_arg(args, 1, "nblob_put", span)?;
    with_vfs(|v| match v.copy_uri(&local, &uri) {
        Ok(()) => Ok(ok_bool(true)),
        Err(e) => Ok(map_err(span, e)),
    })
}

/// nblob.get(uri, local_path) → true
fn nblob_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nblob_get", span)?;
    let uri = str_arg(args, 0, "nblob_get", span)?;
    let local = str_arg(args, 1, "nblob_get", span)?;
    with_vfs(|v| match v.copy_uri(&uri, &local) {
        Ok(()) => Ok(ok_bool(true)),
        Err(e) => Ok(map_err(span, e)),
    })
}

/// nblob.open(uri, mode?, opts?) → file handle
// >>> import "nblob"
// >>> nblob.write("memory://op/f.txt", "xyz")
// >>> let f = nblob.open("memory://op/f.txt", "r")
// >>> nblob.read_bytes(f)
// => "xyz"
fn nblob_open(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nblob_open", span)?;
    let uri = str_arg(args, 0, "nblob_open", span)?;
    let mode_s = if args.len() >= 2 {
        match &*args[1].borrow() {
            Value::String(s) => s.clone(),
            Value::Nil => "r".into(),
            other => {
                return Err(type_err(
                    span,
                    format!("nblob_open() mode must be string, got {}", other.type_name()),
                ))
            }
        }
    } else {
        "r".into()
    };
    let mode = match OpenMode::parse(&mode_s) {
        Ok(m) => m,
        Err(e) => return Ok(map_err(span, e)),
    };
    with_vfs(|v| match v.open_uri(&uri, mode) {
        Ok(f) => Ok(ok_int(alloc_file(f))),
        Err(e) => Ok(map_err(span, e)),
    })
}

// ---------------------------------------------------------------------------
// FS-relative ops
// ---------------------------------------------------------------------------

fn nblob_fs_read(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nblob_fs_read", span)?;
    let id = int_arg(args, 0, "nblob_fs_read", span)?;
    let path = str_arg(args, 1, "nblob_fs_read", span)?;
    let fs = get_fs(id, span)?;
    match fs.store.read(&path) {
        Ok(b) => Ok(ok_str(String::from_utf8_lossy(&b).into_owned())),
        Err(e) => Ok(map_err(span, e)),
    }
}

fn nblob_fs_write(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nblob_fs_write", span)?;
    let id = int_arg(args, 0, "nblob_fs_write", span)?;
    let path = str_arg(args, 1, "nblob_fs_write", span)?;
    let data = bytes_arg(args, 2, "nblob_fs_write", span)?;
    let ct = opt_obj(args, 3).and_then(|m| obj_str(&m, "content_type"));
    let fs = get_fs(id, span)?;
    match fs.store.write(&path, &data, ct.as_deref()) {
        Ok(n) => Ok(ok_int(n as i64)),
        Err(e) => Ok(map_err(span, e)),
    }
}

fn nblob_fs_exists(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nblob_fs_exists", span)?;
    let id = int_arg(args, 0, "nblob_fs_exists", span)?;
    let path = str_arg(args, 1, "nblob_fs_exists", span)?;
    let fs = get_fs(id, span)?;
    match fs.store.exists(&path) {
        Ok(b) => Ok(ok_bool(b)),
        Err(e) => Ok(map_err(span, e)),
    }
}

fn nblob_fs_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nblob_fs_info", span)?;
    let id = int_arg(args, 0, "nblob_fs_info", span)?;
    let path = str_arg(args, 1, "nblob_fs_info", span)?;
    let fs = get_fs(id, span)?;
    match fs.store.info(&path) {
        Ok(e) => Ok(entry_to_value(&e)),
        Err(e) => Ok(map_err(span, e)),
    }
}

fn nblob_fs_ls(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nblob_fs_ls", span)?;
    let id = int_arg(args, 0, "nblob_fs_ls", span)?;
    let path = if args.len() >= 2 {
        match &*args[1].borrow() {
            Value::String(s) => s.clone(),
            Value::Nil => String::new(),
            Value::Object(_) => String::new(),
            other => {
                return Err(type_err(
                    span,
                    format!("nblob_fs_ls() path must be string, got {}", other.type_name()),
                ))
            }
        }
    } else {
        String::new()
    };
    let detail = if args.len() == 3 {
        opt_obj(args, 2)
            .map(|m| obj_bool(&m, "detail", false))
            .unwrap_or(false)
    } else if args.len() == 2 {
        opt_obj(args, 1)
            .map(|m| obj_bool(&m, "detail", false))
            .unwrap_or(false)
    } else {
        false
    };
    let fs = get_fs(id, span)?;
    match fs.store.list(&path, detail) {
        Ok(e) => Ok(entries_to_value(&e)),
        Err(e) => Ok(map_err(span, e)),
    }
}

fn nblob_fs_rm(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nblob_fs_rm", span)?;
    let id = int_arg(args, 0, "nblob_fs_rm", span)?;
    let path = str_arg(args, 1, "nblob_fs_rm", span)?;
    let fs = get_fs(id, span)?;
    match fs.store.remove(&path) {
        Ok(()) => Ok(ok_bool(true)),
        Err(e) => Ok(map_err(span, e)),
    }
}

fn nblob_fs_mkdir(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nblob_fs_mkdir", span)?;
    let id = int_arg(args, 0, "nblob_fs_mkdir", span)?;
    let path = str_arg(args, 1, "nblob_fs_mkdir", span)?;
    let fs = get_fs(id, span)?;
    match fs.store.mkdir(&path) {
        Ok(()) => Ok(ok_bool(true)),
        Err(e) => Ok(map_err(span, e)),
    }
}

fn nblob_fs_open(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nblob_fs_open", span)?;
    let id = int_arg(args, 0, "nblob_fs_open", span)?;
    let path = str_arg(args, 1, "nblob_fs_open", span)?;
    let mode_s = if args.len() >= 3 {
        str_arg(args, 2, "nblob_fs_open", span)?
    } else {
        "r".into()
    };
    let mode = match OpenMode::parse(&mode_s) {
        Ok(m) => m,
        Err(e) => return Ok(map_err(span, e)),
    };
    let fs = get_fs(id, span)?;
    let buf = match mode {
        OpenMode::Read => match fs.store.read(&path) {
            Ok(b) => b,
            Err(e) => return Ok(map_err(span, e)),
        },
        OpenMode::Write => Vec::new(),
        OpenMode::Append => fs.store.read(&path).unwrap_or_default(),
    };
    let pos = if mode == OpenMode::Append {
        buf.len() as u64
    } else {
        0
    };
    Ok(ok_int(alloc_file(OpenFile {
        store: fs.store.clone(),
        key: path,
        mode,
        pos,
        buf,
        dirty: mode == OpenMode::Write,
    })))
}

// ---------------------------------------------------------------------------
// File handle ops
// ---------------------------------------------------------------------------

fn nblob_read_bytes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nblob_read_bytes", span)?;
    let id = int_arg(args, 0, "nblob_read_bytes", span)?;
    let n = if args.len() == 2 {
        Some(int_arg(args, 1, "nblob_read_bytes", span)? as usize)
    } else {
        None
    };
    FILES.with(|m| {
        let mut map = m.borrow_mut();
        let Some(f) = map.get_mut(&id) else {
            return Ok(error_value(
                codes::E4573_NBLOB_INVALID_HANDLE,
                "nblob_error",
                format!("invalid file handle {id}"),
                span,
            ));
        };
        match f.read(n) {
            Ok(b) => Ok(ok_str(String::from_utf8_lossy(&b).into_owned())),
            Err(e) => Ok(map_err(span, e)),
        }
    })
}

fn nblob_write_bytes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nblob_write_bytes", span)?;
    let id = int_arg(args, 0, "nblob_write_bytes", span)?;
    let data = bytes_arg(args, 1, "nblob_write_bytes", span)?;
    FILES.with(|m| {
        let mut map = m.borrow_mut();
        let Some(f) = map.get_mut(&id) else {
            return Ok(error_value(
                codes::E4573_NBLOB_INVALID_HANDLE,
                "nblob_error",
                format!("invalid file handle {id}"),
                span,
            ));
        };
        match f.write(&data) {
            Ok(n) => Ok(ok_int(n as i64)),
            Err(e) => Ok(map_err(span, e)),
        }
    })
}

fn nblob_tell(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nblob_tell", span)?;
    let id = int_arg(args, 0, "nblob_tell", span)?;
    FILES.with(|m| {
        let map = m.borrow();
        match map.get(&id) {
            Some(f) => Ok(ok_int(f.tell() as i64)),
            None => Ok(error_value(
                codes::E4573_NBLOB_INVALID_HANDLE,
                "nblob_error",
                format!("invalid file handle {id}"),
                span,
            )),
        }
    })
}

fn nblob_seek(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nblob_seek", span)?;
    let id = int_arg(args, 0, "nblob_seek", span)?;
    let offset = int_arg(args, 1, "nblob_seek", span)?;
    let whence = if args.len() == 3 {
        int_arg(args, 2, "nblob_seek", span)?
    } else {
        0
    };
    FILES.with(|m| {
        let mut map = m.borrow_mut();
        let Some(f) = map.get_mut(&id) else {
            return Ok(error_value(
                codes::E4573_NBLOB_INVALID_HANDLE,
                "nblob_error",
                format!("invalid file handle {id}"),
                span,
            ));
        };
        match f.seek(offset, whence) {
            Ok(p) => Ok(ok_int(p as i64)),
            Err(e) => Ok(map_err(span, e)),
        }
    })
}

fn nblob_flush(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nblob_flush", span)?;
    let id = int_arg(args, 0, "nblob_flush", span)?;
    FILES.with(|m| {
        let mut map = m.borrow_mut();
        let Some(f) = map.get_mut(&id) else {
            return Ok(error_value(
                codes::E4573_NBLOB_INVALID_HANDLE,
                "nblob_error",
                format!("invalid file handle {id}"),
                span,
            ));
        };
        match f.flush() {
            Ok(()) => Ok(ok_bool(true)),
            Err(e) => Ok(map_err(span, e)),
        }
    })
}

fn nblob_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nblob_close", span)?;
    let id = int_arg(args, 0, "nblob_close", span)?;
    let result = FILES.with(|m| {
        let mut map = m.borrow_mut();
        if let Some(mut f) = map.remove(&id) {
            if let Err(e) = f.flush() {
                return Err(e);
            }
            Ok(true)
        } else {
            Ok(false)
        }
    });
    match result {
        Ok(b) => Ok(ok_bool(b)),
        Err(e) => Ok(map_err(span, e)),
    }
}

fn nblob_size(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nblob_size", span)?;
    let id = int_arg(args, 0, "nblob_size", span)?;
    FILES.with(|m| {
        let map = m.borrow();
        match map.get(&id) {
            Some(f) => Ok(ok_int(f.size() as i64)),
            None => Ok(error_value(
                codes::E4573_NBLOB_INVALID_HANDLE,
                "nblob_error",
                format!("invalid file handle {id}"),
                span,
            )),
        }
    })
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nblob_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nblob_fns![
    ("nblob_parse", "parse", nblob_parse),
    ("nblob_join", "join", nblob_join),
    ("nblob_scheme", "scheme", nblob_scheme),
    ("nblob_local", "local", nblob_local),
    ("nblob_memory", "memory", nblob_memory),
    ("nblob_s3", "s3", nblob_s3),
    ("nblob_azure", "azure", nblob_azure),
    ("nblob_gcs", "gcs", nblob_gcs),
    ("nblob_fs", "fs", nblob_fs),
    ("nblob_close_fs", "close_fs", nblob_close_fs),
    ("nblob_open", "open", nblob_open),
    ("nblob_read", "read", nblob_read),
    ("nblob_write", "write", nblob_write),
    ("nblob_exists", "exists", nblob_exists),
    ("nblob_info", "info", nblob_info),
    ("nblob_ls", "ls", nblob_ls),
    ("nblob_list", "list", nblob_list),
    ("nblob_rm", "rm", nblob_rm),
    ("nblob_mkdir", "mkdir", nblob_mkdir),
    ("nblob_cp", "cp", nblob_cp),
    ("nblob_mv", "mv", nblob_mv),
    ("nblob_put", "put", nblob_put),
    ("nblob_get", "get", nblob_get),
    ("nblob_fs_read", "fs_read", nblob_fs_read),
    ("nblob_fs_write", "fs_write", nblob_fs_write),
    ("nblob_fs_exists", "fs_exists", nblob_fs_exists),
    ("nblob_fs_info", "fs_info", nblob_fs_info),
    ("nblob_fs_ls", "fs_ls", nblob_fs_ls),
    ("nblob_fs_rm", "fs_rm", nblob_fs_rm),
    ("nblob_fs_mkdir", "fs_mkdir", nblob_fs_mkdir),
    ("nblob_fs_open", "fs_open", nblob_fs_open),
    ("nblob_read_bytes", "read_bytes", nblob_read_bytes),
    ("nblob_write_bytes", "write_bytes", nblob_write_bytes),
    ("nblob_tell", "tell", nblob_tell),
    ("nblob_seek", "seek", nblob_seek),
    ("nblob_flush", "flush", nblob_flush),
    ("nblob_close", "close", nblob_close),
    ("nblob_size", "size", nblob_size),
];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

#[cfg(test)]
mod native_smoke {
    use super::*;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    fn s(v: &str) -> ValueRef {
        Value::String(v.to_string()).ref_cell()
    }

    #[test]
    fn parse_join_scheme() {
        let out = nblob_parse(&[s("s3://b/k")], span()).unwrap();
        match &*out.borrow() {
            Value::Object(m) => assert_eq!(
                match &*m["scheme"].borrow() {
                    Value::String(x) => x.as_str(),
                    _ => "",
                },
                "s3"
            ),
            _ => panic!("expected object"),
        }
        let j = nblob_join(&[s("memory://a"), s("b")], span()).unwrap();
        assert!(matches!(&*j.borrow(), Value::String(_)));
    }

    #[test]
    fn memory_read_write_open() {
        let fs = nblob_memory(&[s("smoke")], span()).unwrap();
        let id = match &*fs.borrow() {
            Value::Int(n) => *n,
            _ => panic!("fs handle"),
        };
        nblob_fs_write(
            &[Value::Int(id).ref_cell(), s("f.txt"), s("payload")],
            span(),
        )
        .unwrap();
        let r = nblob_read(&[s("memory://smoke/f.txt")], span()).unwrap();
        match &*r.borrow() {
            Value::String(t) => assert_eq!(t, "payload"),
            _ => panic!("string"),
        }
        let f = nblob_open(&[s("memory://smoke/f.txt"), s("r")], span()).unwrap();
        let fh = match &*f.borrow() {
            Value::Int(n) => *n,
            _ => panic!("file handle"),
        };
        let bytes = nblob_read_bytes(&[Value::Int(fh).ref_cell()], span()).unwrap();
        match &*bytes.borrow() {
            Value::String(t) => assert_eq!(t, "payload"),
            _ => panic!("bytes"),
        }
        nblob_close(&[Value::Int(fh).ref_cell()], span()).unwrap();
        nblob_close_fs(&[Value::Int(id).ref_cell()], span()).unwrap();
    }

    #[test]
    fn list_cp_rm() {
        nblob_write(&[s("memory://lc/a"), s("1")], span()).unwrap();
        nblob_write(&[s("memory://lc/b"), s("2")], span()).unwrap();
        let ls = nblob_ls(&[s("memory://lc")], span()).unwrap();
        match &*ls.borrow() {
            Value::Array(a) => assert_eq!(a.len(), 2),
            _ => panic!("array"),
        }
        nblob_cp(&[s("memory://lc/a"), s("memory://lc/c")], span()).unwrap();
        nblob_rm(&[s("memory://lc/b")], span()).unwrap();
        assert!(matches!(
            &*nblob_exists(&[s("memory://lc/b")], span()).unwrap().borrow(),
            Value::Bool(false)
        ));
    }
}
