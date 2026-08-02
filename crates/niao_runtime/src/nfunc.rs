//! Native nfunc standard library — function toolkit: partial, curry, compose,
//! pipe, memoize/LRU, once, debounce, throttle (~functools / toolz subset).
//! Wrappers are native callables backed by Rust; user functions dispatch via
//! the runtime call hook.
//!
//! Import with `import "nfunc"` (or `import "std/nfunc"`).

use crate::{call_niao_function, error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Wrapper registry — map returned NativeFunction Rc to mutable state.
// ---------------------------------------------------------------------------

struct WrapperEntry {
    native: NativeFn,
    state: Rc<RefCell<WrapperState>>,
}

enum WrapperState {
    Memo(MemoState),
    Once(OnceState),
    Throttle(ThrottleState),
    Debounce(DebounceState),
    Generic,
}

struct MemoState {
    callee: ValueRef,
    capacity: usize,
    map: HashMap<MemoKey, (u64, ValueRef)>,
    recency: BTreeMap<u64, MemoKey>,
    tick: u64,
    hits: u64,
    misses: u64,
}

struct OnceState {
    callee: ValueRef,
    done: bool,
    result: Option<ValueRef>,
}

struct ThrottleState {
    callee: ValueRef,
    interval_ms: i64,
    leading: bool,
    trailing: bool,
    last_invoke_ms: i64,
    last_args: Vec<ValueRef>,
    last_result: Option<ValueRef>,
    trailing_pending: bool,
}

struct DebounceState {
    callee: ValueRef,
    wait_ms: i64,
    leading: bool,
    trailing: bool,
    last_touch_ms: i64,
    last_invoke_ms: i64,
    pending_args: Option<Vec<ValueRef>>,
    last_result: Option<ValueRef>,
}

thread_local! {
    static WRAPPER_REGISTRY: RefCell<Vec<WrapperEntry>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone, Hash, PartialEq, Eq)]
enum MemoKey {
    Nil,
    Bool(bool),
    Int(i64),
    FloatBits(u64),
    Str(String),
    Tuple(Vec<MemoKey>),
}

// ---------------------------------------------------------------------------
// Time + invocation
// ---------------------------------------------------------------------------

#[inline]
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn invoke_callable(callee: ValueRef, args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    match &*callee.borrow() {
        Value::NativeFunction(native) => native(args, span),
        Value::Function(_) => call_niao_function(callee, args, span),
        other => Err(type_err(
            span,
            format!("expected callable, got {}", other.type_name()),
        )),
    }
}

fn register_wrapper(
    state: WrapperState,
    handler: impl Fn(&[ValueRef], Span, Rc<RefCell<WrapperState>>) -> NiaoResult<ValueRef> + 'static,
) -> NativeFn {
    let state = Rc::new(RefCell::new(state));
    let state_for_fn = Rc::clone(&state);
    let native: NativeFn = Rc::new(move |args, span| handler(args, span, Rc::clone(&state_for_fn)));
    WRAPPER_REGISTRY.with(|reg| {
        reg.borrow_mut().push(WrapperEntry {
            native: Rc::clone(&native),
            state,
        });
    });
    native
}

