//! Native ntrace standard library — distributed tracing spans with W3C
//! `traceparent`, events, JSON export, and thread-local handle registry.
//!
//! Import with `import "ntrace"` (or `import "std/ntrace"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

// Wired in codes.rs by central integration.
const E3180_NTRACE_ARITY: u32 = 3180;
const E3181_NTRACE_ERROR: u32 = 3181;
const E3182_NTRACE_TYPE: u32 = 3182;
const E3183_NTRACE_INVALID_HANDLE: u32 = 3183;

static TRACE_SEQ: AtomicU64 = AtomicU64::new(1);

// ---------------------------------------------------------------------------
// Span model
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct TraceEvent {
    name: String,
    attrs: HashMap<String, ValueRef>,
    t_ms: i64,
}

struct SpanRecord {
    name: String,
    trace_id: String,
    span_id: String,
    parent_span_id: Option<String>,
    start: Instant,
    start_ms: i64,
    end_ms: Option<i64>,
    duration_ms: Option<f64>,
    events: Vec<TraceEvent>,
}

thread_local! {
    static SPANS: RefCell<HashMap<i64, SpanRecord>> = RefCell::new(HashMap::new());
    static FINISHED: RefCell<Vec<SpanRecord>> = const { RefCell::new(Vec::new()) };
    static NEXT_HANDLE: RefCell<i64> = const { RefCell::new(1) };
    static CURRENT: RefCell<Option<i64>> = const { RefCell::new(None) };
}

#[inline]
fn new_handle() -> i64 {
    NEXT_HANDLE.with(|h| {
        let mut h = h.borrow_mut();
        let id = *h;
        *h += 1;
        id
    })
}

#[inline]
fn wall_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[inline]
fn random_hex(n_bytes: usize) -> String {
    let mut out = String::with_capacity(n_bytes * 2);
    for _ in 0..n_bytes {
        let x = TRACE_SEQ.fetch_add(0x9E3779B97F4A7C15, Ordering::Relaxed);
        out.push_str(&format!("{:02x}", (x & 0xFF) as u8));
        out.push_str(&format!("{:02x}", ((x >> 8) & 0xFF) as u8));
    }
    out.truncate(n_bytes * 2);
    out
}

#[inline]
fn traceparent(trace_id: &str, span_id: &str, sampled: bool) -> String {
    let flags = if sampled { "01" } else { "00" };
    format!("00-{trace_id}-{span_id}-{flags}")
}

fn with_span<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut SpanRecord) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    SPANS.with(|spans| {
        let mut spans = spans.borrow_mut();
        match spans.get_mut(&id) {
            Some(s) => Ok(Ok(f(s))),
            None => Ok(Err(error_value(
                E3183_NTRACE_INVALID_HANDLE,
                "ntrace_error",
                format!("invalid or closed span handle {id}"),
                span,
            ))),
        }
    })
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3180_NTRACE_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3180_NTRACE_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3182_NTRACE_TYPE, msg.into())
}

fn trace_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3181_NTRACE_ERROR, "ntrace_error", msg.into(), span)
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

