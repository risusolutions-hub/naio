//! Native nreplay standard library — deterministic event record / replay sessions.
//!
//! Capture labeled events with relative timestamps, inspect them, and persist
//! to a simple line format (`kind|||stringdata|||t_ms`).
//!
//! Import with `import "nreplay"` (or `import "std/nreplay"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Session model
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Event {
    kind: String,
    data: ValueRef,
    t_ms: i64,
}

struct Session {
    running: bool,
    started_at_ms: i64,
    events: Vec<Event>,
}

impl Session {
    fn new_running() -> Self {
        Session {
            running: true,
            started_at_ms: now_ms(),
            events: Vec::new(),
        }
    }

    fn from_events(events: Vec<Event>) -> Self {
        Session {
            running: false,
            started_at_ms: now_ms(),
            events,
        }
    }

    fn elapsed_ms(&self) -> i64 {
        (now_ms() - self.started_at_ms).max(0)
    }
}

thread_local! {
    static SESSIONS: RefCell<HashMap<i64, Session>> = RefCell::new(HashMap::new());
    static NEXT_HANDLE: RefCell<i64> = const { RefCell::new(1) };
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn new_handle() -> i64 {
    NEXT_HANDLE.with(|h| {
        let mut h = h.borrow_mut();
        let id = *h;
        *h += 1;
        id
    })
}

fn with_session<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut Session) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    SESSIONS.with(|sessions| {
        let mut sessions = sessions.borrow_mut();
        match sessions.get_mut(&id) {
            Some(s) => Ok(Ok(f(s))),
            None => Ok(Err(invalid_handle(span, id))),
        }
    })
}

// ---------------------------------------------------------------------------
// Error / argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E3142_NREPLAY_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E3140_NREPLAY_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
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

fn replay_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(
        codes::E3141_NREPLAY_ERROR,
        "nreplay_error",
        msg.into(),
        span,
    )
}

fn invalid_handle(span: Span, id: i64) -> ValueRef {
    error_value(
        codes::E3143_NREPLAY_INVALID_HANDLE,
        "nreplay_error",
        format!("invalid or closed nreplay handle {id}"),
        span,
    )
}

fn event_to_value(ev: &Event) -> ValueRef {
    let mut map = HashMap::new();
    map.insert(
        "kind".to_string(),
        Value::String(ev.kind.clone()).ref_cell(),
    );
    map.insert("data".to_string(), Rc::clone(&ev.data));
    map.insert("t_ms".to_string(), Value::Int(ev.t_ms).ref_cell());
    Value::Object(map).ref_cell()
}

fn data_to_string(v: &ValueRef) -> String {
    v.borrow().to_string()
}

const SEP: &str = "|||";

fn encode_line(ev: &Event) -> String {
    format!(
        "{}{}{}{}{}",
        ev.kind,
        SEP,
        data_to_string(&ev.data),
        SEP,
        ev.t_ms
    )
}

