//! Native nfs standard library — high-level file ops: copy/move trees, atomic
//! write, temp files/dirs, disk usage, trash (~`shutil` + `tempfile` + `send2trash`).
//!
//! Import with `import "nfs"` (or `import "std/nfs"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_nfs as nfs;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

// ---------------------------------------------------------------------------
// Temp handles
// ---------------------------------------------------------------------------

struct TempFileEntry {
    guard: nfs::TempFileGuard,
}

struct TempDirEntry {
    guard: nfs::TempDirGuard,
}

thread_local! {
    static TEMP_FILES: RefCell<HashMap<i64, TempFileEntry>> = RefCell::new(HashMap::new());
    static TEMP_DIRS: RefCell<HashMap<i64, TempDirEntry>> = RefCell::new(HashMap::new());
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
    RuntimeError::at(span, codes::E3532_NFS_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E3530_NFS_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E3530_NFS_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn nfs_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E3531_NFS_ERROR, "nfs_error", msg.into(), span)
}

fn invalid_handle(span: Span, kind: &str, id: i64) -> ValueRef {
    error_value(
        codes::E3533_NFS_INVALID_HANDLE,
        "nfs_error",
        format!("invalid or closed {kind} handle {id}"),
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

fn bytes_from_arg(v: &ValueRef, name: &str, span: Span) -> NiaoResult<Vec<u8>> {
    match &*v.borrow() {
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        Value::ByteArray(b) => Ok(b.clone()),
        other => Err(type_err(
            span,
            format!("{name}() expects string or bytes, got {}", other.type_name()),
        )),
    }
}

fn io_result<T>(span: Span, r: Result<T, std::io::Error>) -> Result<T, ValueRef> {
    r.map_err(|e| nfs_err(span, e.to_string()))
}

fn parse_temp_opts(map: Option<HashMap<String, ValueRef>>, span: Span) -> NiaoResult<nfs::TempOpts> {
    let mut opts = nfs::TempOpts::default();
    let Some(map) = map else {
        return Ok(opts);
    };
    if let Some(v) = map.get("dir") {
        opts.dir = Some(PathBuf::from(match &*v.borrow() {
            Value::String(s) => s.clone(),
            other => {
                return Err(type_err(span, format!("opts.dir must be string, got {}", other.type_name())));
            }
        }));
    }
    if let Some(v) = map.get("prefix") {
        opts.prefix = match &*v.borrow() {
            Value::String(s) => s.clone(),
            other => {
                return Err(type_err(span, format!("opts.prefix must be string, got {}", other.type_name())));
            }
        };
    }
    if let Some(v) = map.get("suffix") {
        opts.suffix = match &*v.borrow() {
            Value::String(s) => s.clone(),
            other => {
                return Err(type_err(span, format!("opts.suffix must be string, got {}", other.type_name())));
            }
        };
    }
    Ok(opts)
}

fn parse_copy_opts(map: Option<HashMap<String, ValueRef>>, span: Span) -> NiaoResult<nfs::CopyOpts> {
    let mut opts = nfs::CopyOpts::default();
    let Some(map) = map else {
        return Ok(opts);
    };
    if let Some(v) = map.get("metadata") {
        opts.metadata = bool_arg(&[v.clone()], 0, "opts.metadata", span)?;
    }
    if let Some(v) = map.get("follow_symlinks") {
        opts.follow_symlinks = bool_arg(&[v.clone()], 0, "opts.follow_symlinks", span)?;
    }
    Ok(opts)
}

fn parse_copytree_opts(
    map: Option<HashMap<String, ValueRef>>,
    span: Span,
) -> NiaoResult<nfs::CopyTreeOpts> {
    let mut opts = nfs::copy_tree_opts_default();
    let Some(map) = map else {
        return Ok(opts);
    };
    if let Some(v) = map.get("dirs_exist_ok") {
        opts.dirs_exist_ok = bool_arg(&[v.clone()], 0, "opts.dirs_exist_ok", span)?;
    }
    if let Some(v) = map.get("symlinks") {
        opts.symlinks = bool_arg(&[v.clone()], 0, "opts.symlinks", span)?;
    }
    if let Some(v) = map.get("metadata") {
        opts.metadata = bool_arg(&[v.clone()], 0, "opts.metadata", span)?;
    }
    if let Some(v) = map.get("threads") {
        opts.threads = int_arg(&[v.clone()], 0, "opts.threads", span)? as usize;
    }
    if let Some(v) = map.get("ignore") {
        match &*v.borrow() {
            Value::Array(items) => {
                let mut patterns = Vec::new();
                for item in items {
                    match &*item.borrow() {
                        Value::String(s) => patterns.push(s.clone()),
                        other => {
                            return Err(type_err(
                                span,
                                format!("opts.ignore items must be strings, got {}", other.type_name()),
                            ));
                        }
                    }
                }
                opts.ignore_patterns = patterns;
            }
            other => {
                return Err(type_err(
                    span,
                    format!("opts.ignore must be an array, got {}", other.type_name()),
                ));
            }
        }
    }
    Ok(opts)
}

fn parse_rmtree_opts(map: Option<HashMap<String, ValueRef>>, span: Span) -> NiaoResult<nfs::RmTreeOpts> {
    let mut opts = nfs::RmTreeOpts::default();
    let Some(map) = map else {
        return Ok(opts);
    };
    if let Some(v) = map.get("ignore_errors") {
        opts.ignore_errors = bool_arg(&[v.clone()], 0, "opts.ignore_errors", span)?;
    }
    if let Some(v) = map.get("ignore") {
        match &*v.borrow() {
            Value::Array(items) => {
                let mut patterns = Vec::new();
                for item in items {
                    match &*item.borrow() {
                        Value::String(s) => patterns.push(s.clone()),
                        other => {
                            return Err(type_err(
                                span,
                                format!("opts.ignore items must be strings, got {}", other.type_name()),
                            ));
                        }
                    }
                }
                opts.ignore_patterns = patterns;
            }
            other => {
                return Err(type_err(
                    span,
                    format!("opts.ignore must be an array, got {}", other.type_name()),
                ));
            }
        }
    }
    Ok(opts)
}

fn parse_walk_opts(map: Option<HashMap<String, ValueRef>>, span: Span) -> NiaoResult<nfs::WalkOpts> {
    let mut opts = nfs::WalkOpts::default();
    let Some(map) = map else {
        return Ok(opts);
    };
    if let Some(v) = map.get("topdown") {
        opts.topdown = bool_arg(&[v.clone()], 0, "opts.topdown", span)?;
    }
    if let Some(v) = map.get("follow_symlinks") {
        opts.follow_symlinks = bool_arg(&[v.clone()], 0, "opts.follow_symlinks", span)?;
    }
    Ok(opts)
}

fn parse_atomic_opts(
    map: Option<HashMap<String, ValueRef>>,
    span: Span,
) -> NiaoResult<nfs::AtomicWriteOpts> {
    let mut opts = nfs::AtomicWriteOpts::default();
    let Some(map) = map else {
        return Ok(opts);
    };
    if let Some(v) = map.get("dir") {
        opts.dir = Some(PathBuf::from(match &*v.borrow() {
            Value::String(s) => s.clone(),
            other => {
                return Err(type_err(span, format!("opts.dir must be string, got {}", other.type_name())));
            }
        }));
    }
    if let Some(v) = map.get("fsync") {
        opts.fsync = bool_arg(&[v.clone()], 0, "opts.fsync", span)?;
    }
    #[cfg(unix)]
    if let Some(v) = map.get("mode") {
        opts.mode = Some(int_arg(&[v.clone()], 0, "opts.mode", span)? as u32);
    }
    Ok(opts)
}

fn disk_usage_object(u: nfs::DiskUsage) -> Value {
    let mut map = HashMap::new();
    map.insert("total".to_string(), Value::Int(u.total as i64).ref_cell());
    map.insert("used".to_string(), Value::Int(u.used as i64).ref_cell());
    map.insert("free".to_string(), Value::Int(u.free as i64).ref_cell());
    Value::Object(map)
}

// ---------------------------------------------------------------------------
// Copy / move / tree
// ---------------------------------------------------------------------------

// >>> nfs.copy("a.txt", "b.txt")
// => nil
fn nfs_copy(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nfs_copy", span)?;
    let src = path_arg(args, 0, "copy", span)?;
    let dst = path_arg(args, 1, "copy", span)?;
    let opts = parse_copy_opts(optional_object(args, 2), span)?;
    match io_result(span, nfs::copy_file(&src, &dst, &opts)) {
        Ok(n) => Ok(ok_int(n as i64)),
        Err(e) => Ok(e),
    }
}

// >>> nfs.copy2("a.txt", "b.txt")
// => 42
fn nfs_copy2(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nfs_copy2", span)?;
    let src = path_arg(args, 0, "copy2", span)?;
    let dst = path_arg(args, 1, "copy2", span)?;
    match io_result(span, nfs::copy2(&src, &dst)) {
        Ok(n) => Ok(ok_int(n as i64)),
        Err(e) => Ok(e),
    }
}

// >>> nfs.copyfile("a.txt", "b.txt")
// => 42
fn nfs_copyfile(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nfs_copyfile", span)?;
    let src = path_arg(args, 0, "copyfile", span)?;
    let dst = path_arg(args, 1, "copyfile", span)?;
    match io_result(span, nfs::copyfile(&src, &dst)) {
        Ok(n) => Ok(ok_int(n as i64)),
        Err(e) => Ok(e),
    }
}

// >>> nfs.copymode("a.txt", "b.txt")
// => nil
fn nfs_copymode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nfs_copymode", span)?;
    let src = path_arg(args, 0, "copymode", span)?;
    let dst = path_arg(args, 1, "copymode", span)?;
    match io_result(span, nfs::copy_mode(&src, &dst)) {
        Ok(()) => Ok(ok_nil()),
        Err(e) => Ok(e),
    }
}

// >>> nfs.copystat("a.txt", "b.txt")
// => nil
fn nfs_copystat(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nfs_copystat", span)?;
    let src = path_arg(args, 0, "copystat", span)?;
    let dst = path_arg(args, 1, "copystat", span)?;
    match io_result(span, nfs::copy_stat(&src, &dst)) {
        Ok(()) => Ok(ok_nil()),
        Err(e) => Ok(e),
    }
}

// >>> nfs.copytree("src", "dst")
// => nil
fn nfs_copytree(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nfs_copytree", span)?;
    let src = path_arg(args, 0, "copytree", span)?;
    let dst = path_arg(args, 1, "copytree", span)?;
    let opts = parse_copytree_opts(optional_object(args, 2), span)?;
    match io_result(span, nfs::copy_tree(&src, &dst, &opts)) {
        Ok(()) => Ok(ok_nil()),
        Err(e) => Ok(e),
    }
}

// >>> nfs.move("old.txt", "new.txt")
// => nil
fn nfs_move(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nfs_move", span)?;
    let src = path_arg(args, 0, "move", span)?;
    let dst = path_arg(args, 1, "move", span)?;
    match io_result(span, nfs::move_path(&src, &dst)) {
        Ok(()) => Ok(ok_nil()),
        Err(e) => Ok(e),
    }
}

// >>> nfs.rmtree("tmpdir")
// => nil
fn nfs_rmtree(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nfs_rmtree", span)?;
    let path = path_arg(args, 0, "rmtree", span)?;
    let opts = parse_rmtree_opts(optional_object(args, 1), span)?;
    match io_result(span, nfs::rmtree(&path, &opts)) {
        Ok(()) => Ok(ok_nil()),
        Err(e) => Ok(e),
    }
}

// >>> nfs.disk_usage(".")
// => {total: 1, used: 2, free: 3}
fn nfs_disk_usage(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfs_disk_usage", span)?;
    let path = path_arg(args, 0, "disk_usage", span)?;
    match io_result(span, nfs::disk_usage(&path)) {
        Ok(u) => Ok(disk_usage_object(u).ref_cell()),
        Err(e) => Ok(e),
    }
}

// >>> nfs.tree_size(".")
// => 1024
fn nfs_tree_size(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nfs_tree_size", span)?;
    let path = path_arg(args, 0, "tree_size", span)?;
    let threads = optional_object(args, 1)
        .and_then(|m| m.get("threads").cloned())
        .map(|v| int_arg(&[v], 0, "tree_size", span))
        .transpose()?
        .unwrap_or(niao_parallel::available_threads() as i64) as usize;
    match io_result(span, nfs::tree_size(&path, threads)) {
        Ok(n) => Ok(ok_int(n as i64)),
        Err(e) => Ok(e),
    }
}

// >>> nfs.samefile("a", "b")
// => false
fn nfs_samefile(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nfs_samefile", span)?;
    let a = path_arg(args, 0, "samefile", span)?;
    let b = path_arg(args, 1, "samefile", span)?;
    match io_result(span, nfs::samefile(&a, &b)) {
        Ok(v) => Ok(ok_bool(v)),
        Err(e) => Ok(e),
    }
}

// >>> nfs.which("niao")
// => "/usr/bin/niao"
fn nfs_which(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfs_which", span)?;
    let cmd = string_arg(args, 0, "which", span)?;
    match nfs::which(&cmd) {
        Some(p) => Ok(ok_string(p.to_string_lossy())),
        None => Ok(Value::Nil.ref_cell()),
    }
}

// >>> nfs.walk(".")
// => [{root: ".", dirs: [], files: ["a.txt"]}]
fn nfs_walk(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nfs_walk", span)?;
    let root = path_arg(args, 0, "walk", span)?;
    let opts = parse_walk_opts(optional_object(args, 1), span)?;
    match io_result(span, nfs::walk(&root, &opts)) {
        Ok(entries) => {
            let items: Vec<ValueRef> = entries
                .into_iter()
                .map(|e| {
                    let mut map = HashMap::new();
                    map.insert(
                        "root".to_string(),
                        ok_string(e.root.to_string_lossy()),
                    );
                    let dirs: Vec<ValueRef> = e.dirs.into_iter().map(ok_string).collect();
                    let files: Vec<ValueRef> = e.files.into_iter().map(ok_string).collect();
                    map.insert("dirs".to_string(), Value::Array(dirs).ref_cell());
                    map.insert("files".to_string(), Value::Array(files).ref_cell());
                    Value::Object(map).ref_cell()
                })
                .collect();
            Ok(Value::Array(items).ref_cell())
        }
        Err(e) => Ok(e),
    }
}

// >>> nfs.commonprefix(["/a/b", "/a/c"])
// => "/a"
fn nfs_commonprefix(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfs_commonprefix", span)?;
    match &*args[0].borrow() {
        Value::Array(items) => {
            let mut paths = Vec::with_capacity(items.len());
            for item in items {
                match &*item.borrow() {
                    Value::String(s) => paths.push(PathBuf::from(s)),
                    other => {
                        return Err(type_err(
                            span,
                            format!("commonprefix() paths must be strings, got {}", other.type_name()),
                        ));
                    }
                }
            }
            Ok(ok_string(nfs::common_prefix(&paths).to_string_lossy()))
        }
        other => Err(type_err(
            span,
            format!("commonprefix() expects an array, got {}", other.type_name()),
        )),
    }
}

// ---------------------------------------------------------------------------
// Temp files / dirs
// ---------------------------------------------------------------------------

// >>> nfs.temp_dir()
// => "/tmp"
fn nfs_temp_dir(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nfs_temp_dir", span)?;
    Ok(ok_string(nfs::temp_dir_path().to_string_lossy()))
}

// >>> nfs.mkstemp()
// => {handle: 1, path: "/tmp/.tmp..."}
fn nfs_mkstemp(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nfs_mkstemp", span)?;
    let opts = parse_temp_opts(optional_object(args, 0), span)?;
    match io_result(span, nfs::mkstemp(&opts)) {
        Ok((_file, path)) => {
            let guard = nfs::TempFileGuard::adopt(path.clone(), true);
            let id = alloc_handle();
            TEMP_FILES.with(|t| {
                t.borrow_mut().insert(id, TempFileEntry { guard });
            });
            let mut map = HashMap::new();
            map.insert("handle".to_string(), ok_int(id));
            map.insert("path".to_string(), ok_string(path.to_string_lossy()));
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(e),
    }
}

// >>> nfs.mktemp()
// => "/tmp/.tmp..."
fn nfs_mktemp(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nfs_mktemp", span)?;
    let opts = parse_temp_opts(optional_object(args, 0), span)?;
    match io_result(span, nfs::mktemp(&opts)) {
        Ok(p) => Ok(ok_string(p.to_string_lossy())),
        Err(e) => Ok(e),
    }
}

// >>> nfs.tempfile()
// => 1
fn nfs_tempfile(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nfs_tempfile", span)?;
    let opts = parse_temp_opts(optional_object(args, 0), span)?;
    match io_result(span, nfs::TempFileGuard::new(&opts)) {
        Ok(guard) => {
            let id = alloc_handle();
            TEMP_FILES.with(|t| {
                t.borrow_mut().insert(id, TempFileEntry { guard });
            });
            Ok(ok_int(id))
        }
        Err(e) => Ok(e),
    }
}

// >>> nfs.tempdir()
// => 1
fn nfs_tempdir(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nfs_tempdir", span)?;
    let opts = parse_temp_opts(optional_object(args, 0), span)?;
    match io_result(span, nfs::TempDirGuard::new(&opts)) {
        Ok(guard) => {
            let id = alloc_handle();
            TEMP_DIRS.with(|t| {
                t.borrow_mut().insert(id, TempDirEntry { guard });
            });
            Ok(ok_int(id))
        }
        Err(e) => Ok(e),
    }
}

// >>> nfs.tempfile_path(handle)
// => "/tmp/..."
fn nfs_tempfile_path(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfs_tempfile_path", span)?;
    let id = int_arg(args, 0, "tempfile_path", span)?;
    TEMP_FILES.with(|t| {
        let t = t.borrow();
        match t.get(&id) {
            Some(entry) => Ok(ok_string(entry.guard.path().to_string_lossy())),
            None => Ok(invalid_handle(span, "tempfile", id)),
        }
    })
}

// >>> nfs.tempfile_write(handle, "data")
// => 4
fn nfs_tempfile_write(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nfs_tempfile_write", span)?;
    let id = int_arg(args, 0, "tempfile_write", span)?;
    let data = bytes_from_arg(&args[1], "tempfile_write", span)?;
    TEMP_FILES.with(|t| {
        let mut t = t.borrow_mut();
        match t.get_mut(&id) {
            Some(entry) => match entry.guard.write(&data) {
                Ok(n) => Ok(ok_int(n as i64)),
                Err(e) => Ok(nfs_err(span, e.to_string())),
            },
            None => Ok(invalid_handle(span, "tempfile", id)),
        }
    })
}

// >>> nfs.tempfile_read(handle)
// => "data"
fn nfs_tempfile_read(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nfs_tempfile_read", span)?;
    let id = int_arg(args, 0, "tempfile_read", span)?;
    let max = if args.len() == 2 {
        int_arg(args, 1, "tempfile_read", span)? as usize
    } else {
        64 * 1024
    };
    TEMP_FILES.with(|t| {
        let mut t = t.borrow_mut();
        match t.get_mut(&id) {
            Some(entry) => match entry.guard.read(max) {
                Ok(bytes) => Ok(Value::ByteArray(bytes).ref_cell()),
                Err(e) => Ok(nfs_err(span, e.to_string())),
            },
            None => Ok(invalid_handle(span, "tempfile", id)),
        }
    })
}

// >>> nfs.tempfile_close(handle, {keep: true})
// => "/tmp/..."
fn nfs_tempfile_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nfs_tempfile_close", span)?;
    let id = int_arg(args, 0, "tempfile_close", span)?;
    let keep = optional_object(args, 1)
        .and_then(|m| m.get("keep").cloned())
        .map(|v| bool_arg(&[v], 0, "tempfile_close", span))
        .transpose()?
        .unwrap_or(false);
    TEMP_FILES.with(|t| {
        let mut t = t.borrow_mut();
        match t.remove(&id) {
            Some(mut entry) => {
                entry.guard.keep = keep;
                match entry.guard.close() {
                    Ok(p) => Ok(ok_string(p.to_string_lossy())),
                    Err(e) => Ok(nfs_err(span, e.to_string())),
                }
            }
            None => Ok(invalid_handle(span, "tempfile", id)),
        }
    })
}

// >>> nfs.tempdir_path(handle)
// => "/tmp/..."
fn nfs_tempdir_path(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfs_tempdir_path", span)?;
    let id = int_arg(args, 0, "tempdir_path", span)?;
    TEMP_DIRS.with(|t| {
        let t = t.borrow();
        match t.get(&id) {
            Some(entry) => Ok(ok_string(entry.guard.path().to_string_lossy())),
            None => Ok(invalid_handle(span, "tempdir", id)),
        }
    })
}

// >>> nfs.tempdir_close(handle, {keep: true})
// => "/tmp/..."
fn nfs_tempdir_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nfs_tempdir_close", span)?;
    let id = int_arg(args, 0, "tempdir_close", span)?;
    let keep = optional_object(args, 1)
        .and_then(|m| m.get("keep").cloned())
        .map(|v| bool_arg(&[v], 0, "tempdir_close", span))
        .transpose()?
        .unwrap_or(false);
    TEMP_DIRS.with(|t| {
        let mut t = t.borrow_mut();
        match t.remove(&id) {
            Some(mut entry) => {
                entry.guard.keep = keep;
                match entry.guard.close() {
                    Ok(p) => Ok(ok_string(p.to_string_lossy())),
                    Err(e) => Ok(nfs_err(span, e.to_string())),
                }
            }
            None => Ok(invalid_handle(span, "tempdir", id)),
        }
    })
}

