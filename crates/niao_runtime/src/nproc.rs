//! Native nproc standard library — child processes beyond `nshell`: Popen-style
//! streaming I/O, process pools, OS pipes, in-process IPC channels/queues,
//! file-backed shared memory, and sync primitives (~`multiprocessing` subset).
//!
//! Import with `import "nproc"` (or `import "std/nproc"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_proc as proc;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, MutexGuard};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Handle tables
// ---------------------------------------------------------------------------

struct ProcessEntry {
    child: proc::ChildProcess,
}

struct PipeEntry {
    reader: proc::os_pipe::PipeReader,
    writer: proc::os_pipe::PipeWriter,
}

struct ChannelEntry {
    ch: proc::SharedChannel<ValueRef>,
}

struct QueueEntry {
    ch: proc::SharedChannel<ValueRef>,
}

struct ShmEntry {
    shm: proc::SharedMemory,
}

struct PoolEntry {
    pool: proc::ProcessPool,
}

struct EventEntry {
    ev: proc::SharedEvent,
}

struct LockEntry {
    lock: proc::SharedLock,
}

struct SemEntry {
    sem: proc::SharedSemaphore,
}

struct BarrierEntry {
    bar: proc::SharedBarrier,
}

thread_local! {
    static PROCESSES: RefCell<HashMap<i64, ProcessEntry>> = RefCell::new(HashMap::new());
    static PIPES: RefCell<HashMap<i64, PipeEntry>> = RefCell::new(HashMap::new());
    static CHANNELS: RefCell<HashMap<i64, ChannelEntry>> = RefCell::new(HashMap::new());
    static QUEUES: RefCell<HashMap<i64, QueueEntry>> = RefCell::new(HashMap::new());
    static SHMS: RefCell<HashMap<i64, ShmEntry>> = RefCell::new(HashMap::new());
    static POOLS: RefCell<HashMap<i64, PoolEntry>> = RefCell::new(HashMap::new());
    static EVENTS: RefCell<HashMap<i64, EventEntry>> = RefCell::new(HashMap::new());
    static LOCKS: RefCell<HashMap<i64, LockEntry>> = RefCell::new(HashMap::new());
    static SEMAPHORES: RefCell<HashMap<i64, SemEntry>> = RefCell::new(HashMap::new());
    static BARRIERS: RefCell<HashMap<i64, BarrierEntry>> = RefCell::new(HashMap::new());
    static LOCK_GUARDS: RefCell<HashMap<i64, MutexGuard<'static, ()>>> = RefCell::new(HashMap::new());
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
    RuntimeError::at(span, codes::E3502_NPROC_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E3500_NPROC_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E3500_NPROC_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn nproc_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E3501_NPROC_ERROR, "nproc_error", msg.into(), span)
}

fn invalid_handle(span: Span, kind: &str, id: i64) -> ValueRef {
    error_value(
        codes::E3503_NPROC_INVALID_HANDLE,
        "nproc_error",
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

fn handle_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    int_arg(args, idx, name, span)
}

fn optional_int(args: &[ValueRef], idx: usize) -> Option<i64> {
    args.get(idx).and_then(|v| match &*v.borrow() {
        Value::Int(n) => Some(*n),
        _ => None,
    })
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
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::Int(n) if (0..=255).contains(n) => out.push(*n as u8),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() byte item {} must be 0..=255 int, got {}",
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
            format!("{name}() expects string or bytes, got {}", other.type_name()),
        )),
    }
}

fn lossy_utf8(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn argv_from_cmd(cmd: &ValueRef, span: Span) -> NiaoResult<Vec<String>> {
    match &*cmd.borrow() {
        Value::String(s) => Ok(vec![s.clone()]),
        Value::Array(items) => {
            if items.is_empty() {
                return Err(type_err(span, "command array must not be empty"));
            }
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::String(s) => out.push(s.clone()),
                    other => {
                        return Err(type_err(
                            span,
                            format!("command array[{i}] must be string, got {}", other.type_name()),
                        ));
                    }
                }
            }
            Ok(out)
        }
        other => Err(type_err(
            span,
            format!("command must be string or argv array, got {}", other.type_name()),
        )),
    }
}

#[derive(Default, Clone)]
struct SpawnOpts {
    cwd: Option<std::path::PathBuf>,
    env: HashMap<String, String>,
    stdin_pipe: bool,
    stdout_pipe: bool,
    stderr_pipe: bool,
}

fn parse_spawn_opts(opts: &ValueRef, span: Span) -> NiaoResult<SpawnOpts> {
    match &*opts.borrow() {
        Value::Object(map) => {
            let mut out = SpawnOpts::default();
            out.stdout_pipe = true;
            out.stderr_pipe = true;
            if let Some(cwd) = map.get("cwd") {
                out.cwd = Some(std::path::PathBuf::from(string_arg(&[Rc::clone(cwd)], 0, "opts.cwd", span)?));
            }
            if let Some(env) = map.get("env") {
                match &*env.borrow() {
                    Value::Object(em) => {
                        for (k, v) in em {
                            out.env.insert(k.clone(), string_arg(&[Rc::clone(v)], 0, "opts.env", span)?);
                        }
                    }
                    other => {
                        return Err(type_err(span, format!("opts.env must be object, got {}", other.type_name())));
                    }
                }
            }
            for (key, slot) in [
                ("stdin_pipe", &mut out.stdin_pipe),
                ("stdout_pipe", &mut out.stdout_pipe),
                ("stderr_pipe", &mut out.stderr_pipe),
            ] {
                if let Some(v) = map.get(key) {
                    match &*v.borrow() {
                        Value::Bool(b) => *slot = *b,
                        other => {
                            return Err(type_err(
                                span,
                                format!("opts.{key} must be bool, got {}", other.type_name()),
                            ));
                        }
                    }
                }
            }
            Ok(out)
        }
        other => Err(type_err(span, format!("opts must be object, got {}", other.type_name()))),
    }
}

