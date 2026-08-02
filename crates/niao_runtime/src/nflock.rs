//! Native nflock standard library — advisory file locks, lockfiles, PID files,
//! and timeouts (~Python `filelock` + `fcntl` subset).
//!
//! Import with `import "nflock"` (or `import "std/nflock"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_flock as flock;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Handle tables
// ---------------------------------------------------------------------------

struct LockEntry {
    handle: flock::LockHandle,
}

struct PidEntry {
    pid_file: flock::PidFile,
}

thread_local! {
    static LOCKS: RefCell<HashMap<i64, LockEntry>> = RefCell::new(HashMap::new());
    static PIDS: RefCell<HashMap<i64, PidEntry>> = RefCell::new(HashMap::new());
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
    RuntimeError::at(span, codes::E3523_NFLOCK_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E3521_NFLOCK_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E3521_NFLOCK_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn nflock_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E3522_NFLOCK_ERROR, "nflock_error", msg.into(), span)
}

fn nflock_timeout(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E3525_NFLOCK_TIMEOUT, "nflock_error", msg.into(), span)
}

fn invalid_handle(span: Span, kind: &str, id: i64) -> ValueRef {
    error_value(
        codes::E3524_NFLOCK_INVALID_HANDLE,
        "nflock_error",
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

fn optional_bool(obj: &HashMap<String, ValueRef>, key: &str, default: bool) -> bool {
    obj.get(key)
        .and_then(|v| match &*v.borrow() {
            Value::Bool(b) => Some(*b),
            _ => None,
        })
        .unwrap_or(default)
}

fn optional_int_ms(obj: &HashMap<String, ValueRef>, key: &str) -> Option<Duration> {
    obj.get(key).and_then(|v| match &*v.borrow() {
        Value::Int(n) if *n < 0 => None,
        Value::Int(n) if *n == 0 => Some(Duration::from_millis(0)),
        Value::Int(n) => Some(Duration::from_millis(*n as u64)),
        _ => None,
    })
}

fn parse_lock_opts(opts: &ValueRef, span: Span) -> NiaoResult<flock::LockOptions> {
    let mut out = flock::LockOptions::default();
    match &*opts.borrow() {
        Value::Nil => {}
        Value::Object(map) => {
            out.create = optional_bool(map, "create", true);
            if let Some(v) = map.get("mode") {
                match &*v.borrow() {
                    Value::String(s) => {
                        out.mode = flock::LockMode::from_str(s).ok_or_else(|| {
                            type_err(span, format!("opts.mode: unknown lock mode {s}"))
                        })?;
                    }
                    Value::Bool(true) => out.mode = flock::LockMode::Shared,
                    Value::Bool(false) => out.mode = flock::LockMode::Exclusive,
                    other => {
                        return Err(type_err(
                            span,
                            format!("opts.mode must be string or bool, got {}", other.type_name()),
                        ));
                    }
                }
            }
            if let Some(v) = map.get("shared") {
                if let Value::Bool(b) = &*v.borrow() {
                    out.mode = if *b {
                        flock::LockMode::Shared
                    } else {
                        flock::LockMode::Exclusive
                    };
                }
            }
            if let Some(v) = map.get("exclusive") {
                if let Value::Bool(b) = &*v.borrow() {
                    if *b {
                        out.mode = flock::LockMode::Exclusive;
                    }
                }
            }
            out.timeout = optional_int_ms(map, "timeout_ms")
                .or_else(|| optional_int_ms(map, "timeout"));
            if let Some(v) = map.get("poll_ms") {
                if let Value::Int(n) = &*v.borrow() {
                    if *n >= 0 {
                        out.poll_interval = Duration::from_millis(*n as u64);
                    }
                }
            }
            out.use_flock = optional_bool(map, "use_flock", true);
            if let Some(v) = map.get("content") {
                match &*v.borrow() {
                    Value::String(s) => out.content = Some(s.clone()),
                    other => {
                        return Err(type_err(
                            span,
                            format!("opts.content must be string, got {}", other.type_name()),
                        ));
                    }
                }
            }
        }
        other => {
            return Err(type_err(
                span,
                format!("opts must be object, got {}", other.type_name()),
            ));
        }
    }
    Ok(out)
}

fn parse_pid_opts(opts: &ValueRef, span: Span) -> NiaoResult<flock::PidOptions> {
    let mut out = flock::PidOptions::default();
    match &*opts.borrow() {
        Value::Nil => {}
        Value::Object(map) => {
            out.timeout = optional_int_ms(map, "timeout_ms")
                .or_else(|| optional_int_ms(map, "timeout"));
            if let Some(v) = map.get("poll_ms") {
                if let Value::Int(n) = &*v.borrow() {
                    if *n >= 0 {
                        out.poll_interval = Duration::from_millis(*n as u64);
                    }
                }
            }
            out.force = optional_bool(map, "force", false);
            out.write_pid = optional_bool(map, "write_pid", true);
        }
        other => {
            return Err(type_err(
                span,
                format!("opts must be object, got {}", other.type_name()),
            ));
        }
    }
    Ok(out)
}

fn flock_result_to_value<T>(span: Span, r: flock::FlockResult<T>, ok: impl FnOnce(T) -> ValueRef) -> ValueRef {
    match r {
        Ok(v) => ok(v),
        Err(flock::FlockError::Io(e)) if e.kind() == std::io::ErrorKind::TimedOut => {
            nflock_timeout(span, e.to_string())
        }
        Err(flock::FlockError::Timeout { path, timeout }) => {
            nflock_timeout(span, format!("timed out acquiring lock on {path} after {timeout:?}"))
        }
        Err(e) => nflock_err(span, e.to_string()),
    }
}

fn lock_object(id: i64, path: &str, locked: bool, mode: Option<flock::LockMode>) -> ValueRef {
    let mut map = HashMap::new();
    map.insert("handle".to_string(), Value::Int(id).ref_cell());
    map.insert("path".to_string(), Value::String(path.to_string()).ref_cell());
    map.insert("locked".to_string(), Value::Bool(locked).ref_cell());
    if let Some(m) = mode {
        let name = match m {
            flock::LockMode::Shared => "shared",
            flock::LockMode::Exclusive => "exclusive",
        };
        map.insert("mode".to_string(), Value::String(name.into()).ref_cell());
    }
    Value::Object(map).ref_cell()
}

fn pid_object(id: i64, path: &str, pid: u32) -> ValueRef {
    let mut map = HashMap::new();
    map.insert("handle".to_string(), Value::Int(id).ref_cell());
    map.insert("path".to_string(), Value::String(path.to_string()).ref_cell());
    map.insert("pid".to_string(), Value::Int(pid as i64).ref_cell());
    map.insert("kind".to_string(), Value::String("pid".into()).ref_cell());
    Value::Object(map).ref_cell()
}

fn handle_from_arg(args: &[ValueRef], idx: usize, span: Span) -> NiaoResult<i64> {
    if let Value::Int(n) = &*args[idx].borrow() {
        return Ok(*n);
    }
    if let Value::Object(map) = &*args[idx].borrow() {
        if let Some(h) = map.get("handle") {
            if let Value::Int(n) = &*h.borrow() {
                return Ok(*n);
            }
        }
    }
    Err(type_err(
        span,
        format!(
            "argument {} must be lock handle int or object with handle field",
            idx + 1
        ),
    ))
}

fn with_lock<F>(id: i64, span: Span, f: F) -> ValueRef
where
    F: FnOnce(&mut flock::LockHandle) -> ValueRef,
{
    LOCKS.with(|locks| {
        let mut locks = locks.borrow_mut();
        match locks.get_mut(&id) {
            Some(entry) => f(&mut entry.handle),
            None => invalid_handle(span, "lock", id),
        }
    })
}

fn map_flock_err(span: Span, e: flock::FlockError) -> ValueRef {
    match e {
        flock::FlockError::Io(err) if err.kind() == std::io::ErrorKind::TimedOut => {
            nflock_timeout(span, err.to_string())
        }
        flock::FlockError::Timeout { path, timeout } => {
            nflock_timeout(span, format!("timed out acquiring lock on {path} after {timeout:?}"))
        }
        other => nflock_err(span, other.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Public Niao builtins
// ---------------------------------------------------------------------------

/// nflock_open(path, opts?) → {handle, path, locked}
///
/// >>> import "nflock"
/// >>> let h = nflock.open("/tmp/test.lock")
/// >>> type(h) == "object"
/// => true
fn nflock_open(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nflock_open", span)?;
    let path = string_arg(args, 0, "nflock_open", span)?;
    let opts = if args.len() == 2 {
        parse_lock_opts(&args[1], span)?
    } else {
        flock::LockOptions::default()
    };
    match flock::LockHandle::open(&path, &opts) {
        Ok(handle) => {
            let id = alloc_handle();
            let path_s = handle.path().display().to_string();
            LOCKS.with(|locks| {
                locks.borrow_mut().insert(id, LockEntry { handle });
            });
            Ok(lock_object(id, &path_s, false, None))
        }
        Err(e) => Ok(map_flock_err(span, e)),
    }
}

/// nflock_file(path, opts?) — alias for `open` (~filelock.FileLock).
fn nflock_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    nflock_open(args, span)
}

/// nflock_lock(path, opts?) → acquired lock handle
///
/// >>> let lk = nflock.lock("/tmp/nflock_test.lock")
/// >>> lk.locked
/// => true
fn nflock_lock(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nflock_lock", span)?;
    let path = string_arg(args, 0, "nflock_lock", span)?;
    let opts = if args.len() == 2 {
        parse_lock_opts(&args[1], span)?
    } else {
        flock::LockOptions::default()
    };
    Ok(flock_result_to_value(span, flock::lock(&path, &opts), |mut handle| {
        let id = alloc_handle();
        let path_s = handle.path().display().to_string();
        let mode = handle.mode();
        LOCKS.with(|locks| {
            locks.borrow_mut().insert(id, LockEntry { handle });
        });
        lock_object(id, &path_s, true, mode)
    }))
}

/// nflock_acquire(handle, opts?) → nil
///
/// >>> nflock.acquire(h, {timeout_ms: 1000})
/// => nil
fn nflock_acquire(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nflock_acquire", span)?;
    let id = handle_from_arg(args, 0, span)?;
    let opts = if args.len() == 2 {
        parse_lock_opts(&args[1], span)?
    } else {
        flock::LockOptions::default()
    };
    Ok(with_lock(id, span, |handle| {
        flock_result_to_value(span, handle.acquire(&opts), |_| Value::Nil.ref_cell())
    }))
}

/// nflock_try_acquire(handle, opts?) → bool
///
/// >>> nflock.try_acquire(h)
/// => true
fn nflock_try_acquire(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nflock_try_acquire", span)?;
    let id = handle_from_arg(args, 0, span)?;
    let mode = if args.len() == 2 {
        parse_lock_opts(&args[1], span)?.mode
    } else {
        flock::LockMode::Exclusive
    };
    Ok(with_lock(id, span, |handle| {
        flock_result_to_value(span, handle.try_acquire(mode), |ok| Value::Bool(ok).ref_cell())
    }))
}

/// nflock_release(handle) → nil
fn nflock_release(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nflock_release", span)?;
    let id = handle_from_arg(args, 0, span)?;
    Ok(with_lock(id, span, |handle| {
        flock_result_to_value(span, handle.release(), |_| Value::Nil.ref_cell())
    }))
}

/// nflock_locked(handle) → bool
fn nflock_locked(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nflock_locked", span)?;
    let id = handle_from_arg(args, 0, span)?;
    Ok(with_lock(id, span, |handle| Value::Bool(handle.is_locked()).ref_cell()))
}

/// nflock_close(handle) → nil
fn nflock_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nflock_close", span)?;
    let id = handle_from_arg(args, 0, span)?;
    let removed = LOCKS.with(|locks| locks.borrow_mut().remove(&id).is_some());
    if removed {
        Ok(Value::Nil.ref_cell())
    } else {
        Ok(invalid_handle(span, "lock", id))
    }
}

/// nflock_path(handle) → string
fn nflock_path(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nflock_path", span)?;
    let id = handle_from_arg(args, 0, span)?;
    Ok(with_lock(id, span, |handle| {
        Value::String(handle.path().display().to_string()).ref_cell()
    }))
}

/// nflock_info(handle) → {handle, path, locked, mode?}
fn nflock_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nflock_info", span)?;
    let id = handle_from_arg(args, 0, span)?;
    Ok(with_lock(id, span, |handle| {
        lock_object(id, &handle.path().display().to_string(), handle.is_locked(), handle.mode())
    }))
}

/// nflock_flock(handle, op) → nil — BSD flock / Windows LockFileEx.
fn nflock_flock(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nflock_flock", span)?;
    let id = handle_from_arg(args, 0, span)?;
    let op = int_arg(args, 1, "nflock_flock", span)? as i32;
    Ok(with_lock(id, span, |handle| {
        flock_result_to_value(span, flock::flock(handle.file(), op), |_| Value::Nil.ref_cell())
    }))
}

/// nflock_lockf(handle, cmd, len?, start?) → nil — POSIX record lock.
fn nflock_lockf(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 4, "nflock_lockf", span)?;
    let id = handle_from_arg(args, 0, span)?;
    let cmd = int_arg(args, 1, "nflock_lockf", span)? as i32;
    let len = if args.len() >= 3 {
        int_arg(args, 2, "nflock_lockf", span)?
    } else {
        0
    };
    let start = if args.len() == 4 {
        int_arg(args, 3, "nflock_lockf", span)?
    } else {
        0
    };
    Ok(with_lock(id, span, |handle| {
        flock_result_to_value(
            span,
            flock::lockf(handle.file(), cmd, len, start),
            |_| Value::Nil.ref_cell(),
        )
    }))
}

/// nflock_break_stale(path, force?) → bool
fn nflock_break_stale(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nflock_break_stale", span)?;
    let path = string_arg(args, 0, "nflock_break_stale", span)?;
    let force = if args.len() == 2 {
        bool_arg(args, 1, "nflock_break_stale", span)?
    } else {
        false
    };
    Ok(flock_result_to_value(
        span,
        flock::break_stale(&path, force),
        |b| Value::Bool(b).ref_cell(),
    ))
}

/// nflock_pid_acquire(path, opts?) → {handle, path, pid, kind}
///
/// >>> let pf = nflock.pid_acquire("/tmp/app.pid")
/// >>> pf.pid > 0
/// => true
fn nflock_pid_acquire(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nflock_pid_acquire", span)?;
    let path = string_arg(args, 0, "nflock_pid_acquire", span)?;
    let opts = if args.len() == 2 {
        parse_pid_opts(&args[1], span)?
    } else {
        flock::PidOptions::default()
    };
    Ok(flock_result_to_value(span, flock::PidFile::acquire(&path, &opts), |pf| {
        let id = alloc_handle();
        let path_s = pf.path().display().to_string();
        let pid = pf.pid;
        PIDS.with(|pids| {
            pids.borrow_mut().insert(id, PidEntry { pid_file: pf });
        });
        pid_object(id, &path_s, pid)
    }))
}

/// nflock_pid_read(path) → int
fn nflock_pid_read(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nflock_pid_read", span)?;
    let path = string_arg(args, 0, "nflock_pid_read", span)?;
    Ok(flock_result_to_value(span, flock::read_pid(&path), |pid| {
        Value::Int(pid as i64).ref_cell()
    }))
}

/// nflock_pid_write(path, pid?) → nil
fn nflock_pid_write(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nflock_pid_write", span)?;
    let path = string_arg(args, 0, "nflock_pid_write", span)?;
    let pid = if args.len() == 2 {
        Some(int_arg(args, 1, "nflock_pid_write", span)? as u32)
    } else {
        None
    };
    Ok(flock_result_to_value(
        span,
        flock::write_pid(&path, pid),
        |_| Value::Nil.ref_cell(),
    ))
}

/// nflock_pid_alive(pid) → bool
///
/// >>> nflock.pid_alive(nos.getpid())
/// => true
fn nflock_pid_alive(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nflock_pid_alive", span)?;
    let pid = int_arg(args, 0, "nflock_pid_alive", span)? as u32;
    Ok(Value::Bool(flock::pid_alive(pid)).ref_cell())
}

/// nflock_pid_remove(path) → nil
fn nflock_pid_remove(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nflock_pid_remove", span)?;
    let path = string_arg(args, 0, "nflock_pid_remove", span)?;
    Ok(flock_result_to_value(
        span,
        flock::remove_pid(&path),
        |_| Value::Nil.ref_cell(),
    ))
}

/// nflock_pid_release(handle) → nil
fn nflock_pid_release(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nflock_pid_release", span)?;
    let id = handle_from_arg(args, 0, span)?;
    let entry = PIDS.with(|pids| pids.borrow_mut().remove(&id));
    match entry {
        Some(e) => Ok(flock_result_to_value(span, e.pid_file.release(), |_| Value::Nil.ref_cell())),
        None => Ok(invalid_handle(span, "pid", id)),
    }
}

// ---------------------------------------------------------------------------
// Module exports
// ---------------------------------------------------------------------------

macro_rules! nflock_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nflock_fns![
    ("nflock_open", "open", nflock_open),
    ("nflock_file", "file", nflock_file),
    ("nflock_lock", "lock", nflock_lock),
    ("nflock_acquire", "acquire", nflock_acquire),
    ("nflock_try_acquire", "try_acquire", nflock_try_acquire),
    ("nflock_release", "release", nflock_release),
    ("nflock_locked", "locked", nflock_locked),
    ("nflock_close", "close", nflock_close),
    ("nflock_path", "path", nflock_path),
    ("nflock_info", "info", nflock_info),
    ("nflock_flock", "flock", nflock_flock),
    ("nflock_lockf", "lockf", nflock_lockf),
    ("nflock_break_stale", "break_stale", nflock_break_stale),
    ("nflock_pid_acquire", "pid_acquire", nflock_pid_acquire),
    ("nflock_pid_read", "pid_read", nflock_pid_read),
    ("nflock_pid_write", "pid_write", nflock_pid_write),
    ("nflock_pid_alive", "pid_alive", nflock_pid_alive),
    ("nflock_pid_remove", "pid_remove", nflock_pid_remove),
    ("nflock_pid_release", "pid_release", nflock_pid_release),
];

pub const MODULE_NAME: &str = "nflock";
pub const MODULE_PATHS: &[&str] = &["nflock", "std/nflock"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    map.insert("LOCK_SH".to_string(), Value::Int(flock::LOCK_SH as i64).ref_cell());
    map.insert("LOCK_EX".to_string(), Value::Int(flock::LOCK_EX as i64).ref_cell());
    map.insert("LOCK_NB".to_string(), Value::Int(flock::LOCK_NB as i64).ref_cell());
    map.insert("LOCK_UN".to_string(), Value::Int(flock::LOCK_UN as i64).ref_cell());
    map.insert("F_RDLCK".to_string(), Value::Int(flock::F_RDLCK as i64).ref_cell());
    map.insert("F_WRLCK".to_string(), Value::Int(flock::F_WRLCK as i64).ref_cell());
    map.insert("F_UNLCK".to_string(), Value::Int(flock::F_UNLCK as i64).ref_cell());
    map.insert("F_GETLK".to_string(), Value::Int(flock::F_GETLK as i64).ref_cell());
    map.insert("F_SETLK".to_string(), Value::Int(flock::F_SETLK as i64).ref_cell());
    map.insert("F_SETLKW".to_string(), Value::Int(flock::F_SETLKW as i64).ref_cell());
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;
    use std::fs;

    fn span() -> Span {
        Span::dummy()
    }

    fn s(v: &str) -> ValueRef {
        Value::String(v.to_string()).ref_cell()
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("nflock_rt_{name}"))
    }

    #[test]
    fn lock_acquire_release() {
        let path = temp_path("lock.niao");
        let _ = fs::remove_file(&path);
        let h = nflock_lock(&[s(path.to_str().unwrap())], span()).unwrap();
        let id = match &*h.borrow() {
            Value::Object(m) => match &*m["handle"].borrow() {
                Value::Int(n) => *n,
                _ => panic!("bad handle"),
            },
            _ => panic!("expected object"),
        };
        let locked = nflock_locked(&[Value::Int(id).ref_cell()], span()).unwrap();
        assert!(matches!(&*locked.borrow(), Value::Bool(true)));
        nflock_release(&[Value::Int(id).ref_cell()], span()).unwrap();
        nflock_close(&[Value::Int(id).ref_cell()], span()).unwrap();
        let _ = fs::remove_file(path);
    }

    #[test]
    fn pid_roundtrip() {
        let path = temp_path("pid.niao");
        let _ = fs::remove_file(&path);
        nflock_pid_write(&[s(path.to_str().unwrap())], span()).unwrap();
        let pid = nflock_pid_read(&[s(path.to_str().unwrap())], span()).unwrap();
        assert!(matches!(&*pid.borrow(), Value::Int(n) if *n > 0));
        let alive = nflock_pid_alive(&[pid], span()).unwrap();
        assert!(matches!(&*alive.borrow(), Value::Bool(true)));
        nflock_pid_remove(&[s(path.to_str().unwrap())], span()).unwrap();
    }
}