// ---------------------------------------------------------------------------
// Atomic write & trash
// ---------------------------------------------------------------------------

// >>> nfs.write_atomic("out.txt", "hello")
// => nil
fn nfs_write_atomic(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nfs_write_atomic", span)?;
    let path = path_arg(args, 0, "write_atomic", span)?;
    let text = string_arg(args, 1, "write_atomic", span)?;
    let opts = parse_atomic_opts(optional_object(args, 2), span)?;
    match io_result(span, nfs::write_atomic(&path, &text, &opts)) {
        Ok(()) => Ok(ok_nil()),
        Err(e) => Ok(e),
    }
}

// >>> nfs.write_bytes_atomic("out.bin", [1, 2, 3])
// => nil
fn nfs_write_bytes_atomic(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nfs_write_bytes_atomic", span)?;
    let path = path_arg(args, 0, "write_bytes_atomic", span)?;
    let data = bytes_from_arg(&args[1], "write_bytes_atomic", span)?;
    let opts = parse_atomic_opts(optional_object(args, 2), span)?;
    match io_result(span, nfs::write_bytes_atomic(&path, &data, &opts)) {
        Ok(()) => Ok(ok_nil()),
        Err(e) => Ok(e),
    }
}

// >>> nfs.trash("old.txt")
// => nil
fn nfs_trash(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfs_trash", span)?;
    let path = path_arg(args, 0, "trash", span)?;
    match io_result(span, nfs::trash_path(&path)) {
        Ok(()) => Ok(ok_nil()),
        Err(e) => Ok(e),
    }
}

