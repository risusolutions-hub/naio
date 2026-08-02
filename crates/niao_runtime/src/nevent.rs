//! Native nevent standard library — in-process event emitter / pub-sub with
//! dot-separated typed topics and `*` / `**` wildcards (~blinker, pyee subset).
//!
//! Import with `import "nevent"` (or `import "std/nevent"`).

use crate::{call_niao_function, error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_event::{Emitter, EmitterOptions, SubId, TopicError};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// ---------------------------------------------------------------------------
// Emitter handles + handler registry
// ---------------------------------------------------------------------------

struct HandlerEntry {
    handler: ValueRef,
}

struct EmitterStore {
    engine: Emitter,
    handlers: HashMap<SubId, HandlerEntry>,
    pending: Vec<PendingEmit>,
}

struct PendingEmit {
    topic: String,
    args: Vec<ValueRef>,
}

thread_local! {
    static EMITTERS: RefCell<HashMap<i64, EmitterStore>> = RefCell::new(HashMap::new());
    static NEXT_HANDLE: RefCell<i64> = const { RefCell::new(1) };
    static GLOBAL_HANDLE: RefCell<Option<i64>> = const { RefCell::new(None) };
}

fn alloc_handle() -> i64 {
    NEXT_HANDLE.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    })
}

fn register_emitter(engine: Emitter) -> i64 {
    let id = alloc_handle();
    EMITTERS.with(|m| {
        m.borrow_mut().insert(
            id,
            EmitterStore {
                engine,
                handlers: HashMap::new(),
                pending: Vec::new(),
            },
        );
    });
    id
}

fn with_emitter_mut<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut EmitterStore) -> Result<T, ValueRef>,
) -> NiaoResult<Result<T, ValueRef>> {
    EMITTERS.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(store) => Ok(f(store)),
            None => Ok(Err(invalid_handle(span, id))),
        }
    })
}

fn with_emitter<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&EmitterStore) -> Result<T, ValueRef>,
) -> NiaoResult<Result<T, ValueRef>> {
    EMITTERS.with(|m| {
        let m = m.borrow();
        match m.get(&id) {
            Some(store) => Ok(f(store)),
            None => Ok(Err(invalid_handle(span, id))),
        }
    })
}