fn spawn_opts_to_proc(o: &SpawnOpts) -> proc::SpawnOpts {
    proc::SpawnOpts {
        cwd: o.cwd.clone(),
        env: o.env.clone(),
        stdin_pipe: o.stdin_pipe,
        stdout_pipe: o.stdout_pipe,
        stderr_pipe: o.stderr_pipe,
    }
}

fn process_object(handle: i64, pid: u32) -> ValueRef {
    let mut map = HashMap::new();
    map.insert("handle".to_string(), ok_int(handle));
    map.insert("pid".to_string(), ok_int(pid as i64));
    Value::Object(map).ref_cell()
}

fn job_results_to_niao(results: Vec<proc::JobResult>) -> ValueRef {
    let items: Vec<ValueRef> = results
        .into_iter()
        .map(|r| {
            let mut m = HashMap::new();
            m.insert("stdout".to_string(), ok_string(lossy_utf8(&r.stdout)));
            m.insert("stderr".to_string(), ok_string(lossy_utf8(&r.stderr)));
            m.insert("code".to_string(), ok_int(r.code as i64));
            m.insert("ok".to_string(), ok_bool(r.ok));
            Value::Object(m).ref_cell()
        })
        .collect();
    Value::Array(items).ref_cell()
}

fn with_process_mut<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut proc::ChildProcess) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    PROCESSES.with(|tbl| {
        let mut tbl = tbl.borrow_mut();
        match tbl.get_mut(&id) {
            Some(e) => Ok(Ok(f(&mut e.child))),
            None => Ok(Err(invalid_handle(span, "process", id))),
        }
    })
}

// ---------------------------------------------------------------------------
// Builtins — process & pool
// ---------------------------------------------------------------------------

/// nproc_cpu_count() → int
/// // >>> nproc.cpu_count() >= 1
/// // => true
fn nproc_cpu_count(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    Ok(ok_int(proc::cpu_count() as i64))
}

/// nproc_active_count() → int
fn nproc_active_count(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    let n = PROCESSES.with(|t| {
        t.borrow_mut()
            .values_mut()
            .filter(|e| e.child.poll().is_none())
            .count()
    });
    Ok(ok_int(n as i64))
}

/// nproc_spawn(cmd, opts?) → {handle, pid}
fn nproc_spawn(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nproc_spawn", span)?;
    let argv = argv_from_cmd(&args[0], span)?;
    let opts = if args.len() == 2 {
        parse_spawn_opts(&args[1], span)?
    } else {
        let mut d = SpawnOpts::default();
        d.stdout_pipe = true;
        d.stderr_pipe = true;
        d
    };
    let program = argv[0].clone();
    let rest: Vec<String> = argv[1..].to_vec();
    match proc::ChildProcess::spawn(&program, &rest, &spawn_opts_to_proc(&opts)) {
        Ok(child) => {
            let pid = child.pid();
            let handle = alloc_handle();
            PROCESSES.with(|t| {
                t.borrow_mut().insert(handle, ProcessEntry { child });
            });
            Ok(process_object(handle, pid))
        }
        Err(e) => Ok(nproc_err(span, e.to_string())),
    }
}

/// nproc_poll(proc) → exit code int or nil when running
fn nproc_poll(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproc_poll", span)?;
    let id = handle_arg(args, 0, "nproc_poll", span)?;
    match with_process_mut(id, span, |c| c.poll())? {
        Ok(Some(code)) => Ok(ok_int(code as i64)),
        Ok(None) => Ok(ok_nil()),
        Err(e) => Ok(e),
    }
}

/// nproc_wait(proc, timeout_ms?) → exit code
fn nproc_wait(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nproc_wait", span)?;
    let id = handle_arg(args, 0, "nproc_wait", span)?;
    let timeout = optional_int(args, 1).map(|ms| Duration::from_millis(ms.max(0) as u64));
    match with_process_mut(id, span, |c| c.wait(timeout))? {
        Ok(Ok(code)) => Ok(ok_int(code as i64)),
        Ok(Err(e)) => Ok(nproc_err(span, e.to_string())),
        Err(e) => Ok(e),
    }
}

/// nproc_kill(proc) → bool
fn nproc_kill(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproc_kill", span)?;
    let id = handle_arg(args, 0, "nproc_kill", span)?;
    match with_process_mut(id, span, |c| c.kill())? {
        Ok(Ok(())) => Ok(ok_bool(true)),
        Ok(Err(e)) => Ok(nproc_err(span, e.to_string())),
        Err(e) => Ok(e),
    }
}

/// nproc_terminate(proc) → bool
fn nproc_terminate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproc_terminate", span)?;
    let id = handle_arg(args, 0, "nproc_terminate", span)?;
    match with_process_mut(id, span, |c| c.terminate())? {
        Ok(Ok(())) => Ok(ok_bool(true)),
        Ok(Err(e)) => Ok(nproc_err(span, e.to_string())),
        Err(e) => Ok(e),
    }
}

/// nproc_stdin_write(proc, data) → bytes written
fn nproc_stdin_write(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nproc_stdin_write", span)?;
    let id = handle_arg(args, 0, "nproc_stdin_write", span)?;
    let data = bytes_from_arg(&args[1], "nproc_stdin_write", span)?;
    match with_process_mut(id, span, |c| c.stdin_write(&data))? {
        Ok(Ok(n)) => Ok(ok_int(n as i64)),
        Ok(Err(e)) => Ok(nproc_err(span, e.to_string())),
        Err(e) => Ok(e),
    }
}