// >>> nfs.trash_all(["a.txt", "b.txt"])
// => nil
fn nfs_trash_all(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfs_trash_all", span)?;
    match &*args[0].borrow() {
        Value::Array(items) => {
            let mut paths: Vec<PathBuf> = Vec::with_capacity(items.len());
            for item in items {
                match &*item.borrow() {
                    Value::String(s) => paths.push(PathBuf::from(s)),
                    other => {
                        return Err(type_err(
                            span,
                            format!("trash_all() paths must be strings, got {}", other.type_name()),
                        ));
                    }
                }
            }
            let refs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();
            match io_result(span, nfs::trash_all(&refs)) {
                Ok(()) => Ok(ok_nil()),
                Err(e) => Ok(e),
            }
        }
        other => Err(type_err(
            span,
            format!("trash_all() expects an array, got {}", other.type_name()),
        )),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nfs_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nfs_fns![
    ("nfs_copy", "copy", nfs_copy),
    ("nfs_copy2", "copy2", nfs_copy2),
    ("nfs_copyfile", "copyfile", nfs_copyfile),
    ("nfs_copymode", "copymode", nfs_copymode),
    ("nfs_copystat", "copystat", nfs_copystat),
    ("nfs_copytree", "copytree", nfs_copytree),
    ("nfs_move", "move", nfs_move),
    ("nfs_rmtree", "rmtree", nfs_rmtree),
    ("nfs_disk_usage", "disk_usage", nfs_disk_usage),
    ("nfs_tree_size", "tree_size", nfs_tree_size),
    ("nfs_samefile", "samefile", nfs_samefile),
    ("nfs_which", "which", nfs_which),
    ("nfs_walk", "walk", nfs_walk),
    ("nfs_commonprefix", "commonprefix", nfs_commonprefix),
    ("nfs_temp_dir", "temp_dir", nfs_temp_dir),
    ("nfs_mkstemp", "mkstemp", nfs_mkstemp),
    ("nfs_mktemp", "mktemp", nfs_mktemp),
    ("nfs_tempfile", "tempfile", nfs_tempfile),
    ("nfs_tempdir", "tempdir", nfs_tempdir),
    ("nfs_tempfile_path", "tempfile_path", nfs_tempfile_path),
    ("nfs_tempfile_write", "tempfile_write", nfs_tempfile_write),
    ("nfs_tempfile_read", "tempfile_read", nfs_tempfile_read),
    ("nfs_tempfile_close", "tempfile_close", nfs_tempfile_close),
    ("nfs_tempdir_path", "tempdir_path", nfs_tempdir_path),
    ("nfs_tempdir_close", "tempdir_close", nfs_tempdir_close),
    ("nfs_write_atomic", "write_atomic", nfs_write_atomic),
    ("nfs_write_bytes_atomic", "write_bytes_atomic", nfs_write_bytes_atomic),
    ("nfs_trash", "trash", nfs_trash),
    ("nfs_trash_all", "trash_all", nfs_trash_all),
];

pub const MODULE_NAME: &str = "nfs";
pub const MODULE_PATHS: &[&str] = &["nfs", "std/nfs"];

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
    fn temp_dir_nonempty() {
        let out = nfs_temp_dir(&[], span()).unwrap();
        match &*out.borrow() {
            Value::String(p) => assert!(!p.is_empty()),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn which_finds_shell() {
        let cmd = if cfg!(windows) { "cmd" } else { "sh" };
        let out = nfs_which(&[s(cmd)], span()).unwrap();
        assert!(matches!(&*out.borrow(), Value::String(_)));
    }
}