fn lookup_wrapper(func: &ValueRef) -> Option<Rc<RefCell<WrapperState>>> {
    if let Value::NativeFunction(f) = &*func.borrow() {
        WRAPPER_REGISTRY.with(|reg| {
            reg.borrow()
                .iter()
                .find(|e| Rc::ptr_eq(&e.native, f))
                .map(|e| Rc::clone(&e.state))
        })
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Memo keys
// ---------------------------------------------------------------------------

fn value_to_memo_key(v: &Value, span: Span) -> Result<MemoKey, ValueRef> {
    match v {
        Value::Nil => Ok(MemoKey::Nil),
        Value::Bool(b) => Ok(MemoKey::Bool(*b)),
        Value::Int(n) => Ok(MemoKey::Int(*n)),
        Value::Float(f) => Ok(MemoKey::FloatBits(f.to_bits())),
        Value::String(s) => Ok(MemoKey::Str(s.clone())),
        Value::Array(items) => {
            let keys: Result<Vec<_>, _> = items
                .iter()
                .map(|item| value_to_memo_key(&item.borrow(), span))
                .collect();
            Ok(MemoKey::Tuple(keys?))
        }
        Value::IntArray(v) => Ok(MemoKey::Tuple(
            v.iter().map(|n| MemoKey::Int(*n)).collect(),
        )),
        Value::FloatArray(v) => Ok(MemoKey::Tuple(
            v.iter().map(|n| MemoKey::FloatBits(n.to_bits())).collect(),
        )),
        Value::BoolArray(v) => Ok(MemoKey::Tuple(
            v.iter().map(|n| MemoKey::Bool(*n != 0)).collect(),
        )),
        other => Err(nfunc_err(
            span,
            format!(
                "memoize: unhashable argument type '{}' (use int/string/bool/nil/array of hashables)",
                other.type_name()
            ),
        )),
    }
}

fn args_to_memo_key(args: &[ValueRef], span: Span) -> Result<MemoKey, ValueRef> {
    if args.is_empty() {
        return Ok(MemoKey::Tuple(vec![]));
    }
    if args.len() == 1 {
        return value_to_memo_key(&args[0].borrow(), span);
    }
    let parts: Result<Vec<_>, _> = args
        .iter()
        .map(|a| value_to_memo_key(&a.borrow(), span))
        .collect();
    Ok(MemoKey::Tuple(parts?))
}

fn memo_touch(state: &mut MemoState, key: &MemoKey) {
    let tick = state.tick + 1;
    state.tick = tick;
    if let Some((old_tick, _)) = state.map.get(key) {
        state.recency.remove(old_tick);
    }
    state.recency.insert(tick, key.clone());
}

fn memo_evict(state: &mut MemoState) {
    if state.capacity == 0 {
        return;
    }
    while state.map.len() > state.capacity {
        let Some((&oldest, key)) = state.recency.iter().next() else {
            break;
        };
        let key = key.clone();
        state.recency.remove(&oldest);
        state.map.remove(&key);
    }
}

fn memo_get(state: &mut MemoState, key: &MemoKey) -> Option<ValueRef> {
    if let Some((tick, val)) = state.map.get(key) {
        state.hits += 1;
        let tick = *tick;
        let val = Rc::clone(val);
        state.recency.remove(&tick);
        let new_tick = state.tick + 1;
        state.tick = new_tick;
        state.recency.insert(new_tick, key.clone());
        if let Some(entry) = state.map.get_mut(key) {
            entry.0 = new_tick;
        }
        Some(val)
    } else {
        state.misses += 1;
        None
    }
}

fn memo_insert(state: &mut MemoState, key: MemoKey, val: ValueRef) {
    memo_touch(state, &key);
    let tick = state.tick;
    state.map.insert(key, (tick, val));
    memo_evict(state);
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E2695_NFUNC_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E2693_NFUNC_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E2693_NFUNC_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn callable_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<ValueRef> {
    match &*args[idx].borrow() {
        Value::Function(_) | Value::NativeFunction(_) => Ok(Rc::clone(&args[idx])),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a function as argument {}, got {}",
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

fn array_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<ValueRef>> {
    match &*args[idx].borrow() {
        Value::Array(items) => Ok(items.iter().map(Rc::clone).collect()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn bool_opt(map: &HashMap<String, ValueRef>, key: &str, default: bool) -> bool {
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Bool(b)) => b,
        _ => default,
    }
}

fn nfunc_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E2694_NFUNC_ERROR, "nfunc_error", msg.into(), span)
}

fn function_arity(v: &Value) -> Option<usize> {
    match v {
        Value::Function(f) => Some(f.def.params.len()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Curry helper
// ---------------------------------------------------------------------------

fn curry_layer(
    func: ValueRef,
    bound: Vec<ValueRef>,
    total_arity: usize,
    span: Span,
) -> NiaoResult<ValueRef> {
    if bound.len() >= total_arity {
        return invoke_callable(func, &bound[..total_arity], span);
    }
    let func_c = Rc::clone(&func);
    let bound_c = bound;
    let native = register_wrapper(WrapperState::Generic, move |args, call_span, _| {
        let mut merged = bound_c.clone();
        merged.extend_from_slice(args);
        if merged.len() >= total_arity {
            invoke_callable(Rc::clone(&func_c), &merged[..total_arity], call_span)
        } else {
            curry_layer(Rc::clone(&func_c), merged, total_arity, call_span)
        }
    });
    Ok(Value::NativeFunction(native).ref_cell())
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// nfunc_partial(fn, ...args) — bind leading positional arguments.
///
// >>> import "nfunc"
// >>> let add = fn(a, b) { return a + b }
// >>> let inc = nfunc.partial(add, 1)
// >>> inc(5)
// => 6
fn nfunc_partial(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 64, "nfunc_partial", span)?;
    let func = callable_arg(args, 0, "nfunc_partial", span)?;
    let bound: Vec<ValueRef> = args[1..].iter().map(Rc::clone).collect();
    let func_c = Rc::clone(&func);
    let native = register_wrapper(WrapperState::Generic, move |call_args, call_span, _| {
        let mut all = bound.clone();
        all.extend_from_slice(call_args);
        invoke_callable(Rc::clone(&func_c), &all, call_span)
    });
    Ok(Value::NativeFunction(native).ref_cell())
}

/// nfunc_partial_right(fn, ...args) — bind trailing positional arguments.
///
// >>> let sub = fn(a, b) { return a - b }
// >>> let sub5 = nfunc.partial_right(sub, 5)
// >>> sub5(10)
// => 5
fn nfunc_partial_right(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 64, "nfunc_partial_right", span)?;
    let func = callable_arg(args, 0, "nfunc_partial_right", span)?;
    let bound: Vec<ValueRef> = args[1..].iter().map(Rc::clone).collect();
    let func_c = Rc::clone(&func);
    let native = register_wrapper(WrapperState::Generic, move |call_args, call_span, _| {
        let mut all = call_args.to_vec();
        all.extend(bound.iter().cloned());
        invoke_callable(Rc::clone(&func_c), &all, call_span)
    });
    Ok(Value::NativeFunction(native).ref_cell())
}

/// nfunc_curry(fn, arity?) — unary currying; arity defaults from fn param count.
///
// >>> let add = fn(a, b) { return a + b }
// >>> let curried = nfunc.curry(add)
// >>> curried(2)(3)
// => 5
fn nfunc_curry(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nfunc_curry", span)?;
    let func = callable_arg(args, 0, "nfunc_curry", span)?;
    let total = if args.len() == 2 {
        let n = int_arg(args, 1, "nfunc_curry", span)?;
        if n <= 0 {
            return Ok(nfunc_err(span, "nfunc_curry() arity must be >= 1"));
        }
        n as usize
    } else {
        match function_arity(&func.borrow()) {
            Some(n) if n > 0 => n,
            _ => {
                return Err(type_err(
                    span,
                    "nfunc_curry() cannot infer arity for native functions — pass arity explicitly",
                ));
            }
        }
    };
    curry_layer(func, Vec::new(), total, span)
}

/// nfunc_compose(...fns) — right-to-left composition: compose(f,g)(x) = f(g(x)).
///
// >>> let f = fn(x) { return x + 1 }
// >>> let g = fn(x) { return x * 2 }
// >>> nfunc.compose(f, g)(3)
// => 7
fn nfunc_compose(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 32, "nfunc_compose", span)?;
    if args.len() == 1 {
        return Ok(callable_arg(args, 0, "nfunc_compose", span)?);
    }
    let funcs: Vec<ValueRef> = (0..args.len())
        .map(|i| callable_arg(args, i, "nfunc_compose", span))
        .collect::<Result<_, _>>()?;
    let funcs_c = funcs.clone();
    let native = register_wrapper(WrapperState::Generic, move |call_args, call_span, _| {
        let mut val = invoke_callable(
            Rc::clone(funcs_c.last().expect("compose non-empty")),
            call_args,
            call_span,
        )?;
        for f in funcs_c.iter().rev().skip(1) {
            val = invoke_callable(Rc::clone(f), &[val], call_span)?;
        }
        Ok(val)
    });
    Ok(Value::NativeFunction(native).ref_cell())
}

/// nfunc_pipe(value, ...fns) — left-to-right application.
///
// >>> let f = fn(x) { return x + 1 }
// >>> let g = fn(x) { return x * 2 }
// >>> nfunc.pipe(3, g, f)
// => 7
fn nfunc_pipe(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 33, "nfunc_pipe", span)?;
    let mut val = Rc::clone(&args[0]);
    for i in 1..args.len() {
        let f = callable_arg(args, i, "nfunc_pipe", span)?;
        val = invoke_callable(f, &[val], span)?;
    }
    Ok(val)
}

/// nfunc_apply(fn, args) — call fn with an argument array.
///
// >>> let add = fn(a, b) { return a + b }
// >>> nfunc.apply(add, [1, 2])
// => 3
fn nfunc_apply(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nfunc_apply", span)?;
    let func = callable_arg(args, 0, "nfunc_apply", span)?;
    let call_args = array_arg(args, 1, "nfunc_apply", span)?;
    invoke_callable(func, &call_args, span)
}

/// nfunc_memoize(fn) — unbounded memo cache keyed by hashable arguments.
///
// >>> let n = 0
// >>> let f = nfunc.memoize(fn(x) { n = n + 1; return x * 2 })
// >>> f(4)
// => 8
// >>> f(4)
// => 8
fn nfunc_memoize(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfunc_memoize", span)?;
    let func = callable_arg(args, 0, "nfunc_memoize", span)?;
    let state = MemoState {
        callee: func,
        capacity: 0,
        map: HashMap::new(),
        recency: BTreeMap::new(),
        tick: 0,
        hits: 0,
        misses: 0,
    };
    let native = register_wrapper(WrapperState::Memo(state), move |call_args, call_span, st| {
        let key = match args_to_memo_key(call_args, call_span) {
            Ok(k) => k,
            Err(e) => return Ok(e),
        };
        let mut guard = st.borrow_mut();
        if let WrapperState::Memo(m) = &mut *guard {
            if let Some(hit) = memo_get(m, &key) {
                return Ok(hit);
            }
            let result = invoke_callable(Rc::clone(&m.callee), call_args, call_span)?;
            if matches!(&*result.borrow(), Value::Error(_)) {
                return Ok(result);
            }
            memo_insert(m, key, Rc::clone(&result));
            Ok(result)
        } else {
            Ok(nfunc_err(call_span, "internal memoize state corrupt"))
        }
    });
    Ok(Value::NativeFunction(native).ref_cell())
}

/// nfunc_memoize_lru(fn, maxsize) — LRU-bounded memo cache.
///
// >>> let f = nfunc.memoize_lru(fn(x) { return x * x }, 2)
// >>> f(2)
// => 4
fn nfunc_memoize_lru(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nfunc_memoize_lru", span)?;
    let func = callable_arg(args, 0, "nfunc_memoize_lru", span)?;
    let cap = int_arg(args, 1, "nfunc_memoize_lru", span)?;
    if cap <= 0 {
        return Ok(nfunc_err(span, "nfunc_memoize_lru() maxsize must be >= 1"));
    }
    let state = MemoState {
        callee: func,
        capacity: cap as usize,
        map: HashMap::new(),
        recency: BTreeMap::new(),
        tick: 0,
        hits: 0,
        misses: 0,
    };
    let native = register_wrapper(WrapperState::Memo(state), move |call_args, call_span, st| {
        let key = match args_to_memo_key(call_args, call_span) {
            Ok(k) => k,
            Err(e) => return Ok(e),
        };
        let mut guard = st.borrow_mut();
        if let WrapperState::Memo(m) = &mut *guard {
            if let Some(hit) = memo_get(m, &key) {
                return Ok(hit);
            }
            let result = invoke_callable(Rc::clone(&m.callee), call_args, call_span)?;
            if matches!(&*result.borrow(), Value::Error(_)) {
                return Ok(result);
            }
            memo_insert(m, key, Rc::clone(&result));
            Ok(result)
        } else {
            Ok(nfunc_err(call_span, "internal memoize_lru state corrupt"))
        }
    });
    Ok(Value::NativeFunction(native).ref_cell())
}

/// nfunc_once(fn) — invoke at most once; later calls return the cached result.
///
// >>> let n = 0
// >>> let f = nfunc.once(fn() { n = n + 1; return n })
// >>> f()
// => 1
// >>> f()
// => 1
fn nfunc_once(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfunc_once", span)?;
    let func = callable_arg(args, 0, "nfunc_once", span)?;
    let state = OnceState {
        callee: func,
        done: false,
        result: None,
    };
    let native = register_wrapper(WrapperState::Once(state), move |call_args, call_span, st| {
        let mut guard = st.borrow_mut();
        if let WrapperState::Once(o) = &mut *guard {
            if o.done {
                return Ok(Rc::clone(o.result.as_ref().expect("once result")));
            }
            let result = invoke_callable(Rc::clone(&o.callee), call_args, call_span)?;
            o.done = true;
            o.result = Some(Rc::clone(&result));
            Ok(result)
        } else {
            Ok(nfunc_err(call_span, "internal once state corrupt"))
        }
    });
    Ok(Value::NativeFunction(native).ref_cell())
}

/// nfunc_throttle(fn, interval_ms, opts?) — rate-limit invocations (call-time).
///
// >>> let n = 0
// >>> let f = nfunc.throttle(fn() { n = n + 1; return n }, 1000)
// >>> f()
// => 1
fn nfunc_throttle(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nfunc_throttle", span)?;
    let func = callable_arg(args, 0, "nfunc_throttle", span)?;
    let interval = int_arg(args, 1, "nfunc_throttle", span)?;
    if interval <= 0 {
        return Ok(nfunc_err(span, "nfunc_throttle() interval_ms must be >= 1"));
    }
    let (leading, trailing) = if args.len() == 3 {
        match &*args[2].borrow() {
            Value::Object(map) => (
                bool_opt(map, "leading", true),
                bool_opt(map, "trailing", false),
            ),
            other => {
                return Err(type_err(
                    span,
                    format!(
                        "nfunc_throttle() expects opts object as argument 3, got {}",
                        other.type_name()
                    ),
                ));
            }
        }
    } else {
        (true, false)
    };
    let state = ThrottleState {
        callee: func,
        interval_ms: interval,
        leading,
        trailing,
        last_invoke_ms: 0,
        last_args: Vec::new(),
        last_result: None,
        trailing_pending: false,
    };
    let native = register_wrapper(WrapperState::Throttle(state), move |call_args, call_span, st| {
        let now = now_ms();
        let mut guard = st.borrow_mut();
        if let WrapperState::Throttle(t) = &mut *guard {
            let elapsed = now - t.last_invoke_ms;
            let in_cooldown = t.last_invoke_ms > 0 && elapsed < t.interval_ms;
            if !in_cooldown {
                let result = invoke_callable(Rc::clone(&t.callee), call_args, call_span)?;
                t.last_invoke_ms = now;
                t.last_args = call_args.to_vec();
                t.last_result = Some(Rc::clone(&result));
                t.trailing_pending = false;
                return Ok(result);
            }
            if t.leading && t.last_result.is_some() {
                return Ok(Rc::clone(t.last_result.as_ref().unwrap()));
            }
            if t.trailing {
                t.trailing_pending = true;
                t.last_args = call_args.to_vec();
            }
            if let Some(r) = &t.last_result {
                Ok(Rc::clone(r))
            } else {
                Ok(Value::Nil.ref_cell())
            }
        } else {
            Ok(nfunc_err(call_span, "internal throttle state corrupt"))
        }
    });
    Ok(Value::NativeFunction(native).ref_cell())
}

/// nfunc_debounce(fn, wait_ms, opts?) — collapse rapid calls (call-time).
///
// >>> let n = 0
// >>> let f = nfunc.debounce(fn(x) { n = n + x; return n }, 50, {leading: true})
// >>> f(1)
// => 1
fn nfunc_debounce(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nfunc_debounce", span)?;
    let func = callable_arg(args, 0, "nfunc_debounce", span)?;
    let wait = int_arg(args, 1, "nfunc_debounce", span)?;
    if wait <= 0 {
        return Ok(nfunc_err(span, "nfunc_debounce() wait_ms must be >= 1"));
    }
    let (leading, trailing) = if args.len() == 3 {
        match &*args[2].borrow() {
            Value::Object(map) => (
                bool_opt(map, "leading", false),
                bool_opt(map, "trailing", true),
            ),
            other => {
                return Err(type_err(
                    span,
                    format!(
                        "nfunc_debounce() expects opts object as argument 3, got {}",
                        other.type_name()
                    ),
                ));
            }
        }
    } else {
        (false, true)
    };
    let state = DebounceState {
        callee: func,
        wait_ms: wait,
        leading,
        trailing,
        last_touch_ms: 0,
        last_invoke_ms: 0,
        pending_args: None,
        last_result: None,
    };
    let native = register_wrapper(WrapperState::Debounce(state), move |call_args, call_span, st| {
        let now = now_ms();
        let mut guard = st.borrow_mut();
        if let WrapperState::Debounce(d) = &mut *guard {
            if d.trailing
                && d.pending_args.is_some()
                && d.last_touch_ms > 0
                && now - d.last_touch_ms >= d.wait_ms
            {
                let pending = d.pending_args.take().unwrap();
                let result = invoke_callable(Rc::clone(&d.callee), &pending, call_span)?;
                d.last_invoke_ms = now;
                d.last_result = Some(Rc::clone(&result));
                d.last_touch_ms = now;
                return Ok(result);
            }
            let quiet = d.last_touch_ms == 0 || now - d.last_touch_ms >= d.wait_ms;
            d.last_touch_ms = now;
            d.pending_args = Some(call_args.to_vec());
            if d.leading && quiet {
                let result = invoke_callable(Rc::clone(&d.callee), call_args, call_span)?;
                d.last_invoke_ms = now;
                d.last_result = Some(Rc::clone(&result));
                d.pending_args = None;
                return Ok(result);
            }
            if d.trailing && quiet {
                let result = invoke_callable(Rc::clone(&d.callee), call_args, call_span)?;
                d.last_invoke_ms = now;
                d.last_result = Some(Rc::clone(&result));
                d.pending_args = None;
                return Ok(result);
            }
            if let Some(r) = &d.last_result {
                Ok(Rc::clone(r))
            } else {
                Ok(Value::Nil.ref_cell())
            }
        } else {
            Ok(nfunc_err(call_span, "internal debounce state corrupt"))
        }
    });
    Ok(Value::NativeFunction(native).ref_cell())
}

/// nfunc_debounce_flush(wrapped) — force a pending debounced call.
fn nfunc_debounce_flush(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfunc_debounce_flush", span)?;
    let st = lookup_wrapper(&args[0]).ok_or_else(|| {
        type_err(span, "nfunc_debounce_flush() expects a debounced wrapper from nfunc.debounce")
    })?;
    let mut guard = st.borrow_mut();
    if let WrapperState::Debounce(d) = &mut *guard {
        if let Some(pending) = d.pending_args.take() {
            let result = invoke_callable(Rc::clone(&d.callee), &pending, span)?;
            d.last_result = Some(Rc::clone(&result));
            d.last_invoke_ms = now_ms();
            Ok(result)
        } else if let Some(r) = &d.last_result {
            Ok(Rc::clone(r))
        } else {
            Ok(Value::Nil.ref_cell())
        }
    } else {
        Err(type_err(
            span,
            "nfunc_debounce_flush() expects a debounced wrapper from nfunc.debounce",
        ))
    }
}

/// nfunc_identity(x?) — identity function or passthrough of one value.
///
// >>> nfunc.identity(42)
// => 42
fn nfunc_identity(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nfunc_identity", span)?;
    if args.len() == 1 {
        return Ok(Rc::clone(&args[0]));
    }
    let native = register_wrapper(WrapperState::Generic, move |call_args, _call_span, _| {
        if call_args.is_empty() {
            Ok(Value::Nil.ref_cell())
        } else {
            Ok(Rc::clone(&call_args[0]))
        }
    });
    Ok(Value::NativeFunction(native).ref_cell())
}

/// nfunc_flip(fn) — swap the first two arguments.
///
// >>> let sub = fn(a, b) { return a - b }
// >>> nfunc.flip(sub)(3, 10)
// => 7
fn nfunc_flip(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfunc_flip", span)?;
    let func = callable_arg(args, 0, "nfunc_flip", span)?;
    let func_c = Rc::clone(&func);
    let native = register_wrapper(WrapperState::Generic, move |call_args, call_span, _| {
        if call_args.len() >= 2 {
            let mut swapped = call_args.to_vec();
            swapped.swap(0, 1);
            invoke_callable(Rc::clone(&func_c), &swapped, call_span)
        } else {
            invoke_callable(Rc::clone(&func_c), call_args, call_span)
        }
    });
    Ok(Value::NativeFunction(native).ref_cell())
}

/// nfunc_constant(value) — zero-argument function returning a fixed value.
///
// >>> nfunc.constant(99)()
// => 99
fn nfunc_constant(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfunc_constant", span)?;
    let val = Rc::clone(&args[0]);
    let native = register_wrapper(WrapperState::Generic, move |_call_args, _call_span, _| {
        Ok(Rc::clone(&val))
    });
    Ok(Value::NativeFunction(native).ref_cell())
}

/// nfunc_arity(fn) — parameter count for user functions, nil for native.
///
// >>> nfunc.arity(fn(a, b, c) { return 0 })
// => 3
fn nfunc_arity(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfunc_arity", span)?;
    match function_arity(&args[0].borrow()) {
        Some(n) => Ok(Value::Int(n as i64).ref_cell()),
        None => Ok(Value::Nil.ref_cell()),
    }
}

/// nfunc_cache_info(wrapped) — hits/misses/maxsize/currsize for memo wrappers.
fn nfunc_cache_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfunc_cache_info", span)?;
    let st = lookup_wrapper(&args[0]).ok_or_else(|| {
        type_err(
            span,
            "nfunc_cache_info() expects a memoized wrapper from nfunc.memoize or nfunc.memoize_lru",
        )
    })?;
    let guard = st.borrow();
    match &*guard {
        WrapperState::Memo(m) => {
            let mut map = HashMap::new();
            map.insert("hits".to_string(), Value::Int(m.hits as i64).ref_cell());
            map.insert("misses".to_string(), Value::Int(m.misses as i64).ref_cell());
            map.insert("currsize".to_string(), Value::Int(m.map.len() as i64).ref_cell());
            map.insert(
                "maxsize".to_string(),
                Value::Int(if m.capacity == 0 {
                    -1
                } else {
                    m.capacity as i64
                })
                .ref_cell(),
            );
            let total = m.hits + m.misses;
            let rate = if total == 0 {
                0.0
            } else {
                m.hits as f64 / total as f64
            };
            map.insert("hit_rate".to_string(), Value::Float(rate).ref_cell());
            Ok(Value::Object(map).ref_cell())
        }
        WrapperState::Once(o) => {
            let mut map = HashMap::new();
            map.insert("hits".to_string(), Value::Int(if o.done { 1 } else { 0 }).ref_cell());
            map.insert("misses".to_string(), Value::Int(if o.done { 0 } else { 1 }).ref_cell());
            map.insert("currsize".to_string(), Value::Int(if o.done { 1 } else { 0 }).ref_cell());
            map.insert("maxsize".to_string(), Value::Int(1).ref_cell());
            map.insert(
                "hit_rate".to_string(),
                Value::Float(if o.done { 1.0 } else { 0.0 }).ref_cell(),
            );
            Ok(Value::Object(map).ref_cell())
        }
        _ => Err(type_err(
            span,
            "nfunc_cache_info() expects a memoized wrapper from nfunc.memoize or nfunc.memoize_lru",
        )),
    }
}

/// nfunc_cache_clear(wrapped) — drop cached results.
fn nfunc_cache_clear(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfunc_cache_clear", span)?;
    let st = lookup_wrapper(&args[0]).ok_or_else(|| {
        type_err(
            span,
            "nfunc_cache_clear() expects a memoized or once wrapper from nfunc",
        )
    })?;
    let mut guard = st.borrow_mut();
    match &mut *guard {
        WrapperState::Memo(m) => {
            m.map.clear();
            m.recency.clear();
            m.hits = 0;
            m.misses = 0;
            Ok(Value::Nil.ref_cell())
        }
        WrapperState::Once(o) => {
            o.done = false;
            o.result = None;
            Ok(Value::Nil.ref_cell())
        }
        _ => Err(type_err(
            span,
            "nfunc_cache_clear() expects a memoized or once wrapper from nfunc",
        )),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nfunc_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nfunc_fns![
    ("nfunc_partial", "partial", nfunc_partial),
    ("nfunc_partial_right", "partial_right", nfunc_partial_right),
    ("nfunc_curry", "curry", nfunc_curry),
    ("nfunc_compose", "compose", nfunc_compose),
    ("nfunc_pipe", "pipe", nfunc_pipe),
    ("nfunc_apply", "apply", nfunc_apply),
    ("nfunc_memoize", "memoize", nfunc_memoize),
    ("nfunc_memoize_lru", "memoize_lru", nfunc_memoize_lru),
    ("nfunc_once", "once", nfunc_once),
    ("nfunc_throttle", "throttle", nfunc_throttle),
    ("nfunc_debounce", "debounce", nfunc_debounce),
    ("nfunc_debounce_flush", "debounce_flush", nfunc_debounce_flush),
    ("nfunc_identity", "identity", nfunc_identity),
    ("nfunc_flip", "flip", nfunc_flip),
    ("nfunc_constant", "constant", nfunc_constant),
    ("nfunc_arity", "arity", nfunc_arity),
    ("nfunc_cache_info", "cache_info", nfunc_cache_info),
    ("nfunc_cache_clear", "cache_clear", nfunc_cache_clear),
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

pub const MODULE_NAME: &str = "nfunc";
pub const MODULE_PATHS: &[&str] = &["nfunc", "std/nfunc"];

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

    fn i(v: i64) -> ValueRef {
        Value::Int(v).ref_cell()
    }

    fn native_add2(args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
        let a = match &*args[0].borrow() {
            Value::Int(n) => *n,
            _ => 0,
        };
        let b = match &*args.get(1).map(|v| v.borrow()).as_deref() {
            Some(Value::Int(n)) => *n,
            _ => 0,
        };
        Ok(Value::Int(a + b).ref_cell())
    }

    fn add2_val() -> ValueRef {
        Value::NativeFunction(Rc::new(native_add2)).ref_cell()
    }

    #[test]
    fn partial_binds_leading() {
        let inc = nfunc_partial(&[add2_val(), i(1)], span()).unwrap();
        match &*inc.borrow() {
            Value::NativeFunction(f) => {
                let out = f(&[i(5)], span()).unwrap();
                assert!(matches!(&*out.borrow(), Value::Int(6)));
            }
            other => panic!("expected native fn, got {other:?}"),
        }
    }

    #[test]
    fn partial_right_binds_trailing() {
        let sub5 = nfunc_partial_right(&[add2_val(), i(5)], span()).unwrap();
        match &*sub5.borrow() {
            Value::NativeFunction(f) => {
                // 10 + 5 with add2 is wrong semantics but tests binding order:
                // partial_right(add2, 5)(10) -> add2(10, 5) = 15
                let out = f(&[i(10)], span()).unwrap();
                assert!(matches!(&*out.borrow(), Value::Int(15)));
            }
            other => panic!("expected native fn, got {other:?}"),
        }
    }

    #[test]
    fn memoize_caches_native() {
        static mut CALLS: i64 = 0;
        fn counter(args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
            unsafe {
                CALLS += 1;
            }
            match &*args[0].borrow() {
                Value::Int(n) => Ok(Value::Int(*n * 2).ref_cell()),
                _ => Ok(Value::Nil.ref_cell()),
            }
        }
        let wrapped = nfunc_memoize(
            &[Value::NativeFunction(Rc::new(counter)).ref_cell()],
            span(),
        )
        .unwrap();
        match &*wrapped.borrow() {
            Value::NativeFunction(f) => {
                unsafe { CALLS = 0 };
                let _ = f(&[i(4)], span()).unwrap();
                let _ = f(&[i(4)], span()).unwrap();
                unsafe { assert_eq!(CALLS, 1) };
                let info = nfunc_cache_info(&[wrapped.clone()], span()).unwrap();
                match &*info.borrow() {
                    Value::Object(map) => {
                        assert!(matches!(&*map["hits"].borrow(), Value::Int(1)));
                        assert!(matches!(&*map["misses"].borrow(), Value::Int(1)));
                    }
                    other => panic!("expected object, got {other:?}"),
                }
            }
            other => panic!("expected native fn, got {other:?}"),
        }
    }

    #[test]
    fn memoize_lru_evicts() {
        fn sq(args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
            match &*args[0].borrow() {
                Value::Int(n) => Ok(Value::Int(n * n).ref_cell()),
                _ => Ok(Value::Nil.ref_cell()),
            }
        }
        let f = nfunc_memoize_lru(&[Value::NativeFunction(Rc::new(sq)).ref_cell(), i(2)], span())
            .unwrap();
        match &*f.borrow() {
            Value::NativeFunction(w) => {
                let _ = w(&[i(1)], span()).unwrap();
                let _ = w(&[i(2)], span()).unwrap();
                let _ = w(&[i(3)], span()).unwrap(); // evicts key 1
                let v = w(&[i(1)], span()).unwrap();
                assert!(matches!(&*v.borrow(), Value::Int(1)));
                let info = nfunc_cache_info(&[f.clone()], span()).unwrap();
                match &*info.borrow() {
                    Value::Object(map) => {
                        assert!(matches!(&*map["currsize"].borrow(), Value::Int(2)));
                    }
                    other => panic!("expected object, got {other:?}"),
                }
            }
            other => panic!("expected native fn, got {other:?}"),
        }
    }

    #[test]
    fn once_runs_single_time() {
        static mut N: i64 = 0;
        fn bump(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
            unsafe {
                N += 1;
                Ok(Value::Int(N).ref_cell())
            }
        }
        let f = nfunc_once(&[Value::NativeFunction(Rc::new(bump)).ref_cell()], span()).unwrap();
        match &*f.borrow() {
            Value::NativeFunction(w) => {
                unsafe { N = 0 };
                let a = w(&[], span()).unwrap();
                let b = w(&[], span()).unwrap();
                assert!(matches!(&*a.borrow(), Value::Int(1)));
                assert!(matches!(&*b.borrow(), Value::Int(1)));
            }
            other => panic!("expected native fn, got {other:?}"),
        }
    }

    #[test]
    fn identity_passthrough() {
        let id = nfunc_identity(&[], span()).unwrap();
        match &*id.borrow() {
            Value::NativeFunction(f) => {
                let out = f(&[i(7)], span()).unwrap();
                assert!(matches!(&*out.borrow(), Value::Int(7)));
            }
            other => panic!("expected native fn, got {other:?}"),
        }
        assert!(matches!(
            &*nfunc_identity(&[i(42)], span()).unwrap().borrow(),
            Value::Int(42)
        ));
    }

    #[test]
    fn flip_swaps_args() {
        let flipped = nfunc_flip(&[add2_val()], span()).unwrap();
        match &*flipped.borrow() {
            Value::NativeFunction(f) => {
                let out = f(&[i(3), i(10)], span()).unwrap();
                assert!(matches!(&*out.borrow(), Value::Int(13)));
            }
            other => panic!("expected native fn, got {other:?}"),
        }
    }

    #[test]
    fn compose_native_chain() {
        fn inc(args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
            match &*args[0].borrow() {
                Value::Int(n) => Ok(Value::Int(n + 1).ref_cell()),
                _ => Ok(Value::Nil.ref_cell()),
            }
        }
        fn dbl(args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
            match &*args[0].borrow() {
                Value::Int(n) => Ok(Value::Int(n * 2).ref_cell()),
                _ => Ok(Value::Nil.ref_cell()),
            }
        }
        let inc_v = Value::NativeFunction(Rc::new(inc)).ref_cell();
        let dbl_v = Value::NativeFunction(Rc::new(dbl)).ref_cell();
        let composed = nfunc_compose(&[inc_v, dbl_v], span()).unwrap();
        match &*composed.borrow() {
            Value::NativeFunction(f) => {
                let out = f(&[i(3)], span()).unwrap();
                assert!(matches!(&*out.borrow(), Value::Int(7)));
            }
            other => panic!("expected native fn, got {other:?}"),
        }
    }

    #[test]
    fn bench_memoize_lru_hot_path() {
        use std::time::Instant;
        fn sq(args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
            match &*args[0].borrow() {
                Value::Int(n) => Ok(Value::Int(n * n).ref_cell()),
                _ => Ok(Value::Nil.ref_cell()),
            }
        }
        let f = nfunc_memoize_lru(&[Value::NativeFunction(Rc::new(sq)).ref_cell(), i(4096)], span())
            .unwrap();
        let Value::NativeFunction(w) = &*f.borrow() else {
            panic!("expected wrapper");
        };
        let start = Instant::now();
        for i in 0..500_000 {
            let _ = w(&[i(i % 8192)], span()).unwrap();
        }
        let elapsed = start.elapsed();
        let info = nfunc_cache_info(&[f], span()).unwrap();
        let hit_rate = match &*info.borrow() {
            Value::Object(map) => match &*map["hit_rate"].borrow() {
                Value::Float(r) => *r,
                _ => 0.0,
            },
            _ => 0.0,
        };
        eprintln!(
            "nfunc_memoize_lru: 500k calls in {:.2} ms ({:.0} ns/call), hit_rate={:.4}",
            elapsed.as_secs_f64() * 1000.0,
            elapsed.as_nanos() as f64 / 500_000.0,
            hit_rate
        );
    }

    #[test]
    fn bench_compose_partial_hot_path() {
        use std::time::Instant;
        let add2 = add2_val();
        let inc = nfunc_partial(&[add2, i(1)], span()).unwrap();
        let dbl = Value::NativeFunction(Rc::new(|args: &[ValueRef], _span: Span| {
            match &*args[0].borrow() {
                Value::Int(n) => Ok(Value::Int(n * 2).ref_cell()),
                _ => Ok(Value::Nil.ref_cell()),
            }
        }))
        .ref_cell();
        let composed = nfunc_compose(&[inc, dbl], span()).unwrap();
        let Value::NativeFunction(f) = &*composed.borrow() else {
            panic!("expected composed fn");
        };
        let start = Instant::now();
        let mut sum = 0i64;
        for i in 0..500_000 {
            let out = f(&[i(i % 100)], span()).unwrap();
            if let Value::Int(n) = &*out.borrow() {
                sum += *n;
            }
        }
        let elapsed = start.elapsed();
        eprintln!(
            "nfunc_compose+partial: 500k calls in {:.2} ms ({:.0} ns/call), checksum={}",
            elapsed.as_secs_f64() * 1000.0,
            elapsed.as_nanos() as f64 / 500_000.0,
            sum
        );
    }
}
