//! Native nasync standard library — structured async ergonomics over shared
//! background tasks: spawn, gather/race, timeouts, cancellation, async channels
//! (~asyncio / trio subset).
//!
//! Import with `import "nasync"` (or `import "std/nasync"`).

use crate::async_tasks::{
    cancel_task, shield_task, spawn_async, task_done, task_result_value, task_wait_all,
    task_wait_any, task_wait_loop, task_wait_timeout, with_task, AsyncState, AsyncValue,
};
use crate::parallel::{sendable_to_value_ref, store_callee, take_callee, value_to_sendable};
use crate::{call_niao_function, error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc::{self, RecvTimeoutError, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const E3473_NASYNC_ARITY: u32 = codes::E3473_NASYNC_ARITY;
const E3474_NASYNC_ERROR: u32 = codes::E3474_NASYNC_ERROR;
const E3475_NASYNC_TYPE: u32 = codes::E3475_NASYNC_TYPE;
const E3476_NASYNC_TASK_NOT_FOUND: u32 = codes::E3476_NASYNC_TASK_NOT_FOUND;
const E3477_NASYNC_TIMEOUT: u32 = codes::E3477_NASYNC_TIMEOUT;
const E3478_NASYNC_INVALID_HANDLE: u32 = codes::E3478_NASYNC_INVALID_HANDLE;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3475_NASYNC_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3473_NASYNC_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3473_NASYNC_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_min(args: &[ValueRef], min: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min {
        return Err(RuntimeError::at(
            span,
            E3473_NASYNC_ARITY,
            format!("{name}() expects at least {min} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn nasync_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3474_NASYNC_ERROR, "nasync_error", msg.into(), span)
}

fn ok_bool(b: bool) -> ValueRef {
    Value::Bool(b).ref_cell()
}

fn ok_int(n: i64) -> ValueRef {
    Value::Int(n).ref_cell()
}

fn ok_nil() -> ValueRef {
    Value::Nil.ref_cell()
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

fn task_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<u64> {
    let id = int_arg(args, idx, name, span)?;
    if id <= 0 {
        return Err(type_err(
            span,
            format!("{name}() expects a positive task id as argument {}", idx + 1),
        ));
    }
    Ok(id as u64)
}

fn handle_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<u64> {
    let id = int_arg(args, idx, name, span)?;
    if id <= 0 {
        return Err(type_err(
            span,
            format!("{name}() expects a positive handle as argument {}", idx + 1),
        ));
    }
    Ok(id as u64)
}

fn function_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<ValueRef> {
    match &*args[idx].borrow() {
        Value::Function(_) | Value::NativeFunction(_) => Ok(Rc::clone(&args[idx])),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a callable as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn task_ids_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<u64>> {
    match &*args[idx].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::Int(n) if *n > 0 => out.push(*n as u64),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() expects an array of positive task ids at argument {}, element {}: got {}",
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
                "{name}() expects an array of task ids as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn value_to_async(val: &Value) -> AsyncValue {
    match val {
        Value::Nil => AsyncValue::nil(),
        Value::Int(n) => AsyncValue::int(*n),
        Value::Bool(b) => AsyncValue::Bool(*b),
        Value::Float(f) => AsyncValue::Float(*f),
        Value::String(s) => AsyncValue::String(s.clone()),
        Value::IntArray(v) => AsyncValue::IntArray(v.clone()),
        Value::ByteArray(v) => AsyncValue::ByteArray(v.clone()),
        Value::Array(items) => {
            AsyncValue::Array(items.iter().map(|v| value_to_async(&*v.borrow())).collect())
        }
        Value::Object(map) => {
            let mut out = HashMap::with_capacity(map.len());
            for (k, v) in map {
                out.insert(k.clone(), value_to_async(&*v.borrow()));
            }
            AsyncValue::Object(out)
        }
        other => AsyncValue::String(other.type_name().to_string()),
    }
}

fn async_error_factory(_span: Span) -> impl Fn(Span, String) -> ValueRef {
    move |s, msg| nasync_err(s, msg)
}

fn task_result(state: &AsyncState, span: Span) -> ValueRef {
    task_result_value(
        state,
        span,
        "async task cancelled",
        async_error_factory(span),
    )
}

fn sendable_args_rest(
    args: &[ValueRef],
    start: usize,
    name: &str,
    span: Span,
) -> NiaoResult<Vec<crate::parallel::SendableValue>> {
    let mut out = Vec::with_capacity(args.len().saturating_sub(start));
    for (i, arg) in args.iter().enumerate().skip(start) {
        let v = value_to_sendable(&arg.borrow()).map_err(|e| {
            type_err(
                span,
                format!("{name}() argument {} is not sendable across threads: {e}", i + 1),
            )
        })?;
        out.push(v);
    }
    Ok(out)
}

fn spawn_callable(callee: ValueRef, sendable_args: Vec<crate::parallel::SendableValue>, span: Span) -> u64 {
    let callee_id = store_callee(callee);
    spawn_async(move || {
        let callee = take_callee(callee_id).unwrap_or_else(|| Value::Nil.ref_cell());
        let args: Vec<ValueRef> = sendable_args
            .into_iter()
            .map(sendable_to_value_ref)
            .collect();
        match call_niao_function(callee, &args, span) {
            Ok(v) => {
                if let Some(err) = crate::value_to_error(&v.borrow()) {
                    Err(err.message)
                } else {
                    Ok(value_to_async(&v.borrow()))
                }
            }
            Err(e) => Err(e.to_string()),
        }
    })
}

// ---------------------------------------------------------------------------
// Task primitives
// ---------------------------------------------------------------------------

// >>> import "nasync"
// >>> let t = nasync.spawn(fn() { return 42 })
// >>> nasync.wait(t)
// => 42
fn nasync_spawn(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_min(args, 1, "nasync_spawn", span)?;
    let callee = function_arg(args, 0, "nasync_spawn", span)?;
    let sendable = sendable_args_rest(args, 1, "nasync_spawn", span)?;
    let id = spawn_callable(callee, sendable, span);
    Ok(ok_int(id as i64))
}

// >>> nasync.create_task(fn() { return 1 })
// => <task id>
fn nasync_create_task(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    nasync_spawn(args, span)
}

// >>> nasync.sleep_async(1)
// => <task id>
fn nasync_sleep_async(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nasync_sleep_async", span)?;
    let ms = int_arg(args, 0, "nasync_sleep_async", span)?;
    if ms < 0 {
        return Ok(nasync_err(span, "sleep_async() timeout must be non-negative"));
    }
    let id = spawn_async(move || {
        if ms > 0 {
            thread::sleep(Duration::from_millis(ms as u64));
        }
        Ok(AsyncValue::nil())
    });
    Ok(ok_int(id as i64))
}

// >>> nasync.sleep(0)
// => nil
fn nasync_sleep(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nasync_sleep", span)?;
    let ms = int_arg(args, 0, "nasync_sleep", span)?;
    if ms < 0 {
        return Ok(nasync_err(span, "sleep() timeout must be non-negative"));
    }
    if ms > 0 {
        thread::sleep(Duration::from_millis(ms as u64));
    }
    Ok(ok_nil())
}

// >>> nasync.done(999999)
// => false
fn nasync_done(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nasync_done", span)?;
    let id = task_arg(args, 0, "nasync_done", span)?;
    with_task(
        id,
        "nasync_done",
        span,
        E3476_NASYNC_TASK_NOT_FOUND,
        "",
        async_error_factory(span),
        |state| Ok(ok_bool(task_done(state))),
    )
}

// >>> nasync.poll(<pending task>)
// => nil
fn nasync_poll(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nasync_poll", span)?;
    let id = task_arg(args, 0, "nasync_poll", span)?;
    with_task(
        id,
        "nasync_poll",
        span,
        E3476_NASYNC_TASK_NOT_FOUND,
        "async task cancelled",
        async_error_factory(span),
        |state| Ok(task_result(state, span)),
    )
}

// >>> nasync.wait(<task>)
// => <result>
fn nasync_wait(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nasync_wait", span)?;
    let id = task_arg(args, 0, "nasync_wait", span)?;
    task_wait_loop(id);
    with_task(
        id,
        "nasync_wait",
        span,
        E3476_NASYNC_TASK_NOT_FOUND,
        "async task cancelled",
        async_error_factory(span),
        |state| Ok(task_result(state, span)),
    )
}

// >>> nasync.result(<task>)
// => <result>
fn nasync_result(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    nasync_wait(args, span)
}

// >>> nasync.cancel(<task>)
// => true
fn nasync_cancel(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nasync_cancel", span)?;
    let id = task_arg(args, 0, "nasync_cancel", span)?;
    let cancelled = cancel_task(id, span, E3476_NASYNC_TASK_NOT_FOUND)?;
    Ok(ok_bool(cancelled))
}

// >>> nasync.shield(<task>)
// => true
fn nasync_shield(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nasync_shield", span)?;
    let id = task_arg(args, 0, "nasync_shield", span)?;
    let ok = shield_task(id);
    if !ok {
        return Err(RuntimeError::at(
            span,
            E3476_NASYNC_TASK_NOT_FOUND,
            format!("nasync_shield(): task {id} not found"),
        ));
    }
    Ok(ok_bool(true))
}

// >>> nasync.status(<task>)
// => "pending"
fn nasync_status(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nasync_status", span)?;
    let id = task_arg(args, 0, "nasync_status", span)?;
    with_task(
        id,
        "nasync_status",
        span,
        E3476_NASYNC_TASK_NOT_FOUND,
        "",
        async_error_factory(span),
        |state| {
            let s = match state {
                AsyncState::Pending => "pending",
                AsyncState::Cancelled => "cancelled",
                AsyncState::Done(Ok(_)) => "done",
                AsyncState::Done(Err(_)) => "error",
            };
            Ok(Value::String(s.to_string()).ref_cell())
        },
    )
}

// ---------------------------------------------------------------------------
// Combinators
// ---------------------------------------------------------------------------

// >>> nasync.gather([t1, t2])
// => [r1, r2]
fn nasync_gather(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nasync_gather", span)?;
    let ids = task_ids_arg(args, 0, "nasync_gather", span)?;
    task_wait_all(&ids);
    let mut results = Vec::with_capacity(ids.len());
    for id in ids {
        let val = with_task(
            id,
            "nasync_gather",
            span,
            E3476_NASYNC_TASK_NOT_FOUND,
            "async task cancelled",
            async_error_factory(span),
            |state| Ok(task_result(state, span)),
        )?;
        let is_err = matches!(&*val.borrow(), Value::Error(_));
        if is_err {
            return Ok(val);
        }
        results.push(val);
    }
    Ok(Value::Array(results).ref_cell())
}

// >>> nasync.gather_exceptions([t1, t2])
// => [r1, r2]
fn nasync_gather_exceptions(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nasync_gather_exceptions", span)?;
    let ids = task_ids_arg(args, 0, "nasync_gather_exceptions", span)?;
    task_wait_all(&ids);
    let mut results = Vec::with_capacity(ids.len());
    for id in ids {
        let val = with_task(
            id,
            "nasync_gather_exceptions",
            span,
            E3476_NASYNC_TASK_NOT_FOUND,
            "async task cancelled",
            async_error_factory(span),
            |state| Ok(task_result(state, span)),
        )?;
        results.push(val);
    }
    Ok(Value::Array(results).ref_cell())
}

// >>> nasync.race([t1, t2])
// => {index: 0, task: t1, value: ...}
fn nasync_race(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nasync_race", span)?;
    let ids = task_ids_arg(args, 0, "nasync_race", span)?;
    if ids.is_empty() {
        return Ok(nasync_err(span, "race() expects a non-empty task array"));
    }
    let idx = task_wait_any(&ids).ok_or_else(|| {
        RuntimeError::at(span, E3474_NASYNC_ERROR, "race() found no completable tasks")
    })?;
    let id = ids[idx];
    let value = with_task(
        id,
        "nasync_race",
        span,
        E3476_NASYNC_TASK_NOT_FOUND,
        "async task cancelled",
        async_error_factory(span),
        |state| Ok(task_result(state, span)),
    )?;
    let mut map = HashMap::new();
    map.insert("index".to_string(), ok_int(idx as i64));
    map.insert("task".to_string(), ok_int(id as i64));
    map.insert("value".to_string(), value);
    Ok(Value::Object(map).ref_cell())
}

// >>> nasync.wait_any([t1, t2])
fn nasync_wait_any(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    nasync_race(args, span)
}

// >>> nasync.as_completed([t1, t2, t3])
// => [v1, v2, v3] in completion order
fn nasync_as_completed(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nasync_as_completed", span)?;
    let ids = task_ids_arg(args, 0, "nasync_as_completed", span)?;
    if ids.is_empty() {
        return Ok(Value::Array(vec![]).ref_cell());
    }
    let mut remaining = ids;
    let mut results = Vec::with_capacity(remaining.len());
    while !remaining.is_empty() {
        let idx = task_wait_any(&remaining).ok_or_else(|| {
            RuntimeError::at(
                span,
                E3474_NASYNC_ERROR,
                "as_completed() found no completable tasks",
            )
        })?;
        let id = remaining.remove(idx);
        let val = with_task(
            id,
            "nasync_as_completed",
            span,
            E3476_NASYNC_TASK_NOT_FOUND,
            "async task cancelled",
            async_error_factory(span),
            |state| Ok(task_result(state, span)),
        )?;
        results.push(val);
    }
    Ok(Value::Array(results).ref_cell())
}

// >>> nasync.cancel_all([t1, t2])
// => 2
fn nasync_cancel_all(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nasync_cancel_all", span)?;
    let ids = task_ids_arg(args, 0, "nasync_cancel_all", span)?;
    let mut count = 0i64;
    for id in ids {
        if cancel_task(id, span, E3476_NASYNC_TASK_NOT_FOUND)? {
            count += 1;
        }
    }
    Ok(ok_int(count))
}

// >>> nasync.spawn_all([fn() { return 1 }, fn() { return 2 }])
// => [task1, task2]
fn nasync_spawn_all(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nasync_spawn_all", span)?;
    let limit = if args.len() == 2 {
        let n = int_arg(args, 1, "nasync_spawn_all", span)?;
        if n <= 0 {
            return Ok(nasync_err(span, "spawn_all() limit must be positive"));
        }
        Some(n as usize)
    } else {
        None
    };
    match &*args[0].borrow() {
        Value::Array(items) => {
            let mut task_ids = Vec::with_capacity(items.len());
            let mut in_flight: Vec<u64> = Vec::new();
            for item in items {
                let callee = match &*item.borrow() {
                    Value::Function(_) | Value::NativeFunction(_) => Rc::clone(item),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "spawn_all() expects an array of callables, got {}",
                                other.type_name()
                            ),
                        ));
                    }
                };
                if let Some(cap) = limit {
                    while in_flight.len() >= cap {
                        if let Some(idx) = task_wait_any(&in_flight) {
                            in_flight.remove(idx);
                        } else {
                            break;
                        }
                    }
                }
                let id = spawn_callable(callee, vec![], span);
                in_flight.push(id);
                task_ids.push(ok_int(id as i64));
            }
            if limit.is_some() {
                task_wait_all(&in_flight);
            }
            Ok(Value::Array(task_ids).ref_cell())
        }
        other => Err(type_err(
            span,
            format!(
                "spawn_all() expects an array of callables as argument 1, got {}",
                other.type_name()
            ),
        )),
    }
}

// ---------------------------------------------------------------------------
// Timeouts
// ---------------------------------------------------------------------------

// >>> nasync.wait_timeout(<task>, 1000)
// => {timed_out: false, done: true, value: ...}
fn nasync_wait_timeout(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nasync_wait_timeout", span)?;
    let id = task_arg(args, 0, "nasync_wait_timeout", span)?;
    let ms = int_arg(args, 1, "nasync_wait_timeout", span)?;
    if ms < 0 {
        return Ok(nasync_err(span, "wait_timeout() timeout must be non-negative"));
    }
    let timed_out = task_wait_timeout(id, ms as u64);
    let done = with_task(
        id,
        "nasync_wait_timeout",
        span,
        E3476_NASYNC_TASK_NOT_FOUND,
        "",
        async_error_factory(span),
        |state| Ok(ok_bool(task_done(state))),
    )?;
    let value = if matches!(&*done.borrow(), Value::Bool(true)) {
        with_task(
            id,
            "nasync_wait_timeout",
            span,
            E3476_NASYNC_TASK_NOT_FOUND,
            "async task cancelled",
            async_error_factory(span),
            |state| Ok(task_result(state, span)),
        )?
    } else {
        ok_nil()
    };
    let mut map = HashMap::new();
    map.insert("timed_out".to_string(), ok_bool(timed_out));
    map.insert("done".to_string(), done);
    map.insert("value".to_string(), value);
    Ok(Value::Object(map).ref_cell())
}

// >>> nasync.timeout(fn() { return 7 }, 5000)
// => {timed_out: false, value: 7}
fn nasync_timeout(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 16, "nasync_timeout", span)?;
    let ms = int_arg(args, 1, "nasync_timeout", span)?;
    if ms < 0 {
        return Ok(nasync_err(span, "timeout() timeout must be non-negative"));
    }
    let callee = function_arg(args, 0, "nasync_timeout", span)?;
    let sendable = sendable_args_rest(args, 2, "nasync_timeout", span)?;
    let id = spawn_callable(callee, sendable, span);
    let timed_out = task_wait_timeout(id, ms as u64);
    let value = with_task(
        id,
        "nasync_timeout",
        span,
        E3476_NASYNC_TASK_NOT_FOUND,
        "async task cancelled",
        async_error_factory(span),
        |state| {
            if timed_out && !task_done(state) {
                let _ = cancel_task(id, span, E3476_NASYNC_TASK_NOT_FOUND);
                Ok(error_value(
                    E3477_NASYNC_TIMEOUT,
                    "nasync_timeout",
                    format!("timeout() exceeded {ms} ms"),
                    span,
                ))
            } else {
                Ok(task_result(state, span))
            }
        },
    )?;
    let mut map = HashMap::new();
    map.insert("timed_out".to_string(), ok_bool(timed_out));
    map.insert("value".to_string(), value);
    Ok(Value::Object(map).ref_cell())
}

// ---------------------------------------------------------------------------
// Async channels
// ---------------------------------------------------------------------------

enum ChannelKind {
    Unbounded {
        tx: mpsc::Sender<AsyncValue>,
        rx: Arc<Mutex<mpsc::Receiver<AsyncValue>>>,
    },
    Bounded {
        tx: mpsc::SyncSender<AsyncValue>,
        rx: Arc<Mutex<mpsc::Receiver<AsyncValue>>>,
        capacity: usize,
    },
}

struct AsyncChannel {
    kind: ChannelKind,
    closed: Cell<bool>,
    queued: Cell<usize>,
}

thread_local! {
    static CHANNELS: RefCell<HashMap<u64, AsyncChannel>> = RefCell::new(HashMap::new());
    static NEXT_CHANNEL: Cell<u64> = const { Cell::new(1) };
}

fn alloc_channel(capacity: Option<usize>) -> u64 {
    let id = NEXT_CHANNEL.with(|n| {
        let id = n.get();
        n.set(id + 1);
        id
    });
    let kind = if let Some(cap) = capacity {
        let (tx, rx) = mpsc::sync_channel(cap);
        ChannelKind::Bounded {
            tx,
            rx: Arc::new(Mutex::new(rx)),
            capacity: cap,
        }
    } else {
        let (tx, rx) = mpsc::channel();
        ChannelKind::Unbounded {
            tx,
            rx: Arc::new(Mutex::new(rx)),
        }
    };
    CHANNELS.with(|m| {
        m.borrow_mut().insert(
            id,
            AsyncChannel {
                kind,
                closed: Cell::new(false),
                queued: Cell::new(0),
            },
        );
    });
    id
}

fn with_channel<F>(id: u64, name: &str, span: Span, f: F) -> NiaoResult<ValueRef>
where
    F: FnOnce(&AsyncChannel) -> ValueRef,
{
    CHANNELS.with(|m| {
        let guard = m.borrow();
        match guard.get(&id) {
            Some(ch) => Ok(f(ch)),
            None => Ok(error_value(
                E3478_NASYNC_INVALID_HANDLE,
                "nasync_error",
                format!("{name}(): invalid channel handle {id}"),
                span,
            )),
        }
    })
}

fn channel_send_value(ch: &AsyncChannel, val: AsyncValue, span: Span) -> ValueRef {
    if ch.closed.get() {
        return nasync_err(span, "channel is closed");
    }
    let ok = match &ch.kind {
        ChannelKind::Unbounded { tx, .. } => tx.send(val).is_ok(),
        ChannelKind::Bounded { tx, .. } => tx.send(val).is_ok(),
    };
    if ok {
        ch.queued.set(ch.queued.get() + 1);
        ok_nil()
    } else {
        ch.closed.set(true);
        nasync_err(span, "channel is closed")
    }
}

fn value_to_channel_async(args: &[ValueRef], idx: usize, name: &str, span: Span) -> Result<AsyncValue, RuntimeError> {
    value_to_sendable(&args[idx].borrow())
        .map(|s| match s {
            crate::parallel::SendableValue::Nil => AsyncValue::nil(),
            crate::parallel::SendableValue::Int(n) => AsyncValue::int(n),
            crate::parallel::SendableValue::Bool(b) => AsyncValue::Bool(b),
            crate::parallel::SendableValue::Float(f) => AsyncValue::Float(f),
            crate::parallel::SendableValue::String(s) => AsyncValue::String(s),
            crate::parallel::SendableValue::IntArray(v) => AsyncValue::IntArray(v),
            crate::parallel::SendableValue::FloatArray(_) => {
                AsyncValue::String("float arrays not supported on async channels".into())
            }
            crate::parallel::SendableValue::BoolArray(_) => {
                AsyncValue::String("bool arrays not supported on async channels".into())
            }
            crate::parallel::SendableValue::ByteArray(v) => AsyncValue::ByteArray(v),
            crate::parallel::SendableValue::StringArray(v) => AsyncValue::String(v.join(",")),
            crate::parallel::SendableValue::Array(items) => AsyncValue::Array(
                items
                    .into_iter()
                    .map(|v| match v {
                        crate::parallel::SendableValue::Nil => AsyncValue::nil(),
                        crate::parallel::SendableValue::Int(n) => AsyncValue::int(n),
                        crate::parallel::SendableValue::Bool(b) => AsyncValue::Bool(b),
                        crate::parallel::SendableValue::Float(f) => AsyncValue::Float(f),
                        crate::parallel::SendableValue::String(s) => AsyncValue::String(s),
                        other => AsyncValue::String(format!("{other:?}")),
                    })
                    .collect(),
            ),
            crate::parallel::SendableValue::Object(map) => {
                let mut out = HashMap::new();
                for (k, v) in map {
                    out.insert(
                        k,
                        match v {
                            crate::parallel::SendableValue::Int(n) => AsyncValue::int(n),
                            crate::parallel::SendableValue::String(s) => AsyncValue::String(s),
                            other => AsyncValue::String(format!("{other:?}")),
                        },
                    );
                }
                AsyncValue::Object(out)
            }
        })
        .map_err(|e| type_err(span, format!("{name}() argument {} is not sendable: {e}", idx + 1)))
}

// >>> nasync.channel()
// => <handle>
fn nasync_channel(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nasync_channel", span)?;
    let capacity = if args.is_empty() {
        None
    } else {
        let n = int_arg(args, 0, "nasync_channel", span)?;
        if n <= 0 {
            return Ok(nasync_err(span, "channel() capacity must be positive"));
        }
        Some(n as usize)
    };
    Ok(ok_int(alloc_channel(capacity) as i64))
}

// >>> let ch = nasync.channel(); nasync.channel_send(ch, 1)
fn nasync_channel_send(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nasync_channel_send", span)?;
    let id = handle_arg(args, 0, "nasync_channel_send", span)?;
    let val = value_to_channel_async(args, 1, "nasync_channel_send", span)?;
    Ok(with_channel(id, "nasync_channel_send", span, |ch| {
        channel_send_value(ch, val, span)
    })?)
}

// >>> nasync.channel_recv(ch)
fn nasync_channel_recv(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nasync_channel_recv", span)?;
    let id = handle_arg(args, 0, "nasync_channel_recv", span)?;
    Ok(with_channel(id, "nasync_channel_recv", span, |ch| {
        let rx = match &ch.kind {
            ChannelKind::Unbounded { rx, .. } | ChannelKind::Bounded { rx, .. } => Arc::clone(rx),
        };
        let guard = rx.lock().expect("channel receiver poisoned");
        match guard.recv() {
            Ok(v) => {
                ch.queued.set(ch.queued.get().saturating_sub(1));
                v.to_value().ref_cell()
            }
            Err(_) => {
                ch.closed.set(true);
                nasync_err(span, "channel is closed")
            }
        }
    })?)
}

// >>> nasync.channel_try_recv(ch)
fn nasync_channel_try_recv(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nasync_channel_try_recv", span)?;
    let id = handle_arg(args, 0, "nasync_channel_try_recv", span)?;
    Ok(with_channel(id, "nasync_channel_try_recv", span, |ch| {
        let rx = match &ch.kind {
            ChannelKind::Unbounded { rx, .. } | ChannelKind::Bounded { rx, .. } => Arc::clone(rx),
        };
        let guard = rx.lock().expect("channel receiver poisoned");
        match guard.try_recv() {
            Ok(v) => {
                ch.queued.set(ch.queued.get().saturating_sub(1));
                v.to_value().ref_cell()
            }
            Err(TryRecvError::Empty) => ok_nil(),
            Err(TryRecvError::Disconnected) => {
                ch.closed.set(true);
                nasync_err(span, "channel is closed")
            }
        }
    })?)
}

// >>> nasync.channel_recv_timeout(ch, 100)
fn nasync_channel_recv_timeout(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nasync_channel_recv_timeout", span)?;
    let id = handle_arg(args, 0, "nasync_channel_recv_timeout", span)?;
    let ms = int_arg(args, 1, "nasync_channel_recv_timeout", span)?;
    if ms < 0 {
        return Ok(nasync_err(
            span,
            "channel_recv_timeout() timeout must be non-negative",
        ));
    }
    Ok(with_channel(id, "nasync_channel_recv_timeout", span, |ch| {
        let rx = match &ch.kind {
            ChannelKind::Unbounded { rx, .. } | ChannelKind::Bounded { rx, .. } => Arc::clone(rx),
        };
        let guard = rx.lock().expect("channel receiver poisoned");
        match guard.recv_timeout(Duration::from_millis(ms as u64)) {
            Ok(v) => {
                ch.queued.set(ch.queued.get().saturating_sub(1));
                v.to_value().ref_cell()
            }
            Err(RecvTimeoutError::Timeout) => ok_nil(),
            Err(RecvTimeoutError::Disconnected) => {
                ch.closed.set(true);
                nasync_err(span, "channel is closed")
            }
        }
    })?)
}

// >>> nasync.channel_close(ch)
fn nasync_channel_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nasync_channel_close", span)?;
    let id = handle_arg(args, 0, "nasync_channel_close", span)?;
    CHANNELS.with(|m| {
        if let Some(ch) = m.borrow_mut().remove(&id) {
            ch.closed.set(true);
            Ok(ok_nil())
        } else {
            Ok(error_value(
                E3478_NASYNC_INVALID_HANDLE,
                "nasync_error",
                format!("nasync_channel_close(): invalid channel handle {id}"),
                span,
            ))
        }
    })
}

// >>> nasync.channel_is_closed(ch)
fn nasync_channel_is_closed(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nasync_channel_is_closed", span)?;
    let id = handle_arg(args, 0, "nasync_channel_is_closed", span)?;
    Ok(with_channel(id, "nasync_channel_is_closed", span, |ch| {
        ok_bool(ch.closed.get())
    })?)
}

// >>> nasync.channel_len(ch)
fn nasync_channel_len(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nasync_channel_len", span)?;
    let id = handle_arg(args, 0, "nasync_channel_len", span)?;
    Ok(with_channel(id, "nasync_channel_len", span, |ch| ok_int(ch.queued.get() as i64))?)
}

// ---------------------------------------------------------------------------
// Task groups
// ---------------------------------------------------------------------------

struct TaskGroup {
    tasks: Vec<u64>,
}

thread_local! {
    static GROUPS: RefCell<HashMap<u64, TaskGroup>> = RefCell::new(HashMap::new());
    static NEXT_GROUP: Cell<u64> = const { Cell::new(1) };
}

fn alloc_group() -> u64 {
    NEXT_GROUP.with(|n| {
        let id = n.get();
        n.set(id + 1);
        id
    })
}

// >>> nasync.group()
fn nasync_group(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nasync_group", span)?;
    let id = alloc_group();
    GROUPS.with(|g| {
        g.borrow_mut().insert(id, TaskGroup { tasks: vec![] });
    });
    Ok(ok_int(id as i64))
}

// >>> nasync.group_spawn(g, fn() { return 1 })
fn nasync_group_spawn(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_min(args, 2, "nasync_group_spawn", span)?;
    let group_id = handle_arg(args, 0, "nasync_group_spawn", span)?;
    let callee = function_arg(args, 1, "nasync_group_spawn", span)?;
    let sendable = sendable_args_rest(args, 2, "nasync_group_spawn", span)?;
    let task_id = spawn_callable(callee, sendable, span);
    GROUPS.with(|g| {
        if let Some(group) = g.borrow_mut().get_mut(&group_id) {
            group.tasks.push(task_id);
            Ok(ok_int(task_id as i64))
        } else {
            Ok(error_value(
                E3478_NASYNC_INVALID_HANDLE,
                "nasync_error",
                format!("nasync_group_spawn(): invalid group handle {group_id}"),
                span,
            ))
        }
    })
}

// >>> nasync.group_wait(g)
fn nasync_group_wait(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nasync_group_wait", span)?;
    let group_id = handle_arg(args, 0, "nasync_group_wait", span)?;
    let ids = GROUPS.with(|g| {
        g.borrow()
            .get(&group_id)
            .map(|gr| gr.tasks.clone())
            .ok_or_else(|| {
                RuntimeError::at(
                    span,
                    E3478_NASYNC_INVALID_HANDLE,
                    format!("nasync_group_wait(): invalid group handle {group_id}"),
                )
            })
    })?;
    task_wait_all(&ids);
    let mut results = Vec::with_capacity(ids.len());
    for id in ids {
        let val = with_task(
            id,
            "nasync_group_wait",
            span,
            E3476_NASYNC_TASK_NOT_FOUND,
            "async task cancelled",
            async_error_factory(span),
            |state| Ok(task_result(state, span)),
        )?;
        let is_err = matches!(&*val.borrow(), Value::Error(_));
        if is_err {
            for tid in GROUPS.with(|g| {
                g.borrow()
                    .get(&group_id)
                    .map(|gr| gr.tasks.clone())
                    .unwrap_or_default()
            }) {
                let _ = cancel_task(tid, span, E3476_NASYNC_TASK_NOT_FOUND);
            }
            return Ok(val);
        }
        results.push(val);
    }
    Ok(Value::Array(results).ref_cell())
}

// >>> nasync.group_cancel(g)
fn nasync_group_cancel(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nasync_group_cancel", span)?;
    let group_id = handle_arg(args, 0, "nasync_group_cancel", span)?;
    let ids = GROUPS.with(|g| {
        g.borrow()
            .get(&group_id)
            .map(|gr| gr.tasks.clone())
            .ok_or_else(|| {
                RuntimeError::at(
                    span,
                    E3478_NASYNC_INVALID_HANDLE,
                    format!("nasync_group_cancel(): invalid group handle {group_id}"),
                )
            })
    })?;
    let mut count = 0i64;
    for id in ids {
        if cancel_task(id, span, E3476_NASYNC_TASK_NOT_FOUND)? {
            count += 1;
        }
    }
    Ok(ok_int(count))
}

// ---------------------------------------------------------------------------
// Module exports
// ---------------------------------------------------------------------------

macro_rules! nasync_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nasync_fns![
    ("nasync_spawn", "spawn", nasync_spawn),
    ("nasync_create_task", "create_task", nasync_create_task),
    ("nasync_sleep_async", "sleep_async", nasync_sleep_async),
    ("nasync_sleep", "sleep", nasync_sleep),
    ("nasync_done", "done", nasync_done),
    ("nasync_poll", "poll", nasync_poll),
    ("nasync_wait", "wait", nasync_wait),
    ("nasync_result", "result", nasync_result),
    ("nasync_cancel", "cancel", nasync_cancel),
    ("nasync_shield", "shield", nasync_shield),
    ("nasync_status", "status", nasync_status),
    ("nasync_gather", "gather", nasync_gather),
    ("nasync_gather_exceptions", "gather_exceptions", nasync_gather_exceptions),
    ("nasync_race", "race", nasync_race),
    ("nasync_wait_any", "wait_any", nasync_wait_any),
    ("nasync_as_completed", "as_completed", nasync_as_completed),
    ("nasync_cancel_all", "cancel_all", nasync_cancel_all),
    ("nasync_spawn_all", "spawn_all", nasync_spawn_all),
    ("nasync_wait_timeout", "wait_timeout", nasync_wait_timeout),
    ("nasync_timeout", "timeout", nasync_timeout),
    ("nasync_channel", "channel", nasync_channel),
    ("nasync_channel_send", "channel_send", nasync_channel_send),
    ("nasync_channel_recv", "channel_recv", nasync_channel_recv),
    ("nasync_channel_try_recv", "channel_try_recv", nasync_channel_try_recv),
    ("nasync_channel_recv_timeout", "channel_recv_timeout", nasync_channel_recv_timeout),
    ("nasync_channel_close", "channel_close", nasync_channel_close),
    ("nasync_channel_is_closed", "channel_is_closed", nasync_channel_is_closed),
    ("nasync_channel_len", "channel_len", nasync_channel_len),
    ("nasync_group", "group", nasync_group),
    ("nasync_group_spawn", "group_spawn", nasync_group_spawn),
    ("nasync_group_wait", "group_wait", nasync_group_wait),
    ("nasync_group_cancel", "group_cancel", nasync_group_cancel),
];

pub const MODULE_NAME: &str = "nasync";
pub const MODULE_PATHS: &[&str] = &["nasync", "std/nasync"];

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

    fn i(v: i64) -> ValueRef {
        Value::Int(v).ref_cell()
    }

    fn native_ret(args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
        match &*args[0].borrow() {
            Value::Int(n) => Ok(Value::Int(*n * 2).ref_cell()),
            _ => Ok(Value::Nil.ref_cell()),
        }
    }

    #[test]
    fn spawn_wait_native() {
        let t = nasync_spawn(
            &[Value::NativeFunction(Rc::new(native_ret)).ref_cell(), i(21)],
            span(),
        )
        .unwrap();
        let id = match &*t.borrow() {
            Value::Int(n) => *n as u64,
            other => panic!("expected task id, got {other:?}"),
        };
        let out = nasync_wait(&[i(id as i64)], span()).unwrap();
        assert!(matches!(&*out.borrow(), Value::Int(42)));
    }

    #[test]
    fn gather_two_tasks() {
        fn ret(args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
            match &*args[0].borrow() {
                Value::Int(n) => Ok(Value::Int(*n).ref_cell()),
                _ => Ok(Value::Nil.ref_cell()),
            }
        }
        let f = Value::NativeFunction(Rc::new(ret)).ref_cell();
        let t1 = nasync_spawn(&[f.clone(), i(1)], span()).unwrap();
        let t2 = nasync_spawn(&[f, i(2)], span()).unwrap();
        let out = nasync_gather(&[Value::Array(vec![t1, t2]).ref_cell()], span()).unwrap();
        match &*out.borrow() {
            Value::Array(items) => {
                assert_eq!(items.len(), 2);
                assert!(matches!(&*items[0].borrow(), Value::Int(1)));
                assert!(matches!(&*items[1].borrow(), Value::Int(2)));
            }
            other => panic!("expected array, got {other:?}"),
        };
    }

    #[test]
    fn channel_send_recv() {
        let ch = nasync_channel(&[], span()).unwrap();
        let _ = nasync_channel_send(&[ch.clone(), i(99)], span()).unwrap();
        let v = nasync_channel_recv(&[ch], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Int(99)));
    }

    #[test]
    fn cancel_pending_task() {
        let t = nasync_sleep_async(&[i(60_000)], span()).unwrap();
        let id = match &*t.borrow() {
            Value::Int(n) => *n,
            _ => panic!("expected int"),
        };
        let cancelled = nasync_cancel(&[i(id)], span()).unwrap();
        assert!(matches!(&*cancelled.borrow(), Value::Bool(true)));
    }

    #[test]
    fn wait_timeout_times_out() {
        let t = nasync_sleep_async(&[i(60_000)], span()).unwrap();
        let id = match &*t.borrow() {
            Value::Int(n) => *n,
            _ => panic!("expected int"),
        };
        let out = nasync_wait_timeout(&[i(id), i(10)], span()).unwrap();
        match &*out.borrow() {
            Value::Object(map) => {
                assert!(matches!(&*map["timed_out"].borrow(), Value::Bool(true)));
            }
            other => panic!("expected object, got {other:?}"),
        };
    }

    #[test]
    fn bench_spawn_gather() {
        use std::time::Instant;
        fn ret(args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
            match &*args[0].borrow() {
                Value::Int(n) => Ok(Value::Int(*n).ref_cell()),
                _ => Ok(Value::Nil.ref_cell()),
            }
        }
        let f = Value::NativeFunction(Rc::new(ret)).ref_cell();
        let start = Instant::now();
        let mut ids = Vec::new();
        for n in 0..200 {
            let t = nasync_spawn(&[f.clone(), i(n)], span()).unwrap();
            let id = match &*t.borrow() {
                Value::Int(v) => *v,
                _ => panic!("expected task id"),
            };
            ids.push(id);
        }
        let _ = nasync_gather(&[Value::Array(ids.iter().map(|id| i(*id)).collect()).ref_cell()], span())
            .unwrap();
        eprintln!(
            "nasync bench spawn+gather 200 tasks: {:.2} ms",
            start.elapsed().as_secs_f64() * 1000.0
        );
    }

    #[test]
    fn bench_channel_throughput() {
        use std::time::Instant;
        let ch = nasync_channel(&[], span()).unwrap();
        let start = Instant::now();
        for n in 0..50_000 {
            let _ = nasync_channel_send(&[ch.clone(), i(n)], span()).unwrap();
            let _ = nasync_channel_recv(&[ch.clone()], span()).unwrap();
        }
        eprintln!(
            "nasync bench channel 50k send/recv: {:.2} ms",
            start.elapsed().as_secs_f64() * 1000.0
        );
    }
}