fn parse_line(line: &str, span: Span, line_no: usize) -> Result<Event, ValueRef> {
    let line = line.trim_end_matches(['\r', '\n']);
    if line.is_empty() {
        return Err(replay_err(
            span,
            format!("nreplay_load() empty line at {line_no}"),
        ));
    }
    let parts: Vec<&str> = line.splitn(3, SEP).collect();
    if parts.len() != 3 {
        return Err(replay_err(
            span,
            format!("nreplay_load() invalid line {line_no}: expected kind|||data|||t_ms"),
        ));
    }
    let kind = parts[0].to_string();
    if kind.is_empty() {
        return Err(replay_err(
            span,
            format!("nreplay_load() empty kind at line {line_no}"),
        ));
    }
    let t_ms: i64 = parts[2].parse().map_err(|_| {
        replay_err(
            span,
            format!("nreplay_load() invalid t_ms at line {line_no}"),
        )
    })?;
    Ok(Event {
        kind,
        data: Value::String(parts[1].to_string()).ref_cell(),
        t_ms,
    })
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// nreplay_start() -> handle
fn nreplay_start(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nreplay_start", span)?;
    let id = new_handle();
    SESSIONS.with(|sessions| {
        sessions.borrow_mut().insert(id, Session::new_running());
    });
    Ok(Value::Int(id).ref_cell())
}

/// nreplay_stop(h) -> true
fn nreplay_stop(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nreplay_stop", span)?;
    let id = int_arg(args, 0, "nreplay_stop", span)?;
    match with_session(id, span, |s| {
        s.running = false;
    })? {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(e),
    }
}

/// nreplay_record(h, kind, data) -> true
fn nreplay_record(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nreplay_record", span)?;
    let id = int_arg(args, 0, "nreplay_record", span)?;
    let kind = string_arg(args, 1, "nreplay_record", span)?;
    if kind.is_empty() {
        return Ok(replay_err(span, "nreplay_record() kind must be non-empty"));
    }
    let data = Rc::clone(&args[2]);
    match with_session(id, span, |s| {
        let t_ms = s.elapsed_ms();
        s.events.push(Event { kind, data, t_ms });
    })? {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(e),
    }
}

/// nreplay_events(h) -> [{kind, data, t_ms}, ...]
fn nreplay_events(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nreplay_events", span)?;
    let id = int_arg(args, 0, "nreplay_events", span)?;
    match with_session(id, span, |s| {
        s.events.iter().map(event_to_value).collect::<Vec<_>>()
    })? {
        Ok(items) => Ok(Value::Array(items).ref_cell()),
        Err(e) => Ok(e),
    }
}

/// nreplay_len(h) -> int
fn nreplay_len(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nreplay_len", span)?;
    let id = int_arg(args, 0, "nreplay_len", span)?;
    match with_session(id, span, |s| s.events.len() as i64)? {
        Ok(n) => Ok(Value::Int(n).ref_cell()),
        Err(e) => Ok(e),
    }
}

/// nreplay_play(h, i) -> event object or nil
fn nreplay_play(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nreplay_play", span)?;
    let id = int_arg(args, 0, "nreplay_play", span)?;
    let idx = int_arg(args, 1, "nreplay_play", span)?;
    if idx < 0 {
        return Ok(Value::Nil.ref_cell());
    }
    match with_session(id, span, |s| s.events.get(idx as usize).map(event_to_value))? {
        Ok(Some(v)) => Ok(v),
        Ok(None) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

/// nreplay_save(h, path) -> true
fn nreplay_save(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nreplay_save", span)?;
    let id = int_arg(args, 0, "nreplay_save", span)?;
    let path = string_arg(args, 1, "nreplay_save", span)?;
    let body = match with_session(id, span, |s| {
        let mut out = String::new();
        for (i, ev) in s.events.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(&encode_line(ev));
        }
        out
    })? {
        Ok(s) => s,
        Err(e) => return Ok(e),
    };
    match fs::write(&path, body) {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(replay_err(
            span,
            format!("nreplay_save() failed to write '{path}': {e}"),
        )),
    }
}

/// nreplay_load(path) -> handle
fn nreplay_load(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nreplay_load", span)?;
    let path = string_arg(args, 0, "nreplay_load", span)?;
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            return Ok(replay_err(
                span,
                format!("nreplay_load() failed to read '{path}': {e}"),
            ));
        }
    };
    let mut events = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match parse_line(line, span, i + 1) {
            Ok(ev) => events.push(ev),
            Err(e) => return Ok(e),
        }
    }
    let id = new_handle();
    SESSIONS.with(|sessions| {
        sessions
            .borrow_mut()
            .insert(id, Session::from_events(events));
    });
    Ok(Value::Int(id).ref_cell())
}

/// nreplay_clear(h) -> true
fn nreplay_clear(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nreplay_clear", span)?;
    let id = int_arg(args, 0, "nreplay_clear", span)?;
    match with_session(id, span, |s| {
        s.events.clear();
    })? {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(e),
    }
}