/// nproc_stdout_read(proc, max?) → string
fn nproc_stdout_read(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nproc_stdout_read", span)?;
    let id = handle_arg(args, 0, "nproc_stdout_read", span)?;
    let max = optional_int(args, 1).unwrap_or(65_536).max(1) as usize;
    match with_process_mut(id, span, |c| c.stdout_read(max))? {
        Ok(Ok(b)) => Ok(ok_string(lossy_utf8(&b))),
        Ok(Err(e)) => Ok(nproc_err(span, e.to_string())),
        Err(e) => Ok(e),
    }
}

/// nproc_stdout_read_all(proc) → string
fn nproc_stdout_read_all(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproc_stdout_read_all", span)?;
    let id = handle_arg(args, 0, "nproc_stdout_read_all", span)?;
    match with_process_mut(id, span, |c| c.stdout_read_all())? {
        Ok(Ok(b)) => Ok(ok_string(lossy_utf8(&b))),
        Ok(Err(e)) => Ok(nproc_err(span, e.to_string())),
        Err(e) => Ok(e),
    }
}

/// nproc_stderr_read_all(proc) → string
fn nproc_stderr_read_all(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproc_stderr_read_all", span)?;
    let id = handle_arg(args, 0, "nproc_stderr_read_all", span)?;
    match with_process_mut(id, span, |c| c.stderr_read_all())? {
        Ok(Ok(b)) => Ok(ok_string(lossy_utf8(&b))),
        Ok(Err(e)) => Ok(nproc_err(span, e.to_string())),
        Err(e) => Ok(e),
    }
}

/// nproc_communicate(proc, input?, timeout_ms?) → {stdout, stderr, code}
fn nproc_communicate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nproc_communicate", span)?;
    let id = handle_arg(args, 0, "nproc_communicate", span)?;
    let input = if args.len() >= 2 && !matches!(&*args[1].borrow(), Value::Nil) {
        Some(bytes_from_arg(&args[1], "nproc_communicate", span)?)
    } else {
        None
    };
    let timeout = optional_int(args, 2).map(|ms| Duration::from_millis(ms.max(0) as u64));
    match with_process_mut(id, span, |c| c.communicate(input.as_deref(), timeout))? {
        Ok(Ok((out, err, code))) => {
            let mut m = HashMap::new();
            m.insert("stdout".to_string(), ok_string(lossy_utf8(&out)));
            m.insert("stderr".to_string(), ok_string(lossy_utf8(&err)));
            m.insert("code".to_string(), ok_int(code as i64));
            Ok(Value::Object(m).ref_cell())
        }
        Ok(Err(e)) => Ok(nproc_err(span, e.to_string())),
        Err(e) => Ok(e),
    }
}

/// nproc_close(proc) → bool
fn nproc_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproc_close", span)?;
    let id = handle_arg(args, 0, "nproc_close", span)?;
    let removed = PROCESSES.with(|t| t.borrow_mut().remove(&id).is_some());
    Ok(ok_bool(removed))
}

/// nproc_pool(workers) → handle
fn nproc_pool(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproc_pool", span)?;
    let workers = int_arg(args, 0, "nproc_pool", span)?;
    if workers <= 0 {
        return Ok(nproc_err(span, "workers must be > 0"));
    }
    let handle = alloc_handle();
    POOLS.with(|t| {
        t.borrow_mut().insert(
            handle,
            PoolEntry {
                pool: proc::ProcessPool::new(workers as usize),
            },
        );
    });
    Ok(ok_int(handle))
}

fn argv_list_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<Vec<String>>> {
    match &*args[idx].borrow() {
        Value::Array(outer) => {
            let mut out = Vec::with_capacity(outer.len());
            for (i, item) in outer.iter().enumerate() {
                out.push(argv_from_cmd(item, span).map_err(|e| {
                    RuntimeError::at(span, codes::E3502_NPROC_TYPE, format!("{name}()[{i}]: {}", e))
                })?);
            }
            Ok(out)
        }
        other => Err(type_err(
            span,
            format!("{name}() expects array of argv arrays, got {}", other.type_name()),
        )),
    }
}

/// nproc_pool_map(pool, commands, opts?) → [{stdout, stderr, code, ok}, …]
fn nproc_pool_map(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nproc_pool_map", span)?;
    let id = handle_arg(args, 0, "nproc_pool_map", span)?;
    let commands = argv_list_arg(args, 1, "nproc_pool_map", span)?;
    let opts = if args.len() == 3 {
        spawn_opts_to_proc(&parse_spawn_opts(&args[2], span)?)
    } else {
        proc::SpawnOpts {
            stdout_pipe: true,
            stderr_pipe: true,
            ..Default::default()
        }
    };
    POOLS.with(|t| {
        let tbl = t.borrow();
        match tbl.get(&id) {
            Some(e) => match e.pool.map(&commands, &opts) {
                Ok(results) => Ok(job_results_to_niao(results)),
                Err(msg) => Ok(nproc_err(span, msg)),
            },
            None => Ok(invalid_handle(span, "pool", id)),
        }
    })
}