fn optional_object(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> NiaoResult<HashMap<String, ValueRef>> {
    if args.len() <= idx {
        return Ok(HashMap::new());
    }
    match &*args[idx].borrow() {
        Value::Object(map) => Ok(map.clone()),
        Value::Nil => Ok(HashMap::new()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects object or nil as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn clone_object(map: &HashMap<String, ValueRef>) -> HashMap<String, ValueRef> {
    map.iter().map(|(k, v)| (k.clone(), Rc::clone(v))).collect()
}

fn span_to_object(rec: &SpanRecord) -> HashMap<String, ValueRef> {
    let mut map = HashMap::new();
    map.insert("name".to_string(), Value::String(rec.name.clone()).ref_cell());
    map.insert("trace_id".to_string(), Value::String(rec.trace_id.clone()).ref_cell());
    map.insert("span_id".to_string(), Value::String(rec.span_id.clone()).ref_cell());
    if let Some(ref p) = rec.parent_span_id {
        map.insert("parent_span_id".to_string(), Value::String(p.clone()).ref_cell());
    } else {
        map.insert("parent_span_id".to_string(), Value::Nil.ref_cell());
    }
    map.insert("start_ms".to_string(), Value::Int(rec.start_ms).ref_cell());
    if let Some(end) = rec.end_ms {
        map.insert("end_ms".to_string(), Value::Int(end).ref_cell());
    }
    if let Some(ms) = rec.duration_ms {
        map.insert("duration_ms".to_string(), Value::Float(ms).ref_cell());
    }
    map.insert(
        "traceparent".to_string(),
        Value::String(traceparent(&rec.trace_id, &rec.span_id, true)).ref_cell(),
    );
    let events: Vec<ValueRef> = rec
        .events
        .iter()
        .map(|e| {
            let mut em = HashMap::new();
            em.insert("name".to_string(), Value::String(e.name.clone()).ref_cell());
            em.insert("t_ms".to_string(), Value::Int(e.t_ms).ref_cell());
            em.insert("attrs".to_string(), Value::Object(clone_object(&e.attrs)).ref_cell());
            Value::Object(em).ref_cell()
        })
        .collect();
    map.insert("events".to_string(), Value::Array(events).ref_cell());
    map
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn ntrace_start(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ntrace_start", span)?;
    let name = string_arg(args, 0, "ntrace_start", span)?;
    let parent = if args.len() == 2 {
        Some(int_arg(args, 1, "ntrace_start", span)?)
    } else {
        CURRENT.with(|c| *c.borrow())
    };

    let (trace_id, parent_span_id) = if let Some(pid) = parent {
        let parent_info = SPANS.with(|spans| {
            spans
                .borrow()
                .get(&pid)
                .map(|p| (p.trace_id.clone(), Some(p.span_id.clone())))
        });
        match parent_info {
            Some((tid, sid)) => (tid, sid),
            None => return Ok(trace_err(span, format!("invalid parent span handle {pid}"))),
        }
    } else {
        (random_hex(16), None)
    };

    let span_id = random_hex(8);
    let id = new_handle();
    let record = SpanRecord {
        name,
        trace_id,
        span_id,
        parent_span_id,
        start: Instant::now(),
        start_ms: wall_now_ms(),
        end_ms: None,
        duration_ms: None,
        events: Vec::new(),
    };
    SPANS.with(|spans| {
        spans.borrow_mut().insert(id, record);
    });
    CURRENT.with(|c| *c.borrow_mut() = Some(id));
    Ok(Value::Int(id).ref_cell())
}

fn ntrace_end(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntrace_end", span)?;
    let id = int_arg(args, 0, "ntrace_end", span)?;
    let removed = SPANS.with(|spans| spans.borrow_mut().remove(&id));
    match removed {
        Some(mut rec) => {
            let ms = rec.start.elapsed().as_secs_f64() * 1000.0;
            rec.end_ms = Some(wall_now_ms());
            rec.duration_ms = Some(ms);
            CURRENT.with(|c| {
                if *c.borrow() == Some(id) {
                    *c.borrow_mut() = None;
                }
            });
            let obj = span_to_object(&rec);
            FINISHED.with(|f| f.borrow_mut().push(rec));
            Ok(Value::Object(obj).ref_cell())
        }
        None => Ok(error_value(
            E3183_NTRACE_INVALID_HANDLE,
            "ntrace_error",
            format!("invalid or closed span handle {id}"),
            span,
        )),
    }
}

fn ntrace_event(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ntrace_event", span)?;
    let name = string_arg(args, 0, "ntrace_event", span)?;
    let attrs = optional_object(args, 1, "ntrace_event", span)?;
    let target = CURRENT.with(|c| *c.borrow());
    let Some(id) = target else {
        return Ok(trace_err(span, "ntrace_event() called with no active span"));
    };
    match with_span(id, span, |rec| {
        rec.events.push(TraceEvent {
            name,
            attrs,
            t_ms: wall_now_ms(),
        });
    })? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn ntrace_traceparent(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "ntrace_traceparent", span)?;
    let id = if args.is_empty() {
        CURRENT.with(|c| *c.borrow())
    } else {
        Some(int_arg(args, 0, "ntrace_traceparent", span)?)
    };
    let Some(id) = id else {
        return Ok(trace_err(span, "no active span for traceparent"));
    };
    match with_span(id, span, |rec| {
        traceparent(&rec.trace_id, &rec.span_id, true)
    })? {
        Ok(tp) => Ok(Value::String(tp).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn ntrace_current(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ntrace_current", span)?;
    let cur = CURRENT.with(|c| *c.borrow());
    match cur {
        Some(id) => Ok(Value::Int(id).ref_cell()),
        None => Ok(Value::Nil.ref_cell()),
    }
}

fn ntrace_export(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "ntrace_export", span)?;
    let as_json = args.len() == 1 && matches!(&*args[0].borrow(), Value::Bool(true));
    let active: Vec<HashMap<String, ValueRef>> = SPANS.with(|spans| {
        spans
            .borrow()
            .values()
            .map(span_to_object)
            .collect()
    });
    let finished: Vec<HashMap<String, ValueRef>> =
        FINISHED.with(|f| f.borrow().iter().map(span_to_object).collect());
    let mut root = HashMap::new();
    let active_arr: Vec<ValueRef> = active
        .into_iter()
        .map(|m| Value::Object(m).ref_cell())
        .collect();
    let finished_arr: Vec<ValueRef> = finished
        .into_iter()
        .map(|m| Value::Object(m).ref_cell())
        .collect();
    root.insert("active".to_string(), Value::Array(active_arr).ref_cell());
    root.insert("finished".to_string(), Value::Array(finished_arr).ref_cell());
    if as_json {
        Ok(Value::String(export_json_string(&root)).ref_cell())
    } else {
        Ok(Value::Object(root).ref_cell())
    }
}

fn export_json_string(root: &HashMap<String, ValueRef>) -> String {
    serde_json::to_string(&value_to_json_value(&Value::Object(root.clone()))).unwrap_or_else(|_| "{}".into())
}

fn value_to_json_value(v: &Value) -> serde_json::Value {
    match v {
        Value::Nil => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Int(n) => serde_json::Value::Number((*n).into()),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(|i| value_to_json_value(&i.borrow())).collect())
        }
        Value::Object(map) => {
            let mut obj = serde_json::Map::new();
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for k in keys {
                obj.insert(k.clone(), value_to_json_value(&map[k].borrow()));
            }
            serde_json::Value::Object(obj)
        }
        _ => serde_json::Value::String(v.type_name().to_string()),
    }
}

fn ntrace_clear(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ntrace_clear", span)?;
    SPANS.with(|s| s.borrow_mut().clear());
    FINISHED.with(|f| f.borrow_mut().clear());
    CURRENT.with(|c| *c.borrow_mut() = None);
    Ok(Value::Nil.ref_cell())
}

fn ntrace_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntrace_close", span)?;
    let id = int_arg(args, 0, "ntrace_close", span)?;
    let removed = SPANS.with(|s| s.borrow_mut().remove(&id).is_some());
    if CURRENT.with(|c| *c.borrow() == Some(id)) {
        CURRENT.with(|c| *c.borrow_mut() = None);
    }
    Ok(Value::Bool(removed).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ntrace_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ntrace_fns![
    ("ntrace_start", "start", ntrace_start),
    ("ntrace_end", "end", ntrace_end),
    ("ntrace_event", "event", ntrace_event),
    ("ntrace_traceparent", "traceparent", ntrace_traceparent),
    ("ntrace_current", "current", ntrace_current),
    ("ntrace_export", "export", ntrace_export),
    ("ntrace_clear", "clear", ntrace_clear),
    ("ntrace_close", "close", ntrace_close),
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

pub const MODULE_NAME: &str = "ntrace";
pub const MODULE_PATHS: &[&str] = &["ntrace", "std/ntrace"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::dummy()
    }

    fn int_handle(v: ValueRef) -> i64 {
        match v.borrow().clone() {
            Value::Int(n) => n,
            other => panic!("expected int handle, got {other:?}"),
        }
    }

    #[test]
    fn span_lifecycle_and_traceparent() {
        let _ = ntrace_clear(&[], span());
        let h = int_handle(ntrace_start(&[Value::String("root".into()).ref_cell()], span()).unwrap());

        let tp = match ntrace_traceparent(&[], span()).unwrap().borrow().clone() {
            Value::String(s) => s,
            other => panic!("expected string, got {other:?}"),
        };
        assert!(tp.starts_with("00-"));
        let parts: Vec<&str> = tp.split('-').collect();
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0], "00");
        assert_eq!(parts[1].len(), 32);
        assert_eq!(parts[2].len(), 16);

        ntrace_event(
            &[
                Value::String("hit".into()).ref_cell(),
                Value::Object({
                    let mut m = HashMap::new();
                    m.insert("k".to_string(), Value::Int(1).ref_cell());
                    m
                })
                .ref_cell(),
            ],
            span(),
        )
        .unwrap();

        let child = int_handle(ntrace_start(
            &[
                Value::String("child".into()).ref_cell(),
                Value::Int(h).ref_cell(),
            ],
            span(),
        )
        .unwrap());

        let ended_val = ntrace_end(&[Value::Int(child).ref_cell()], span())
            .unwrap()
            .borrow()
            .clone();
        match ended_val {
            Value::Object(map) => {
                assert!(matches!(&*map["name"].borrow(), Value::String(s) if s == "child"));
                assert!(matches!(&*map["parent_span_id"].borrow(), Value::String(_)));
                assert!(matches!(&*map["duration_ms"].borrow(), Value::Float(_)));
            }
            other => panic!("expected object, got {other:?}"),
        }

        let _ = ntrace_end(&[Value::Int(h).ref_cell()], span()).unwrap();
        let exp_val = ntrace_export(&[], span()).unwrap().borrow().clone();
        match exp_val {
            Value::Object(map) => match &*map["finished"].borrow() {
                Value::Array(items) => assert_eq!(items.len(), 2),
                other => panic!("expected array, got {other:?}"),
            },
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn export_json_string_mode() {
        let _ = ntrace_clear(&[], span());
        let h = int_handle(ntrace_start(&[Value::String("x".into()).ref_cell()], span()).unwrap());
        let json = match ntrace_export(&[Value::Bool(true).ref_cell()], span())
            .unwrap()
            .borrow()
            .clone()
        {
            Value::String(s) => s,
            other => panic!("expected string json, got {other:?}"),
        };
        assert!(json.contains("\"active\""));
        ntrace_close(&[Value::Int(h).ref_cell()], span()).unwrap();
    }

    #[test]
    fn invalid_handle() {
        let v = ntrace_end(&[Value::Int(999_999).ref_cell()], span()).unwrap();
        assert!(matches!(v.borrow().clone(), Value::Error(_)));
    }

    #[test]
    fn independent_handle_ids() {
        let _ = ntrace_clear(&[], span());
        let h1 = int_handle(ntrace_start(&[Value::String("a".into()).ref_cell()], span()).unwrap());
        let h2 = int_handle(ntrace_start(&[Value::String("b".into()).ref_cell()], span()).unwrap());
        assert_ne!(h1, h2);
        ntrace_close(&[Value::Int(h1).ref_cell()], span()).unwrap();
        ntrace_close(&[Value::Int(h2).ref_cell()], span()).unwrap();
    }
}