/// nreplay_close(h) -> true if handle existed
fn nreplay_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nreplay_close", span)?;
    let id = int_arg(args, 0, "nreplay_close", span)?;
    let removed = SESSIONS.with(|sessions| sessions.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

/// nreplay_running(h) -> bool
fn nreplay_running(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nreplay_running", span)?;
    let id = int_arg(args, 0, "nreplay_running", span)?;
    match with_session(id, span, |s| s.running)? {
        Ok(b) => Ok(Value::Bool(b).ref_cell()),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nreplay_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nreplay_fns![
    ("nreplay_start", "start", nreplay_start),
    ("nreplay_stop", "stop", nreplay_stop),
    ("nreplay_record", "record", nreplay_record),
    ("nreplay_events", "events", nreplay_events),
    ("nreplay_len", "len", nreplay_len),
    ("nreplay_play", "play", nreplay_play),
    ("nreplay_save", "save", nreplay_save),
    ("nreplay_load", "load", nreplay_load),
    ("nreplay_clear", "clear", nreplay_clear),
    ("nreplay_close", "close", nreplay_close),
    ("nreplay_running", "running", nreplay_running),
];

fn all_builtins() -> Vec<(&'static str, NativeFn)> {
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

pub const MODULE_NAME: &str = "nreplay";
pub const MODULE_PATHS: &[&str] = &["nreplay", "std/nreplay"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;
    use std::path::PathBuf;

    fn span() -> Span {
        Span::dummy()
    }

    fn i(v: i64) -> ValueRef {
        Value::Int(v).ref_cell()
    }

    fn s(v: &str) -> ValueRef {
        Value::String(v.to_string()).ref_cell()
    }

    fn handle(r: NiaoResult<ValueRef>) -> ValueRef {
        let v = r.unwrap();
        assert!(matches!(&*v.borrow(), Value::Int(_)), "expected handle int");
        v
    }

    fn temp_path(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("nreplay_test_{}_{}", std::process::id(), name));
        p
    }

    #[test]
    fn start_stop_running() {
        let h = handle(nreplay_start(&[], span()));
        let r = nreplay_running(&[h.clone()], span()).unwrap();
        assert!(matches!(&*r.borrow(), Value::Bool(true)));
        nreplay_stop(&[h.clone()], span()).unwrap();
        let r = nreplay_running(&[h.clone()], span()).unwrap();
        assert!(matches!(&*r.borrow(), Value::Bool(false)));
        nreplay_close(&[h], span()).unwrap();
    }

    #[test]
    fn record_events_play_len() {
        let h = handle(nreplay_start(&[], span()));
        nreplay_record(&[h.clone(), s("tick"), i(1)], span()).unwrap();
        nreplay_record(&[h.clone(), s("note"), s("hello")], span()).unwrap();
        let n = nreplay_len(&[h.clone()], span()).unwrap();
        assert!(matches!(&*n.borrow(), Value::Int(2)));

        let evs = nreplay_events(&[h.clone()], span()).unwrap();
        match &*evs.borrow() {
            Value::Array(items) => assert_eq!(items.len(), 2),
            other => panic!("expected array, got {other:?}"),
        }

        let first = nreplay_play(&[h.clone(), i(0)], span()).unwrap();
        match &*first.borrow() {
            Value::Object(map) => {
                assert!(matches!(&*map["kind"].borrow(), Value::String(k) if k == "tick"));
                assert!(matches!(&*map["data"].borrow(), Value::Int(1)));
                assert!(matches!(&*map["t_ms"].borrow(), Value::Int(_)));
            }
            other => panic!("expected object, got {other:?}"),
        }

        let missing = nreplay_play(&[h.clone(), i(99)], span()).unwrap();
        assert!(matches!(&*missing.borrow(), Value::Nil));
        nreplay_close(&[h], span()).unwrap();
    }

    #[test]
    fn clear_and_close() {
        let h = handle(nreplay_start(&[], span()));
        nreplay_record(&[h.clone(), s("a"), s("x")], span()).unwrap();
        nreplay_clear(&[h.clone()], span()).unwrap();
        let n = nreplay_len(&[h.clone()], span()).unwrap();
        assert!(matches!(&*n.borrow(), Value::Int(0)));
        let closed = nreplay_close(&[h.clone()], span()).unwrap();
        assert!(matches!(&*closed.borrow(), Value::Bool(true)));
        let again = nreplay_close(&[h], span()).unwrap();
        assert!(matches!(&*again.borrow(), Value::Bool(false)));
    }

    #[test]
    fn save_load_roundtrip() {
        let path = temp_path("roundtrip.nrep");
        let _ = fs::remove_file(&path);

        let h = handle(nreplay_start(&[], span()));
        nreplay_record(&[h.clone(), s("rng"), s("42")], span()).unwrap();
        nreplay_record(&[h.clone(), s("time"), s("100")], span()).unwrap();
        let ok = nreplay_save(&[h.clone(), s(path.to_str().unwrap())], span()).unwrap();
        assert!(matches!(&*ok.borrow(), Value::Bool(true)));
        nreplay_close(&[h], span()).unwrap();

        let loaded = handle(nreplay_load(&[s(path.to_str().unwrap())], span()));
        let r = nreplay_running(&[loaded.clone()], span()).unwrap();
        assert!(matches!(&*r.borrow(), Value::Bool(false)));
        let n = nreplay_len(&[loaded.clone()], span()).unwrap();
        assert!(matches!(&*n.borrow(), Value::Int(2)));
        let ev = nreplay_play(&[loaded.clone(), i(0)], span()).unwrap();
        match &*ev.borrow() {
            Value::Object(map) => {
                assert!(matches!(&*map["kind"].borrow(), Value::String(k) if k == "rng"));
                assert!(matches!(&*map["data"].borrow(), Value::String(d) if d == "42"));
            }
            other => panic!("expected object, got {other:?}"),
        }
        nreplay_close(&[loaded], span()).unwrap();
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn invalid_handle_error_value() {
        let v = nreplay_len(&[i(424_242)], span()).unwrap();
        match &*v.borrow() {
            Value::Error(e) => assert_eq!(e.code, codes::E3143_NREPLAY_INVALID_HANDLE),
            other => panic!("expected error, got {other:?}"),
        }
    }

    #[test]
    fn arity_and_type_errors() {
        let err = nreplay_start(&[i(1)], span()).unwrap_err();
        assert_eq!(err.code(), codes::E3140_NREPLAY_ARITY);

        let err = nreplay_record(&[s("not-int"), s("k"), i(1)], span()).unwrap_err();
        assert_eq!(err.code(), codes::E3142_NREPLAY_TYPE);

        let err = nreplay_play(&[], span()).unwrap_err();
        assert_eq!(err.code(), codes::E3140_NREPLAY_ARITY);
    }

    #[test]
    fn load_bad_line() {
        let path = temp_path("bad.nrep");
        fs::write(&path, "not-a-valid-line\n").unwrap();
        let v = nreplay_load(&[s(path.to_str().unwrap())], span()).unwrap();
        match &*v.borrow() {
            Value::Error(e) => assert_eq!(e.code, codes::E3141_NREPLAY_ERROR),
            other => panic!("expected error, got {other:?}"),
        }
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn namespace_has_expected_methods() {
        match namespace() {
            Value::Object(map) => {
                for key in [
                    "start", "stop", "record", "events", "len", "play", "save", "load", "clear",
                    "close", "running",
                ] {
                    assert!(map.contains_key(key), "missing {key}");
                }
            }
            other => panic!("expected object, got {other:?}"),
        }
        assert_eq!(builtins().len(), 11);
        assert_eq!(MODULE_NAME, "nreplay");
        assert_eq!(MODULE_PATHS, &["nreplay", "std/nreplay"]);
    }
}