/// nproc_pool_map_argv(pool, template, items, opts?) → results
fn nproc_pool_map_argv(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nproc_pool_map_argv", span)?;
    let id = handle_arg(args, 0, "nproc_pool_map_argv", span)?;
    let template = argv_from_cmd(&args[1], span)?;
    let items = match &*args[2].borrow() {
        Value::Array(arr) => {
            let mut out = Vec::with_capacity(arr.len());
            for (i, v) in arr.iter().enumerate() {
                match &*v.borrow() {
                    Value::String(s) => out.push(s.clone()),
                    other => {
                        return Err(type_err(
                            span,
                            format!("items[{i}] must be string, got {}", other.type_name()),
                        ));
                    }
                }
            }
            out
        }
        other => {
            return Err(type_err(
                span,
                format!("items must be string array, got {}", other.type_name()),
            ));
        }
    };
    let opts = if args.len() == 4 {
        spawn_opts_to_proc(&parse_spawn_opts(&args[3], span)?)
    } else {
        proc::SpawnOpts {
            stdout_pipe: true,
            stderr_pipe: true,
            ..Default::default()
        }
    };
    POOLS.with(|t| {
        let tbl = t.borrow();
        match tbl.get(&id) {
            Some(e) => match e.pool.map_argv(&template, &items, &opts) {
                Ok(results) => Ok(job_results_to_niao(results)),
                Err(msg) => Ok(nproc_err(span, msg)),
            },
            None => Ok(invalid_handle(span, "pool", id)),
        }
    })
}

/// nproc_pool_close(pool) → bool
fn nproc_pool_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproc_pool_close", span)?;
    let id = handle_arg(args, 0, "nproc_pool_close", span)?;
    POOLS.with(|t| {
        let mut tbl = t.borrow_mut();
        if let Some(e) = tbl.get_mut(&id) {
            e.pool.close();
            Ok(ok_bool(true))
        } else {
            Ok(invalid_handle(span, "pool", id))
        }
    })
}

/// nproc_pool_join(pool) → nil
fn nproc_pool_join(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproc_pool_join", span)?;
    let id = handle_arg(args, 0, "nproc_pool_join", span)?;
    POOLS.with(|t| {
        let tbl = t.borrow();
        match tbl.get(&id) {
            Some(e) => match e.pool.join() {
                Ok(()) => Ok(ok_nil()),
                Err(msg) => Ok(nproc_err(span, msg)),
            },
            None => Ok(invalid_handle(span, "pool", id)),
        }
    })
}

// ---------------------------------------------------------------------------
// OS pipes
// ---------------------------------------------------------------------------

/// nproc_pipe() → {handle}
fn nproc_pipe(_args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    match proc::OsPipe::new() {
        Ok(p) => {
            let handle = alloc_handle();
            PIPES.with(|t| {
                t.borrow_mut().insert(
                    handle,
                    PipeEntry {
                        reader: p.reader,
                        writer: p.writer,
                    },
                );
            });
            let mut m = HashMap::new();
            m.insert("handle".to_string(), ok_int(handle));
            Ok(Value::Object(m).ref_cell())
        }
        Err(e) => Ok(nproc_err(span, e.to_string())),
    }
}

/// nproc_pipe_read(pipe, max?) → string
fn nproc_pipe_read(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nproc_pipe_read", span)?;
    let id = handle_arg(args, 0, "nproc_pipe_read", span)?;
    let max = optional_int(args, 1).unwrap_or(65_536).max(1) as usize;
    PIPES.with(|t| {
        let mut tbl = t.borrow_mut();
        match tbl.get_mut(&id) {
            Some(p) => {
                let mut buf = vec![0u8; max];
                match p.reader.read(&mut buf) {
                    Ok(n) => {
                        buf.truncate(n);
                        Ok(ok_string(lossy_utf8(&buf)))
                    }
                    Err(e) => Ok(nproc_err(span, e.to_string())),
                }
            }
            None => Ok(invalid_handle(span, "pipe", id)),
        }
    })
}

/// nproc_pipe_write(pipe, data) → int
fn nproc_pipe_write(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nproc_pipe_write", span)?;
    let id = handle_arg(args, 0, "nproc_pipe_write", span)?;
    let data = bytes_from_arg(&args[1], "nproc_pipe_write", span)?;
    PIPES.with(|t| {
        let mut tbl = t.borrow_mut();
        match tbl.get_mut(&id) {
            Some(p) => match p.writer.write_all(&data) {
                Ok(()) => Ok(ok_int(data.len() as i64)),
                Err(e) => Ok(nproc_err(span, e.to_string())),
            },
            None => Ok(invalid_handle(span, "pipe", id)),
        }
    })
}

/// nproc_pipe_close_read(pipe) → bool
fn nproc_pipe_close_read(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproc_pipe_close_read", span)?;
    let id = handle_arg(args, 0, "nproc_pipe_close_read", span)?;
    PIPES.with(|t| {
        let mut tbl = t.borrow_mut();
        if let Some(p) = tbl.get_mut(&id) {
            p.reader.close();
            Ok(ok_bool(true))
        } else {
            Ok(invalid_handle(span, "pipe", id))
        }
    })
}

/// nproc_pipe_close_write(pipe) → bool
fn nproc_pipe_close_write(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproc_pipe_close_write", span)?;
    let id = handle_arg(args, 0, "nproc_pipe_close_write", span)?;
    PIPES.with(|t| {
        let mut tbl = t.borrow_mut();
        if let Some(p) = tbl.get_mut(&id) {
            p.writer.close();
            Ok(ok_bool(true))
        } else {
            Ok(invalid_handle(span, "pipe", id))
        }
    })
}

// ---------------------------------------------------------------------------
// Channels & queues (in-process)
// ---------------------------------------------------------------------------

/// nproc_channel(capacity?) → handle
fn nproc_channel(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nproc_channel", span)?;
    let ch = if args.is_empty() {
        Arc::new(proc::Channel::unbounded())
    } else {
        let cap = int_arg(args, 0, "nproc_channel", span)?;
        if cap <= 0 {
            return Ok(nproc_err(span, "capacity must be > 0"));
        }
        Arc::new(proc::Channel::bounded(cap as usize))
    };
    let handle = alloc_handle();
    CHANNELS.with(|t| {
        t.borrow_mut().insert(handle, ChannelEntry { ch });
    });
    Ok(ok_int(handle))
}