fn global_handle() -> i64 {
    GLOBAL_HANDLE.with(|g| {
        if let Some(id) = *g.borrow() {
            return id;
        }
        let id = register_emitter(Emitter::default());
        *g.borrow_mut() = Some(id);
        id
    })
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E3510_NEVENT_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E3508_NEVENT_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E3508_NEVENT_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_min(args: &[ValueRef], min: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min {
        return Err(RuntimeError::at(
            span,
            codes::E3508_NEVENT_ARITY,
            format!("{name}() expects at least {min} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn nevent_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E3509_NEVENT_ERROR, "nevent_error", msg.into(), span)
}

fn invalid_handle(span: Span, id: i64) -> ValueRef {
    error_value(
        codes::E3511_NEVENT_INVALID_HANDLE,
        "nevent_error",
        format!("invalid or closed emitter handle {id}"),
        span,
    )
}

fn topic_err(span: Span, e: TopicError) -> ValueRef {
    error_value(
        codes::E3512_NEVENT_TOPIC,
        "nevent_error",
        e.to_string(),
        span,
    )
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

fn callable_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<ValueRef> {
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

fn optional_object(args: &[ValueRef], idx: usize) -> Option<HashMap<String, ValueRef>> {
    args.get(idx).and_then(|v| match &*v.borrow() {
        Value::Object(map) => Some(map.clone()),
        Value::Nil => None,
        _ => None,
    })
}

fn parse_emitter_opts(opts: Option<HashMap<String, ValueRef>>, span: Span) -> Result<EmitterOptions, ValueRef> {
    let mut out = EmitterOptions::default();
    let Some(map) = opts else {
        return Ok(out);
    };
    if let Some(v) = map.get("max_listeners") {
        match &*v.borrow() {
            Value::Int(n) if *n >= 0 => {
                out.max_listeners_per_pattern = *n as usize;
            }
            other => {
                return Err(nevent_err(
                    span,
                    format!("max_listeners must be a non-negative int, got {}", other.type_name()),
                ));
            }
        }
    }
    Ok(out)
}

fn invoke_handler(handler: &ValueRef, args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    match &*handler.borrow() {
        Value::NativeFunction(native) => native(args, span),
        Value::Function(_) => call_niao_function(Rc::clone(handler), args, span),
        other => Err(type_err(
            span,
            format!("expected callable handler, got {}", other.type_name()),
        )),
    }
}

fn handlers_equal(a: &ValueRef, b: &ValueRef) -> bool {
    match (&*a.borrow(), &*b.borrow()) {
        (Value::NativeFunction(fa), Value::NativeFunction(fb)) => Rc::ptr_eq(fa, fb),
        (Value::Function(fa), Value::Function(fb)) => Rc::ptr_eq(fa, fb),
        _ => false,
    }
}

fn dispatch_emit(
    store: &mut EmitterStore,
    topic: String,
    args: Vec<ValueRef>,
    span: Span,
) -> Result<ValueRef, ValueRef> {
    if store.engine.is_paused() {
        store.pending.push(PendingEmit { topic, args });
        let mut map = HashMap::new();
        map.insert("called".to_string(), Value::Int(0).ref_cell());
        map.insert("queued".to_string(), Value::Bool(true).ref_cell());
        return Ok(Value::Object(map).ref_cell());
    }

    let ids = store.engine.matching_ids(&topic);
    let mut called = 0i64;
    let mut errors: Vec<ValueRef> = Vec::new();
    let mut once_ids: Vec<SubId> = Vec::new();

    for id in &ids {
        let Some(entry) = store.handlers.get(id) else {
            continue;
        };
        let pattern = store
            .engine
            .pattern_for(*id)
            .unwrap_or("")
            .to_string();
        let has_wildcard = pattern.contains('*');
        let mut call_args = Vec::new();
        if has_wildcard {
            call_args.push(Value::String(topic.clone()).ref_cell());
        }
        call_args.extend(args.iter().cloned());

        match invoke_handler(&entry.handler, &call_args, span) {
            Ok(_) => {
                called += 1;
                if store.engine.is_once(*id) {
                    once_ids.push(*id);
                }
            }
            Err(e) => {
                errors.push(Value::String(e.to_string()).ref_cell());
            }
        }
    }

    store.engine.consume_once(&once_ids);
    store.engine.record_emit(called as usize);

    let mut map = HashMap::new();
    map.insert("called".to_string(), Value::Int(called).ref_cell());
    map.insert("queued".to_string(), Value::Bool(false).ref_cell());
    if !errors.is_empty() {
        map.insert("errors".to_string(), Value::Array(errors).ref_cell());
    }
    Ok(Value::Object(map).ref_cell())
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

// >>> let h = nevent.new()
// => handle int
fn nevent_new(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nevent_new", span)?;
    let opts = match parse_emitter_opts(optional_object(args, 0), span) {
        Ok(o) => o,
        Err(e) => return Ok(e),
    };
    let id = register_emitter(Emitter::new(opts));
    Ok(Value::Int(id).ref_cell())
}

// >>> nevent.global()
// => global bus handle
fn nevent_global(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nevent_global", span)?;
    match global_handle() {
        id => Ok(Value::Int(id).ref_cell()),
    }
}

// >>> nevent.close(handle)
// => true
fn nevent_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nevent_close", span)?;
    let id = handle_arg(args, 0, "nevent_close", span)?;
    let removed = EMITTERS.with(|m| m.borrow_mut().remove(&id).is_some());
    GLOBAL_HANDLE.with(|g| {
        if *g.borrow() == Some(id) {
            *g.borrow_mut() = None;
        }
    });
    Ok(Value::Bool(removed).ref_cell())
}

// >>> nevent.on(handle, "user.created", fn(data) { })
// => subscription id
fn nevent_on(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nevent_on", span)?;
    let id = handle_arg(args, 0, "nevent_on", span)?;
    let topic = string_arg(args, 1, "nevent_on", span)?;
    let handler = callable_arg(args, 2, "nevent_on", span)?;
    match with_emitter_mut(id, span, |store| {
        match store.engine.subscribe(&topic, false) {
            Ok(sub_id) => {
                store.handlers.insert(sub_id, HandlerEntry { handler });
                Ok(Value::Int(sub_id as i64).ref_cell())
            }
            Err(niao_event::SubscribeError::InvalidPattern(e)) => Err(topic_err(span, e)),
            Err(niao_event::SubscribeError::MaxListeners) => Err(nevent_err(
                span,
                format!("max listeners exceeded for pattern '{topic}'"),
            )),
        }
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

// >>> nevent.once(handle, "boot", fn() { })
// => subscription id
fn nevent_once(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nevent_once", span)?;
    let id = handle_arg(args, 0, "nevent_once", span)?;
    let topic = string_arg(args, 1, "nevent_once", span)?;
    let handler = callable_arg(args, 2, "nevent_once", span)?;
    match with_emitter_mut(id, span, |store| {
        match store.engine.subscribe(&topic, true) {
            Ok(sub_id) => {
                store.handlers.insert(sub_id, HandlerEntry { handler });
                Ok(Value::Int(sub_id as i64).ref_cell())
            }
            Err(niao_event::SubscribeError::InvalidPattern(e)) => Err(topic_err(span, e)),
            Err(niao_event::SubscribeError::MaxListeners) => Err(nevent_err(
                span,
                format!("max listeners exceeded for pattern '{topic}'"),
            )),
        }
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

// >>> nevent.off(handle, "user.created", handler?)
// => count removed
fn nevent_off(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nevent_off", span)?;
    let id = handle_arg(args, 0, "nevent_off", span)?;
    let topic = string_arg(args, 1, "nevent_off", span)?;
    match with_emitter_mut(id, span, |store| {
        if args.len() == 3 {
            let handler = callable_arg(args, 2, "nevent_off", span)?;
            let normalized = niao_event::normalize(&topic);
            let mut removed = 0i64;
            let ids: Vec<SubId> = store
                .handlers
                .iter()
                .filter_map(|(sid, entry)| {
                    if !handlers_equal(&entry.handler, &handler) {
                        return None;
                    }
                    store
                        .engine
                        .pattern_for(*sid)
                        .filter(|p| *p == normalized.as_str())
                        .map(|_| *sid)
                })
                .collect();
            for sid in ids {
                if store.engine.unsubscribe_id(sid) {
                    store.handlers.remove(&sid);
                    removed += 1;
                }
            }
            Ok(Value::Int(removed).ref_cell())
        } else {
            let removed = store.engine.unsubscribe_pattern(&topic) as i64;
            store.handlers.retain(|sid, _| store.engine.pattern_for(*sid).is_some());
            Ok(Value::Int(removed).ref_cell())
        }
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

// >>> nevent.off_id(handle, sub_id)
// => bool
fn nevent_off_id(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nevent_off_id", span)?;
    let id = handle_arg(args, 0, "nevent_off_id", span)?;
    let sub_id = int_arg(args, 1, "nevent_off_id", span)? as SubId;
    match with_emitter_mut(id, span, |store| {
        let ok = store.engine.unsubscribe_id(sub_id);
        if ok {
            store.handlers.remove(&sub_id);
        }
        Ok(Value::Bool(ok).ref_cell())
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

// >>> nevent.emit(handle, "user.created", payload)
// => {called, queued, errors?}
fn nevent_emit(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_min(args, 2, "nevent_emit", span)?;
    let id = handle_arg(args, 0, "nevent_emit", span)?;
    let topic = string_arg(args, 1, "nevent_emit", span)?;
    if !niao_event::is_valid_topic(&topic) {
        if let Err(e) = niao_event::TopicPattern::parse(&topic) {
            return Ok(topic_err(span, e));
        }
        return Ok(topic_err(span, TopicError::Empty));
    }
    let payload: Vec<ValueRef> = args[2..].iter().cloned().collect();
    match with_emitter_mut(id, span, |store| {
        dispatch_emit(store, topic, payload, span)
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

// >>> nevent.emit(handle, "user.created", payload) — validate topic on emit

// >>> nevent.pause(handle)
fn nevent_pause(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nevent_pause", span)?;
    let id = handle_arg(args, 0, "nevent_pause", span)?;
    match with_emitter_mut(id, span, |store| {
        store.engine.pause();
        Ok(Value::Bool(true).ref_cell())
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

// >>> nevent.resume(handle)
fn nevent_resume(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nevent_resume", span)?;
    let id = handle_arg(args, 0, "nevent_resume", span)?;
    match with_emitter_mut(id, span, |store| {
        store.engine.resume();
        Ok(Value::Bool(true).ref_cell())
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

// >>> nevent.flush(handle)
fn nevent_flush(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nevent_flush", span)?;
    let id = handle_arg(args, 0, "nevent_flush", span)?;
    match with_emitter_mut(id, span, |store| {
        let pending = std::mem::take(&mut store.pending);
        let mut total_called = 0i64;
        let mut batches = Vec::new();
        for p in pending {
            let result = dispatch_emit(store, p.topic, p.args, span)?;
            match &*result.borrow() {
                Value::Object(map) => {
                    if let Value::Int(n) = &*map.get("called").unwrap().borrow() {
                        total_called += n;
                    }
                    batches.push(result);
                }
                _ => batches.push(result),
            }
        }
        let mut map = HashMap::new();
        map.insert("called".to_string(), Value::Int(total_called).ref_cell());
        map.insert("batches".to_string(), Value::Array(batches).ref_cell());
        Ok(Value::Object(map).ref_cell())
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

// >>> nevent.clear(handle, pattern?)
fn nevent_clear(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nevent_clear", span)?;
    let id = handle_arg(args, 0, "nevent_clear", span)?;
    match with_emitter_mut(id, span, |store| {
        let removed = if args.len() == 2 {
            let pattern = string_arg(args, 1, "nevent_clear", span)?;
            store.engine.unsubscribe_pattern(&pattern) as i64
        } else {
            store.handlers.clear();
            store.engine.clear() as i64
        };
        store.handlers.retain(|sid, _| store.engine.pattern_for(*sid).is_some());
        Ok(Value::Int(removed).ref_cell())
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

// >>> nevent.listener_count(handle, topic?)
fn nevent_listener_count(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nevent_listener_count", span)?;
    let id = handle_arg(args, 0, "nevent_listener_count", span)?;
    let filter = if args.len() == 2 {
        Some(string_arg(args, 1, "nevent_listener_count", span)?)
    } else {
        None
    };
    match with_emitter(id, span, |store| {
        let n = store.engine.listener_count(filter.as_deref());
        Ok(Value::Int(n as i64).ref_cell())
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

// >>> nevent.has_listeners(handle, topic?)
fn nevent_has_listeners(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nevent_has_listeners", span)?;
    let id = handle_arg(args, 0, "nevent_has_listeners", span)?;
    let filter = if args.len() == 2 {
        Some(string_arg(args, 1, "nevent_has_listeners", span)?)
    } else {
        None
    };
    match with_emitter(id, span, |store| {
        Ok(Value::Bool(store.engine.has_listeners(filter.as_deref())).ref_cell())
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

// >>> nevent.topics(handle)
fn nevent_topics(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nevent_topics", span)?;
    let id = handle_arg(args, 0, "nevent_topics", span)?;
    match with_emitter(id, span, |store| {
        let items: Vec<ValueRef> = store
            .engine
            .topics()
            .into_iter()
            .map(|s| Value::String(s).ref_cell())
            .collect();
        Ok(Value::Array(items).ref_cell())
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

// >>> nevent.stats(handle)
fn nevent_stats(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nevent_stats", span)?;
    let id = handle_arg(args, 0, "nevent_stats", span)?;
    match with_emitter(id, span, |store| {
        let s = store.engine.stats();
        let mut map = HashMap::new();
        map.insert("emits".to_string(), Value::Int(s.emit_count as i64).ref_cell());
        map.insert(
            "deliveries".to_string(),
            Value::Int(s.delivery_count as i64).ref_cell(),
        );
        map.insert(
            "subscriptions".to_string(),
            Value::Int(s.subscription_count as i64).ref_cell(),
        );
        map.insert(
            "paused".to_string(),
            Value::Bool(store.engine.is_paused()).ref_cell(),
        );
        map.insert(
            "pending".to_string(),
            Value::Int(store.pending.len() as i64).ref_cell(),
        );
        Ok(Value::Object(map).ref_cell())
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

// >>> nevent.match_topic("user.*", "user.created")
// => true
fn nevent_match_topic(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nevent_match_topic", span)?;
    let pattern = string_arg(args, 0, "nevent_match_topic", span)?;
    let topic = string_arg(args, 1, "nevent_match_topic", span)?;
    Ok(Value::Bool(niao_event::topic_matches(&pattern, &topic)).ref_cell())
}

// >>> nevent.parse_topic("a.b.c")
// => ["a", "b", "c"]
fn nevent_parse_topic(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nevent_parse_topic", span)?;
    let topic = string_arg(args, 0, "nevent_parse_topic", span)?;
    let parts = niao_event::split_topic(&topic);
    if parts.is_empty() && !topic.trim().is_empty() {
        return Ok(topic_err(span, TopicError::InvalidChar('?')));
    }
    let items: Vec<ValueRef> = parts.into_iter().map(|s| Value::String(s).ref_cell()).collect();
    Ok(Value::Array(items).ref_cell())
}

// >>> nevent.join_topic(["a", "b"])
// => "a.b"
fn nevent_join_topic(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nevent_join_topic", span)?;
    let parts = match &*args[0].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::String(s) => out.push(s.as_str()),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "nevent_join_topic() segment {} must be string, got {}",
                                i + 1,
                                other.type_name()
                            ),
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
                    "nevent_join_topic() expects string array, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    match niao_event::join_topic(&parts) {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(topic_err(span, e)),
    }
}

// >>> nevent.normalize_topic(" foo..bar ")
// => "foo.bar"
fn nevent_normalize_topic(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nevent_normalize_topic", span)?;
    let topic = string_arg(args, 0, "nevent_normalize_topic", span)?;
    Ok(Value::String(niao_event::normalize(&topic)).ref_cell())
}

// >>> nevent.is_valid_topic("user.created")
// => true
fn nevent_is_valid_topic(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nevent_is_valid_topic", span)?;
    let topic = string_arg(args, 0, "nevent_is_valid_topic", span)?;
    Ok(Value::Bool(niao_event::is_valid_topic(&topic)).ref_cell())
}

// >>> nevent.is_valid_pattern("user.*")
fn nevent_is_valid_pattern(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nevent_is_valid_pattern", span)?;
    let pattern = string_arg(args, 0, "nevent_is_valid_pattern", span)?;
    Ok(Value::Bool(niao_event::is_valid_pattern(&pattern)).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nevent_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nevent_fns![
    ("nevent_new", "new", nevent_new),
    ("nevent_global", "global", nevent_global),
    ("nevent_close", "close", nevent_close),
    ("nevent_on", "on", nevent_on),
    ("nevent_once", "once", nevent_once),
    ("nevent_off", "off", nevent_off),
    ("nevent_off_id", "off_id", nevent_off_id),
    ("nevent_emit", "emit", nevent_emit),
    ("nevent_pause", "pause", nevent_pause),
    ("nevent_resume", "resume", nevent_resume),
    ("nevent_flush", "flush", nevent_flush),
    ("nevent_clear", "clear", nevent_clear),
    ("nevent_listener_count", "listener_count", nevent_listener_count),
    ("nevent_has_listeners", "has_listeners", nevent_has_listeners),
    ("nevent_topics", "topics", nevent_topics),
    ("nevent_stats", "stats", nevent_stats),
    ("nevent_match_topic", "match_topic", nevent_match_topic),
    ("nevent_parse_topic", "parse_topic", nevent_parse_topic),
    ("nevent_join_topic", "join_topic", nevent_join_topic),
    ("nevent_normalize_topic", "normalize_topic", nevent_normalize_topic),
    ("nevent_is_valid_topic", "is_valid_topic", nevent_is_valid_topic),
    ("nevent_is_valid_pattern", "is_valid_pattern", nevent_is_valid_pattern),
];

pub const MODULE_NAME: &str = "nevent";
pub const MODULE_PATHS: &[&str] = &["nevent", "std/nevent"];

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

    fn native_echo(args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
        Ok(Rc::clone(&args[0]))
    }

    #[test]
    fn on_emit_once() {
        let em = nevent_new(&[], span()).unwrap();
        let handler = Value::NativeFunction(Rc::new(native_echo)).ref_cell();
        nevent_on(
            &[em.clone(), Value::String("ping".into()).ref_cell(), handler],
            span(),
        )
        .unwrap();
        let r = nevent_emit(
            &[em.clone(), Value::String("ping".into()).ref_cell(), Value::Int(42).ref_cell()],
            span(),
        )
        .unwrap();
        match &*r.borrow() {
            Value::Object(m) => assert_eq!(*m.get("called").unwrap().borrow(), Value::Int(1)),
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn match_topic_builtin() {
        let r = nevent_match_topic(
            &[
                Value::String("user.*".into()).ref_cell(),
                Value::String("user.created".into()).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        assert_eq!(*r.borrow(), Value::Bool(true));
    }
}