/// nproc_channel_send(ch, value) → bool
fn nproc_channel_send(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nproc_channel_send", span)?;
    let id = handle_arg(args, 0, "nproc_channel_send", span)?;
    CHANNELS.with(|t| {
        let tbl = t.borrow();
        match tbl.get(&id) {
            Some(e) => match e.ch.send(Rc::clone(&args[1])) {
                Ok(()) => Ok(ok_bool(true)),
                Err(msg) => Ok(nproc_err(span, msg)),
            },
            None => Ok(invalid_handle(span, "channel", id)),
        }
    })
}

/// nproc_channel_recv(ch, timeout_ms?) → value or nil
fn nproc_channel_recv(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nproc_channel_recv", span)?;
    let id = handle_arg(args, 0, "nproc_channel_recv", span)?;
    let timeout = optional_int(args, 1).map(|ms| ms.max(0) as u64);
    CHANNELS.with(|t| {
        let tbl = t.borrow();
        match tbl.get(&id) {
            Some(e) => match e.ch.recv(timeout) {
                Ok(Some(v)) => Ok(v),
                Ok(None) => Ok(ok_nil()),
                Err(msg) => Ok(nproc_err(span, msg)),
            },
            None => Ok(invalid_handle(span, "channel", id)),
        }
    })
}

/// nproc_channel_try_recv(ch) → value or nil
fn nproc_channel_try_recv(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproc_channel_try_recv", span)?;
    let id = handle_arg(args, 0, "nproc_channel_try_recv", span)?;
    CHANNELS.with(|t| {
        let tbl = t.borrow();
        match tbl.get(&id) {
            Some(e) => match e.ch.try_recv() {
                Ok(Some(v)) => Ok(v),
                Ok(None) => Ok(ok_nil()),
                Err(msg) => Ok(nproc_err(span, msg)),
            },
            None => Ok(invalid_handle(span, "channel", id)),
        }
    })
}

/// nproc_channel_close(ch) → bool
fn nproc_channel_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproc_channel_close", span)?;
    let id = handle_arg(args, 0, "nproc_channel_close", span)?;
    CHANNELS.with(|t| {
        let mut tbl = t.borrow_mut();
        if let Some(e) = tbl.remove(&id) {
            e.ch.close();
            Ok(ok_bool(true))
        } else {
            Ok(invalid_handle(span, "channel", id))
        }
    })
}

/// nproc_queue() → handle
fn nproc_queue(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    let handle = alloc_handle();
    QUEUES.with(|t| {
        t.borrow_mut().insert(
            handle,
            QueueEntry {
                ch: Arc::new(proc::Channel::unbounded()),
            },
        );
    });
    Ok(ok_int(handle))
}

/// nproc_queue_put(q, value) → bool
fn nproc_queue_put(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nproc_queue_put", span)?;
    let id = handle_arg(args, 0, "nproc_queue_put", span)?;
    QUEUES.with(|t| {
        let tbl = t.borrow();
        match tbl.get(&id) {
            Some(e) => match e.ch.send(Rc::clone(&args[1])) {
                Ok(()) => Ok(ok_bool(true)),
                Err(msg) => Ok(nproc_err(span, msg)),
            },
            None => Ok(invalid_handle(span, "queue", id)),
        }
    })
}

/// nproc_queue_get(q, timeout_ms?) → value or nil
fn nproc_queue_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nproc_queue_get", span)?;
    let id = handle_arg(args, 0, "nproc_queue_get", span)?;
    let timeout = optional_int(args, 1).map(|ms| ms.max(0) as u64);
    QUEUES.with(|t| {
        let tbl = t.borrow();
        match tbl.get(&id) {
            Some(e) => match e.ch.recv(timeout) {
                Ok(Some(v)) => Ok(v),
                Ok(None) => Ok(ok_nil()),
                Err(msg) => Ok(nproc_err(span, msg)),
            },
            None => Ok(invalid_handle(span, "queue", id)),
        }
    })
}

/// nproc_queue_try_get(q) → value or nil
fn nproc_queue_try_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproc_queue_try_get", span)?;
    let id = handle_arg(args, 0, "nproc_queue_try_get", span)?;
    QUEUES.with(|t| {
        let tbl = t.borrow();
        match tbl.get(&id) {
            Some(e) => match e.ch.try_recv() {
                Ok(Some(v)) => Ok(v),
                Ok(None) => Ok(ok_nil()),
                Err(msg) => Ok(nproc_err(span, msg)),
            },
            None => Ok(invalid_handle(span, "queue", id)),
        }
    })
}

/// nproc_queue_close(q) → bool
fn nproc_queue_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproc_queue_close", span)?;
    let id = handle_arg(args, 0, "nproc_queue_close", span)?;
    QUEUES.with(|t| {
        let mut tbl = t.borrow_mut();
        if let Some(e) = tbl.remove(&id) {
            e.ch.close();
            Ok(ok_bool(true))
        } else {
            Ok(invalid_handle(span, "queue", id))
        }
    })
}

// ---------------------------------------------------------------------------
// Shared memory
// ---------------------------------------------------------------------------

/// nproc_shared_memory(name, size) → handle
fn nproc_shared_memory(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nproc_shared_memory", span)?;
    let name = string_arg(args, 0, "nproc_shared_memory", span)?;
    let size = int_arg(args, 1, "nproc_shared_memory", span)?;
    if size <= 0 {
        return Ok(nproc_err(span, "size must be > 0"));
    }
    match proc::SharedMemory::create(&name, size as usize) {
        Ok(shm) => {
            let handle = alloc_handle();
            SHMS.with(|t| {
                t.borrow_mut().insert(handle, ShmEntry { shm });
            });
            Ok(ok_int(handle))
        }
        Err(e) => Ok(nproc_err(span, e.to_string())),
    }
}

/// nproc_shared_open(name) → handle
fn nproc_shared_open(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproc_shared_open", span)?;
    let name = string_arg(args, 0, "nproc_shared_open", span)?;
    match proc::SharedMemory::open(&name) {
        Ok(shm) => {
            let handle = alloc_handle();
            SHMS.with(|t| {
                t.borrow_mut().insert(handle, ShmEntry { shm });
            });
            Ok(ok_int(handle))
        }
        Err(e) => Ok(nproc_err(span, e.to_string())),
    }
}

/// nproc_shared_read(shm, offset, len) → string
fn nproc_shared_read(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nproc_shared_read", span)?;
    let id = handle_arg(args, 0, "nproc_shared_read", span)?;
    let offset = int_arg(args, 1, "nproc_shared_read", span)?;
    let len = int_arg(args, 2, "nproc_shared_read", span)?;
    if offset < 0 || len < 0 {
        return Ok(nproc_err(span, "offset and len must be >= 0"));
    }
    SHMS.with(|t| {
        let tbl = t.borrow();
        match tbl.get(&id) {
            Some(e) => match e.shm.read(offset as usize, len as usize) {
                Ok(b) => Ok(ok_string(lossy_utf8(&b))),
                Err(er) => Ok(nproc_err(span, er.to_string())),
            },
            None => Ok(invalid_handle(span, "shared memory", id)),
        }
    })
}

/// nproc_shared_write(shm, offset, data) → int
fn nproc_shared_write(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nproc_shared_write", span)?;
    let id = handle_arg(args, 0, "nproc_shared_write", span)?;
    let offset = int_arg(args, 1, "nproc_shared_write", span)?;
    if offset < 0 {
        return Ok(nproc_err(span, "offset must be >= 0"));
    }
    let data = bytes_from_arg(&args[2], "nproc_shared_write", span)?;
    SHMS.with(|t| {
        let mut tbl = t.borrow_mut();
        match tbl.get_mut(&id) {
            Some(e) => match e.shm.write(offset as usize, &data) {
                Ok(n) => Ok(ok_int(n as i64)),
                Err(er) => Ok(nproc_err(span, er.to_string())),
            },
            None => Ok(invalid_handle(span, "shared memory", id)),
        }
    })
}

/// nproc_shared_size(shm) → int
fn nproc_shared_size(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproc_shared_size", span)?;
    let id = handle_arg(args, 0, "nproc_shared_size", span)?;
    SHMS.with(|t| {
        let tbl = t.borrow();
        match tbl.get(&id) {
            Some(e) => Ok(ok_int(e.shm.len() as i64)),
            None => Ok(invalid_handle(span, "shared memory", id)),
        }
    })
}

/// nproc_shared_unlink(name) → bool
fn nproc_shared_unlink(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproc_shared_unlink", span)?;
    let name = string_arg(args, 0, "nproc_shared_unlink", span)?;
    match proc::SharedMemory::unlink(&name) {
        Ok(b) => Ok(ok_bool(b)),
        Err(e) => Ok(nproc_err(span, e.to_string())),
    }
}

// ---------------------------------------------------------------------------
// Sync primitives
// ---------------------------------------------------------------------------

/// nproc_event() → handle
fn nproc_event(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    let handle = alloc_handle();
    EVENTS.with(|t| {
        t.borrow_mut().insert(handle, EventEntry { ev: Arc::new(proc::Event::new()) });
    });
    Ok(ok_int(handle))
}

fn nproc_event_set(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproc_event_set", span)?;
    let id = handle_arg(args, 0, "nproc_event_set", span)?;
    EVENTS.with(|t| {
        let tbl = t.borrow();
        match tbl.get(&id) {
            Some(e) => {
                e.ev.set();
                Ok(ok_nil())
            }
            None => Ok(invalid_handle(span, "event", id)),
        }
    })
}

fn nproc_event_clear(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproc_event_clear", span)?;
    let id = handle_arg(args, 0, "nproc_event_clear", span)?;
    EVENTS.with(|t| {
        let tbl = t.borrow();
        match tbl.get(&id) {
            Some(e) => {
                e.ev.clear();
                Ok(ok_nil())
            }
            None => Ok(invalid_handle(span, "event", id)),
        }
    })
}

fn nproc_event_wait(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nproc_event_wait", span)?;
    let id = handle_arg(args, 0, "nproc_event_wait", span)?;
    let timeout = optional_int(args, 1).map(|ms| ms.max(0) as u64);
    EVENTS.with(|t| {
        let tbl = t.borrow();
        match tbl.get(&id) {
            Some(e) => Ok(ok_bool(e.ev.wait(timeout))),
            None => Ok(invalid_handle(span, "event", id)),
        }
    })
}

fn nproc_event_is_set(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproc_event_is_set", span)?;
    let id = handle_arg(args, 0, "nproc_event_is_set", span)?;
    EVENTS.with(|t| {
        let tbl = t.borrow();
        match tbl.get(&id) {
            Some(e) => Ok(ok_bool(e.ev.is_set())),
            None => Ok(invalid_handle(span, "event", id)),
        }
    })
}

/// nproc_lock() → handle
fn nproc_lock(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    let handle = alloc_handle();
    LOCKS.with(|t| {
        t.borrow_mut().insert(handle, LockEntry { lock: Arc::new(proc::Lock::new()) });
    });
    Ok(ok_int(handle))
}

fn store_lock_guard(id: i64, guard: MutexGuard<'_, ()>) {
    LOCK_GUARDS.with(|g| {
        g.borrow_mut().insert(id, unsafe {
            std::mem::transmute::<MutexGuard<'_, ()>, MutexGuard<'static, ()>>(guard)
        });
    });
}

fn nproc_lock_acquire(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nproc_lock_acquire", span)?;
    let id = handle_arg(args, 0, "nproc_lock_acquire", span)?;
    let timeout = optional_int(args, 1).map(|ms| ms.max(0) as u64);
    LOCKS.with(|t| {
        let tbl = t.borrow();
        match tbl.get(&id) {
            Some(e) => {
                if LOCK_GUARDS.with(|g| g.borrow().contains_key(&id)) {
                    return Ok(nproc_err(span, "lock already held on this thread"));
                }
                if let Some(ms) = timeout {
                    let start = std::time::Instant::now();
                    loop {
                        match e.lock.try_lock_inner() {
                            Ok(guard) => {
                                store_lock_guard(id, guard);
                                return Ok(ok_bool(true));
                            }
                            Err(_) if start.elapsed().as_millis() as u64 >= ms => {
                                return Ok(ok_bool(false));
                            }
                            Err(msg) if msg.contains("poisoned") => return Ok(nproc_err(span, msg)),
                            Err(_) => std::thread::sleep(std::time::Duration::from_millis(1)),
                        }
                    }
                }
                match e.lock.lock_inner() {
                    Ok(guard) => {
                        store_lock_guard(id, guard);
                        Ok(ok_bool(true))
                    }
                    Err(msg) => Ok(nproc_err(span, msg)),
                }
            }
            None => Ok(invalid_handle(span, "lock", id)),
        }
    })
}

fn nproc_lock_try_acquire(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproc_lock_try_acquire", span)?;
    let id = handle_arg(args, 0, "nproc_lock_try_acquire", span)?;
    LOCKS.with(|t| {
        let tbl = t.borrow();
        match tbl.get(&id) {
            Some(e) => {
                if LOCK_GUARDS.with(|g| g.borrow().contains_key(&id)) {
                    return Ok(ok_bool(false));
                }
                match e.lock.try_lock_inner() {
                    Ok(guard) => {
                        store_lock_guard(id, guard);
                        Ok(ok_bool(true))
                    }
                    Err(_) => Ok(ok_bool(false)),
                }
            }
            None => Ok(invalid_handle(span, "lock", id)),
        }
    })
}

fn nproc_lock_release(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproc_lock_release", span)?;
    let id = handle_arg(args, 0, "nproc_lock_release", span)?;
    let released = LOCK_GUARDS.with(|g| g.borrow_mut().remove(&id).is_some());
    if released {
        Ok(ok_bool(true))
    } else {
        Ok(nproc_err(span, "lock not held on this thread"))
    }
}

/// nproc_semaphore(n) → handle
fn nproc_semaphore(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproc_semaphore", span)?;
    let n = int_arg(args, 0, "nproc_semaphore", span)?;
    if n < 0 {
        return Ok(nproc_err(span, "permits must be >= 0"));
    }
    let handle = alloc_handle();
    SEMAPHORES.with(|t| {
        t.borrow_mut().insert(
            handle,
            SemEntry {
                sem: Arc::new(proc::Semaphore::new(n as usize)),
            },
        );
    });
    Ok(ok_int(handle))
}

fn nproc_semaphore_acquire(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nproc_semaphore_acquire", span)?;
    let id = handle_arg(args, 0, "nproc_semaphore_acquire", span)?;
    let timeout = optional_int(args, 1).map(|ms| ms.max(0) as u64);
    SEMAPHORES.with(|t| {
        let tbl = t.borrow();
        match tbl.get(&id) {
            Some(e) => Ok(ok_bool(e.sem.acquire(timeout))),
            None => Ok(invalid_handle(span, "semaphore", id)),
        }
    })
}

fn nproc_semaphore_try_acquire(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproc_semaphore_try_acquire", span)?;
    let id = handle_arg(args, 0, "nproc_semaphore_try_acquire", span)?;
    SEMAPHORES.with(|t| {
        let tbl = t.borrow();
        match tbl.get(&id) {
            Some(e) => Ok(ok_bool(e.sem.try_acquire())),
            None => Ok(invalid_handle(span, "semaphore", id)),
        }
    })
}

fn nproc_semaphore_release(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproc_semaphore_release", span)?;
    let id = handle_arg(args, 0, "nproc_semaphore_release", span)?;
    SEMAPHORES.with(|t| {
        let tbl = t.borrow();
        match tbl.get(&id) {
            Some(e) => {
                e.sem.release();
                Ok(ok_nil())
            }
            None => Ok(invalid_handle(span, "semaphore", id)),
        }
    })
}

/// nproc_barrier(parties) → handle
fn nproc_barrier(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nproc_barrier", span)?;
    let parties = int_arg(args, 0, "nproc_barrier", span)?;
    if parties <= 0 {
        return Ok(nproc_err(span, "parties must be > 0"));
    }
    match proc::Barrier::new(parties as usize) {
        Ok(bar) => {
            let handle = alloc_handle();
            BARRIERS.with(|t| {
                t.borrow_mut().insert(handle, BarrierEntry { bar: Arc::new(bar) });
            });
            Ok(ok_int(handle))
        }
        Err(msg) => Ok(nproc_err(span, msg)),
    }
}

fn nproc_barrier_wait(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nproc_barrier_wait", span)?;
    let id = handle_arg(args, 0, "nproc_barrier_wait", span)?;
    let timeout = optional_int(args, 1).map(|ms| ms.max(0) as u64);
    BARRIERS.with(|t| {
        let tbl = t.borrow();
        match tbl.get(&id) {
            Some(e) => match e.bar.wait(timeout) {
                Ok(n) => Ok(ok_int(n as i64)),
                Err(msg) => Ok(nproc_err(span, msg)),
            },
            None => Ok(invalid_handle(span, "barrier", id)),
        }
    })
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nproc_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nproc_fns![
    ("nproc_cpu_count", "cpu_count", nproc_cpu_count),
    ("nproc_active_count", "active_count", nproc_active_count),
    ("nproc_spawn", "spawn", nproc_spawn),
    ("nproc_poll", "poll", nproc_poll),
    ("nproc_wait", "wait", nproc_wait),
    ("nproc_kill", "kill", nproc_kill),
    ("nproc_terminate", "terminate", nproc_terminate),
    ("nproc_stdin_write", "stdin_write", nproc_stdin_write),
    ("nproc_stdout_read", "stdout_read", nproc_stdout_read),
    ("nproc_stdout_read_all", "stdout_read_all", nproc_stdout_read_all),
    ("nproc_stderr_read_all", "stderr_read_all", nproc_stderr_read_all),
    ("nproc_communicate", "communicate", nproc_communicate),
    ("nproc_close", "close", nproc_close),
    ("nproc_pool", "pool", nproc_pool),
    ("nproc_pool_map", "pool_map", nproc_pool_map),
    ("nproc_pool_map_argv", "pool_map_argv", nproc_pool_map_argv),
    ("nproc_pool_close", "pool_close", nproc_pool_close),
    ("nproc_pool_join", "pool_join", nproc_pool_join),
    ("nproc_pipe", "pipe", nproc_pipe),
    ("nproc_pipe_read", "pipe_read", nproc_pipe_read),
    ("nproc_pipe_write", "pipe_write", nproc_pipe_write),
    ("nproc_pipe_close_read", "pipe_close_read", nproc_pipe_close_read),
    ("nproc_pipe_close_write", "pipe_close_write", nproc_pipe_close_write),
    ("nproc_channel", "channel", nproc_channel),
    ("nproc_channel_send", "channel_send", nproc_channel_send),
    ("nproc_channel_recv", "channel_recv", nproc_channel_recv),
    ("nproc_channel_try_recv", "channel_try_recv", nproc_channel_try_recv),
    ("nproc_channel_close", "channel_close", nproc_channel_close),
    ("nproc_queue", "queue", nproc_queue),
    ("nproc_queue_put", "queue_put", nproc_queue_put),
    ("nproc_queue_get", "queue_get", nproc_queue_get),
    ("nproc_queue_try_get", "queue_try_get", nproc_queue_try_get),
    ("nproc_queue_close", "queue_close", nproc_queue_close),
    ("nproc_shared_memory", "shared_memory", nproc_shared_memory),
    ("nproc_shared_open", "shared_open", nproc_shared_open),
    ("nproc_shared_read", "shared_read", nproc_shared_read),
    ("nproc_shared_write", "shared_write", nproc_shared_write),
    ("nproc_shared_size", "shared_size", nproc_shared_size),
    ("nproc_shared_unlink", "shared_unlink", nproc_shared_unlink),
    ("nproc_event", "event", nproc_event),
    ("nproc_event_set", "event_set", nproc_event_set),
    ("nproc_event_clear", "event_clear", nproc_event_clear),
    ("nproc_event_wait", "event_wait", nproc_event_wait),
    ("nproc_event_is_set", "event_is_set", nproc_event_is_set),
    ("nproc_lock", "lock", nproc_lock),
    ("nproc_lock_acquire", "lock_acquire", nproc_lock_acquire),
    ("nproc_lock_try_acquire", "lock_try_acquire", nproc_lock_try_acquire),
    ("nproc_lock_release", "lock_release", nproc_lock_release),
    ("nproc_semaphore", "semaphore", nproc_semaphore),
    ("nproc_semaphore_acquire", "semaphore_acquire", nproc_semaphore_acquire),
    ("nproc_semaphore_try_acquire", "semaphore_try_acquire", nproc_semaphore_try_acquire),
    ("nproc_semaphore_release", "semaphore_release", nproc_semaphore_release),
    ("nproc_barrier", "barrier", nproc_barrier),
    ("nproc_barrier_wait", "barrier_wait", nproc_barrier_wait),
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

pub const MODULE_NAME: &str = "nproc";
pub const MODULE_PATHS: &[&str] = &["nproc", "std/nproc"];

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
    fn cpu_count_positive() {
        match &*nproc_cpu_count(&[], span()).unwrap().borrow() {
            Value::Int(n) => assert!(*n >= 1),
            other => panic!("expected int, got {other:?}"),
        }
    }

    #[test]
    fn channel_send_recv() {
        let ch = nproc_channel(&[], span()).unwrap();
        let id = match &*ch.borrow() {
            Value::Int(n) => *n,
            other => panic!("expected handle, got {other:?}"),
        };
        let msg = Value::String("ping".into()).ref_cell();
        nproc_channel_send(&[ok_int(id), msg], span()).unwrap();
        let got = nproc_channel_recv(&[ok_int(id)], span()).unwrap();
        match &*got.borrow() {
            Value::String(s) => assert_eq!(s, "ping"),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    #[cfg(windows)]
    fn spawn_echo() {
        let cmd = Value::Array(vec![
            Value::String("cmd".into()).ref_cell(),
            Value::String("/C".into()).ref_cell(),
            Value::String("echo hi".into()).ref_cell(),
        ])
        .ref_cell();
        let proc = nproc_spawn(&[cmd], span()).unwrap();
        let handle = match &*proc.borrow() {
            Value::Object(m) => match &*m["handle"].borrow() {
                Value::Int(n) => *n,
                other => panic!("bad handle {other:?}"),
            },
            other => panic!("expected object {other:?}"),
        };
        let out = nproc_stdout_read_all(&[ok_int(handle)], span()).unwrap();
        let _ = nproc_wait(&[ok_int(handle)], span()).unwrap();
        match &*out.borrow() {
            Value::String(s) => assert!(s.contains("hi")),
            other => panic!("expected stdout {other:?}"),
        }
    }
}
