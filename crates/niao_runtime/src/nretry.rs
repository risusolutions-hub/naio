//! Native nretry standard library — retry with exponential backoff, jitter,
//! deadlines, and retry-on predicates (~tenacity / backoff subset; complements
//! nfallback circuit breakers).
//!
//! Import with `import "nretry"` (or `import "std/nretry"`).

use crate::{call_niao_function, error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_rand::thread_rng;
use niao_retry::{
    apply_jitter, compute_wait_ms, deadline_exceeded, exponential_raw, should_stop_attempts,
    BackoffStrategy, JitterKind, RetryOutcome, RetryPolicy,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Policy registry
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct PolicyState {
    config: RetryPolicy,
    retry_on: Option<ValueRef>,
    stop_on: Option<ValueRef>,
    before: Option<ValueRef>,
    after: Option<ValueRef>,
    before_sleep: Option<ValueRef>,
}

struct PolicyEntry {
    id: i64,
    state: Rc<RefCell<PolicyState>>,
}

thread_local! {
    static POLICIES: RefCell<HashMap<i64, PolicyEntry>> = RefCell::new(HashMap::new());
    static NEXT_POLICY_ID: RefCell<i64> = const { RefCell::new(1) };
}

fn new_policy_id() -> i64 {
    NEXT_POLICY_ID.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    })
}

fn register_policy(state: PolicyState) -> i64 {
    let id = new_policy_id();
    POLICIES.with(|p| {
        p.borrow_mut().insert(
            id,
            PolicyEntry {
                id,
                state: Rc::new(RefCell::new(state)),
            },
        );
    });
    id
}

fn lookup_policy(id: i64) -> Option<Rc<RefCell<PolicyState>>> {
    POLICIES.with(|p| p.borrow().get(&id).map(|e| Rc::clone(&e.state)))
}

// ---------------------------------------------------------------------------
// Time + invocation
// ---------------------------------------------------------------------------

#[inline]
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn invoke_callable(callee: &ValueRef, args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    match &*callee.borrow() {
        Value::NativeFunction(native) => native(args, span),
        Value::Function(_) => call_niao_function(Rc::clone(callee), args, span),
        other => Err(type_err(
            span,
            format!("expected callable, got {}", other.type_name()),
        )),
    }
}

fn invoke_bool_pred(
    pred: &ValueRef,
    args: &[ValueRef],
    span: Span,
    ctx: &str,
) -> NiaoResult<bool> {
    let out = invoke_callable(pred, args, span)?;
    match &*out.borrow() {
        Value::Bool(b) => Ok(*b),
        other => Err(type_err(
            span,
            format!("{ctx} predicate must return bool, got {}", other.type_name()),
        )),
    }
}

fn is_error_value(v: &ValueRef) -> bool {
    matches!(&*v.borrow(), Value::Error(_))
}

fn is_nil_value(v: &ValueRef) -> bool {
    matches!(&*v.borrow(), Value::Nil)
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E3515_NRETRY_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E3513_NRETRY_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E3513_NRETRY_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn nretry_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E3514_NRETRY_ERROR, "nretry_error", msg.into(), span)
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

fn float_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<f64> {
    match &*args[idx].borrow() {
        Value::Float(f) => Ok(*f),
        Value::Int(n) => Ok(*n as f64),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a number as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}


fn object_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<HashMap<String, ValueRef>> {
    match &*args[idx].borrow() {
        Value::Object(map) => Ok(map.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an object as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn opt_i64(map: &HashMap<String, ValueRef>, keys: &[&str], span: Span) -> NiaoResult<Option<i64>> {
    for key in keys {
        if let Some(v) = map.get(*key) {
            return match &*v.borrow() {
                Value::Nil => Ok(None),
                Value::Int(n) => Ok(Some(*n)),
                other => Err(type_err(
                    span,
                    format!("opts '{key}' expects int, got {}", other.type_name()),
                )),
            };
        }
    }
    Ok(None)
}

fn opt_f64(map: &HashMap<String, ValueRef>, keys: &[&str], span: Span) -> NiaoResult<Option<f64>> {
    for key in keys {
        if let Some(v) = map.get(*key) {
            return match &*v.borrow() {
                Value::Nil => Ok(None),
                Value::Float(f) => Ok(Some(*f)),
                Value::Int(n) => Ok(Some(*n as f64)),
                other => Err(type_err(
                    span,
                    format!("opts '{key}' expects number, got {}", other.type_name()),
                )),
            };
        }
    }
    Ok(None)
}

fn opt_bool(map: &HashMap<String, ValueRef>, keys: &[&str], default: bool) -> bool {
    for key in keys {
        if let Some(v) = map.get(*key) {
            return match &*v.borrow() {
                Value::Bool(b) => *b,
                _ => default,
            };
        }
    }
    default
}

fn opt_string(map: &HashMap<String, ValueRef>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(v) = map.get(*key) {
            if let Value::String(s) = &*v.borrow() {
                return Some(s.clone());
            }
        }
    }
    None
}

fn opt_callable(map: &HashMap<String, ValueRef>, keys: &[&str]) -> Option<ValueRef> {
    for key in keys {
        if let Some(v) = map.get(*key) {
            match &*v.borrow() {
                Value::Function(_) | Value::NativeFunction(_) => return Some(Rc::clone(v)),
                Value::Nil => return None,
                _ => {}
            }
        }
    }
    None
}

fn parse_policy_from_map(map: &HashMap<String, ValueRef>, span: Span) -> Result<PolicyState, ValueRef> {
    let mut config = RetryPolicy::default();

    if let Some(n) = opt_i64(map, &["attempts", "max_attempts", "stop_after_attempt"], span)
        .map_err(|e| error_value_from_runtime(e, span))?
    {
        if n < 0 {
            return Err(nretry_err(span, "attempts must be >= 0 (0 = unlimited)"));
        }
        config.max_attempts = n as u32;
    }

    if let Some(n) = opt_i64(map, &["min_wait_ms", "wait_ms", "wait", "base_ms"], span)
        .map_err(|e| error_value_from_runtime(e, span))?
    {
        if n < 0 {
            return Err(nretry_err(span, "min_wait_ms must be >= 0"));
        }
        config.min_wait_ms = n as u64;
    }

    if let Some(n) = opt_i64(map, &["max_wait_ms", "max_wait", "cap_ms"], span)
        .map_err(|e| error_value_from_runtime(e, span))?
    {
        if n < 0 {
            return Err(nretry_err(span, "max_wait_ms must be >= 0"));
        }
        config.max_wait_ms = n as u64;
    }

    if let Some(m) = opt_f64(map, &["multiplier", "exp_multiplier", "factor"], span)
        .map_err(|e| error_value_from_runtime(e, span))?
    {
        if m < 1.0 {
            return Err(nretry_err(span, "multiplier must be >= 1.0"));
        }
        config.multiplier = m;
    }

    if let Some(s) = opt_string(map, &["strategy", "backoff", "wait_strategy"]) {
        config.strategy = BackoffStrategy::parse(&s).ok_or_else(|| {
            nretry_err(
                span,
                format!(
                    "unknown strategy '{s}' (fixed, exponential, random_exponential, decorrelated)"
                ),
            )
        })?;
    }

    if let Some(s) = opt_string(map, &["jitter", "jitter_strategy"]) {
        config.jitter = JitterKind::parse(&s).ok_or_else(|| {
            nretry_err(span, format!("unknown jitter '{s}' (none, full, equal, decorrelated)"))
        })?;
    }

    if let Some(n) = opt_i64(
        map,
        &["deadline_ms", "timeout_ms", "stop_after_delay_ms", "max_delay_ms"],
        span,
    )
    .map_err(|e| error_value_from_runtime(e, span))?
    {
        if n < 0 {
            return Err(nretry_err(span, "deadline_ms must be >= 0"));
        }
        config.deadline_ms = if n == 0 { None } else { Some(n as u64) };
    }

    config.retry_on_error = opt_bool(map, &["retry_on_error", "reraise_on_error"], true);
    config.retry_on_nil = opt_bool(map, &["retry_on_nil"], false);
    config.sleep = opt_bool(map, &["sleep", "do_sleep"], true);

    if let Err(msg) = config.validate() {
        return Err(nretry_err(span, msg));
    }

    Ok(PolicyState {
        config,
        retry_on: opt_callable(map, &["retry_on", "retry_if", "retry_if_result"]),
        stop_on: opt_callable(map, &["stop_on", "stop_if", "retry_until"]),
        before: opt_callable(map, &["before", "before_attempt"]),
        after: opt_callable(map, &["after", "after_attempt"]),
        before_sleep: opt_callable(map, &["before_sleep", "before_wait"]),
    })
}

fn error_value_from_runtime(e: RuntimeError, span: Span) -> ValueRef {
    error_value(codes::E3515_NRETRY_TYPE, "nretry_error", e.message(), span)
}

fn policy_to_config_object(policy: &RetryPolicy) -> HashMap<String, ValueRef> {
    let mut map = HashMap::new();
    map.insert("attempts".into(), Value::Int(policy.max_attempts as i64).ref_cell());
    map.insert("min_wait_ms".into(), Value::Int(policy.min_wait_ms as i64).ref_cell());
    map.insert("max_wait_ms".into(), Value::Int(policy.max_wait_ms as i64).ref_cell());
    map.insert("multiplier".into(), Value::Float(policy.multiplier).ref_cell());
    map.insert(
        "strategy".into(),
        Value::String(policy.strategy.as_str().into()).ref_cell(),
    );
    map.insert(
        "jitter".into(),
        Value::String(policy.jitter.as_str().into()).ref_cell(),
    );
    map.insert(
        "deadline_ms".into(),
        match policy.deadline_ms {
            Some(ms) => Value::Int(ms as i64).ref_cell(),
            None => Value::Nil.ref_cell(),
        },
    );
    map.insert(
        "retry_on_error".into(),
        Value::Bool(policy.retry_on_error).ref_cell(),
    );
    map.insert("retry_on_nil".into(), Value::Bool(policy.retry_on_nil).ref_cell());
    map.insert("sleep".into(), Value::Bool(policy.sleep).ref_cell());
    map
}

fn policy_object(id: i64, policy: &RetryPolicy) -> ValueRef {
    let mut map = policy_to_config_object(policy);
    map.insert("id".into(), Value::Int(id).ref_cell());
    Value::Object(map).ref_cell()
}

fn stats_object(
    result: ValueRef,
    outcome: &RetryOutcome,
    ok: bool,
) -> HashMap<String, ValueRef> {
    let mut map = HashMap::new();
    map.insert("ok".into(), Value::Bool(ok).ref_cell());
    map.insert("result".into(), result);
    map.insert("attempts".into(), Value::Int(outcome.attempts as i64).ref_cell());
    map.insert("sleep_ms".into(), Value::Int(outcome.sleep_ms as i64).ref_cell());
    map.insert("elapsed_ms".into(), Value::Int(outcome.elapsed_ms as i64).ref_cell());
    map.insert(
        "stopped_by_deadline".into(),
        Value::Bool(outcome.stopped_by_deadline).ref_cell(),
    );
    map.insert(
        "stopped_by_attempts".into(),
        Value::Bool(outcome.stopped_by_attempts).ref_cell(),
    );
    map
}

// ---------------------------------------------------------------------------
// Retry engine
// ---------------------------------------------------------------------------

struct ExecResult {
    result: ValueRef,
    outcome: RetryOutcome,
    ok: bool,
}

fn should_retry_result(
    result: &ValueRef,
    attempt: u32,
    state: &PolicyState,
    span: Span,
) -> NiaoResult<bool> {
    if let Some(pred) = &state.retry_on {
        return invoke_bool_pred(
            pred,
            &[Rc::clone(result), Value::Int(attempt as i64).ref_cell()],
            span,
            "retry_on",
        );
    }
    if state.config.retry_on_error && is_error_value(result) {
        return Ok(true);
    }
    if state.config.retry_on_nil && is_nil_value(result) {
        return Ok(true);
    }
    Ok(false)
}

fn should_stop(
    result: &ValueRef,
    attempt: u32,
    start_ms: u64,
    state: &PolicyState,
    span: Span,
) -> NiaoResult<bool> {
    if let Some(pred) = &state.stop_on {
        if invoke_bool_pred(
            pred,
            &[Rc::clone(result), Value::Int(attempt as i64).ref_cell()],
            span,
            "stop_on",
        )? {
            return Ok(true);
        }
    }
    if should_stop_attempts(attempt, &state.config) {
        return Ok(true);
    }
    if deadline_exceeded(start_ms, now_ms(), state.config.deadline_ms) {
        return Ok(true);
    }
    Ok(false)
}

fn execute_retry(
    callee: ValueRef,
    callee_args: &[ValueRef],
    state: &PolicyState,
    span: Span,
) -> NiaoResult<ExecResult> {
    let start_ms = now_ms();
    let mut attempt = 0u32;
    let mut total_sleep = 0u64;
    let mut prev_wait = state.config.min_wait_ms;
    let mut last_result = Value::Nil.ref_cell();
    let mut stopped_by_deadline = false;
    let mut stopped_by_attempts = false;
    let mut rng = thread_rng();

    loop {
        attempt += 1;

        if let Some(before) = &state.before {
            invoke_callable(
                before,
                &[Value::Int(attempt as i64).ref_cell()],
                span,
            )?;
        }

        last_result = invoke_callable(&callee, callee_args, span)?;

        if let Some(after) = &state.after {
            invoke_callable(
                after,
                &[
                    Value::Int(attempt as i64).ref_cell(),
                    Rc::clone(&last_result),
                ],
                span,
            )?;
        }

        let retry = should_retry_result(&last_result, attempt, state, span)?;
        if !retry {
            break;
        }

        let stop = should_stop(&last_result, attempt, start_ms, state, span)?;
        if stop {
            stopped_by_attempts = should_stop_attempts(attempt, &state.config);
            stopped_by_deadline =
                deadline_exceeded(start_ms, now_ms(), state.config.deadline_ms);
            break;
        }

        let wait = compute_wait_ms(attempt, &state.config, prev_wait, &mut rng);
        prev_wait = wait;

        if state.config.sleep && wait > 0 {
            if let Some(bs) = &state.before_sleep {
                invoke_callable(
                    bs,
                    &[
                        Value::Int(attempt as i64).ref_cell(),
                        Value::Int(wait as i64).ref_cell(),
                        Rc::clone(&last_result),
                    ],
                    span,
                )?;
            }
            thread::sleep(std::time::Duration::from_millis(wait));
            total_sleep = total_sleep.saturating_add(wait);
        }

        if deadline_exceeded(start_ms, now_ms(), state.config.deadline_ms) {
            stopped_by_deadline = true;
            break;
        }
    }

    let elapsed = now_ms().saturating_sub(start_ms);
    let ok = !is_error_value(&last_result) && !is_nil_value(&last_result);
    let mut outcome = RetryOutcome::new(attempt, total_sleep, elapsed);
    outcome.stopped_by_deadline = stopped_by_deadline;
    outcome.stopped_by_attempts = stopped_by_attempts;

    Ok(ExecResult {
        result: last_result,
        outcome,
        ok,
    })
}

fn policy_from_opts_arg(args: &[ValueRef], idx: usize, span: Span) -> Result<PolicyState, ValueRef> {
    if args.len() <= idx {
        return Ok(PolicyState {
            config: RetryPolicy::default(),
            retry_on: None,
            stop_on: None,
            before: None,
            after: None,
            before_sleep: None,
        });
    }
    parse_policy_from_map(&object_arg(args, idx, "opts", span).map_err(|e| {
        error_value(codes::E3515_NRETRY_TYPE, "nretry_error", e.message(), span)
    })?, span)
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

// >>> nretry.is_error(err(1, "x", "boom"))
// => true
fn nretry_is_error(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nretry_is_error", span)?;
    Ok(Value::Bool(is_error_value(&args[0])).ref_cell())
}

// >>> nretry.is_nil(nil)
// => true
fn nretry_is_nil(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nretry_is_nil", span)?;
    Ok(Value::Bool(is_nil_value(&args[0])).ref_cell())
}

// >>> nretry.sleep(0)
// => nil
fn nretry_sleep(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nretry_sleep", span)?;
    let ms = int_arg(args, 0, "nretry_sleep", span)?;
    if ms < 0 {
        return Ok(nretry_err(span, "sleep() ms must be >= 0"));
    }
    if ms > 0 {
        thread::sleep(std::time::Duration::from_millis(ms as u64));
    }
    Ok(Value::Nil.ref_cell())
}

// >>> nretry.exponential(2, {min_wait_ms: 100, multiplier: 2, jitter: "none"})
// => 200
fn nretry_exponential(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nretry_exponential", span)?;
    let attempt = int_arg(args, 0, "nretry_exponential", span)?;
    if attempt < 1 {
        return Ok(nretry_err(span, "exponential() attempt must be >= 1"));
    }
    let state = policy_from_opts_arg(args, 1, span).map_err(|e| return Ok(e))?;
    let raw = exponential_raw(attempt as u32, &state.config);
    Ok(Value::Int(raw as i64).ref_cell())
}

// >>> nretry.backoff(1)
// => 500
fn nretry_backoff(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nretry_backoff", span)?;
    let attempt = int_arg(args, 0, "nretry_backoff", span)?;
    if attempt < 1 {
        return Ok(nretry_err(span, "backoff() attempt must be >= 1"));
    }
    let state = policy_from_opts_arg(args, 1, span).map_err(|e| return Ok(e))?;
    let mut rng = thread_rng();
    let wait = compute_wait_ms(attempt as u32, &state.config, state.config.min_wait_ms, &mut rng);
    Ok(Value::Int(wait as i64).ref_cell())
}

// >>> nretry.jitter(1000, "full") >= 0
// => true
fn nretry_jitter(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nretry_jitter", span)?;
    let wait = int_arg(args, 0, "nretry_jitter", span)?;
    if wait < 0 {
        return Ok(nretry_err(span, "jitter() wait_ms must be >= 0"));
    }
    let jitter = if args.len() > 1 {
        match &*args[1].borrow() {
            Value::String(s) => JitterKind::parse(s).ok_or_else(|| {
                type_err(span, format!("unknown jitter kind '{s}'"))
            })?,
            other => {
                return Err(type_err(
                    span,
                    format!(
                        "nretry_jitter() expects string kind, got {}",
                        other.type_name()
                    ),
                ));
            }
        }
    } else {
        JitterKind::Full
    };
    let policy = RetryPolicy::default();
    let mut rng = thread_rng();
    let out = apply_jitter(wait as u64, jitter, &policy, wait as u64, &mut rng);
    Ok(Value::Int(out as i64).ref_cell())
}

// >>> nretry.default_opts().attempts
// => 3
fn nretry_default_opts(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nretry_default_opts", span)?;
    Ok(Value::Object(policy_to_config_object(&RetryPolicy::default())).ref_cell())
}

// >>> nretry.validate({attempts: 2})
// => true
fn nretry_validate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nretry_validate", span)?;
    let map = object_arg(args, 0, "nretry_validate", span)?;
    match parse_policy_from_map(&map, span) {
        Ok(_) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(e),
    }
}

// >>> nretry.policy({attempts: 2}).attempts
// => 2
fn nretry_policy(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nretry_policy", span)?;
    let state = if args.is_empty() {
        PolicyState {
            config: RetryPolicy::default(),
            retry_on: None,
            stop_on: None,
            before: None,
            after: None,
            before_sleep: None,
        }
    } else {
        parse_policy_from_map(&object_arg(args, 0, "nretry_policy", span)?, span)
            .map_err(|e| return Ok(e))?
    };
    let id = register_policy(state);
    let entry = lookup_policy(id).expect("just registered");
    let config = entry.borrow().config.clone();
    Ok(policy_object(id, &config))
}

// >>> nretry.call(fn() { return 1 })
// => 1
fn nretry_call(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nretry_call", span)?;
    let callee = callable_arg(args, 0, "nretry_call", span)?;
    let state = policy_from_opts_arg(args, 1, span).map_err(|e| return Ok(e))?;
    let exec = execute_retry(callee, &[], state, span)?;
    Ok(exec.result)
}

// >>> nretry.call_ex(fn() { return 1 }).ok
// => true
fn nretry_call_ex(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nretry_call_ex", span)?;
    let callee = callable_arg(args, 0, "nretry_call_ex", span)?;
    let state = policy_from_opts_arg(args, 1, span).map_err(|e| return Ok(e))?;
    let exec = execute_retry(callee, &[], state, span)?;
    Ok(Value::Object(stats_object(exec.result, &exec.outcome, exec.ok)).ref_cell())
}

// >>> nretry.wrap(fn() { return 1 })()( )
// => 1
fn nretry_wrap(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nretry_wrap", span)?;
    let callee = callable_arg(args, 0, "nretry_wrap", span)?;
    let state = policy_from_opts_arg(args, 1, span).map_err(|e| return Ok(e))?;
    let state = Rc::new(RefCell::new(state));
    let inner = Rc::clone(&callee);
    let state_for_fn = Rc::clone(&state);
    let native: NativeFn = Rc::new(move |fargs, fspan| {
        let st = state_for_fn.borrow();
        let exec = execute_retry(Rc::clone(&inner), fargs, &st, fspan)?;
        Ok(exec.result)
    });
    Ok(Value::NativeFunction(native).ref_cell())
}

// >>> nretry.policy_call({attempts: 1}, fn() { return 9 })
// => 9
fn nretry_policy_call(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nretry_policy_call", span)?;
    let map = object_arg(args, 0, "nretry_policy_call", span)?;
    if let Some(id_v) = map.get("id") {
        if let Value::Int(id) = &*id_v.borrow() {
            if let Some(state) = lookup_policy(*id) {
                let callee = callable_arg(args, 1, "nretry_policy_call", span)?;
                let exec = execute_retry(callee, &[], state.borrow(), span)?;
                return Ok(exec.result);
            }
        }
    }
    let state = parse_policy_from_map(&map, span).map_err(|e| return Ok(e))?;
    let callee = callable_arg(args, 1, "nretry_policy_call", span)?;
    let exec = execute_retry(callee, &[], state, span)?;
    Ok(exec.result)
}

// >>> nretry.policy_wrap({attempts: 2}, fn(x) { return x })(7)
// => 7
fn nretry_policy_wrap(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nretry_policy_wrap", span)?;
    let map = object_arg(args, 0, "nretry_policy_wrap", span)?;
    let callee = callable_arg(args, 1, "nretry_policy_wrap", span)?;
    let state = if let Some(id_v) = map.get("id") {
        if let Value::Int(id) = &*id_v.borrow() {
            if let Some(st) = lookup_policy(*id) {
                st.borrow().clone()
            } else {
                return Ok(nretry_err(span, format!("unknown policy id {id}")));
            }
        } else {
            parse_policy_from_map(&map, span).map_err(|e| return Ok(e))?
        }
    } else {
        parse_policy_from_map(&map, span).map_err(|e| return Ok(e))?
    };
    let state = Rc::new(RefCell::new(state));
    let inner = Rc::clone(&callee);
    let state_for_fn = Rc::clone(&state);
    let native: NativeFn = Rc::new(move |fargs, fspan| {
        let st = state_for_fn.borrow();
        let exec = execute_retry(Rc::clone(&inner), fargs, &st, fspan)?;
        Ok(exec.result)
    });
    Ok(Value::NativeFunction(native).ref_cell())
}

// >>> nretry.attempts_left(2, {attempts: 5})
// => 3
fn nretry_attempts_left(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nretry_attempts_left", span)?;
    let used = int_arg(args, 0, "nretry_attempts_left", span)?;
    if used < 0 {
        return Ok(nretry_err(span, "attempts_left() used must be >= 0"));
    }
    let max = if args.len() > 1 {
        let state = parse_policy_from_map(&object_arg(args, 1, "nretry_attempts_left", span)?, span)
            .map_err(|e| return Ok(e))?;
        state.config.max_attempts as i64
    } else {
        RetryPolicy::default().max_attempts as i64
    };
    Ok(Value::Int((max - used).max(0)).ref_cell())
}

// >>> nretry.stop_after_attempt(3)
// => {stop_after_attempt: 3}
fn nretry_stop_after_attempt(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nretry_stop_after_attempt", span)?;
    let n = int_arg(args, 0, "nretry_stop_after_attempt", span)?;
    if n < 1 {
        return Ok(nretry_err(span, "stop_after_attempt() must be >= 1"));
    }
    let mut map = HashMap::new();
    map.insert("attempts".into(), Value::Int(n).ref_cell());
    Ok(Value::Object(map).ref_cell())
}

// >>> nretry.stop_after_delay(5000)
// => {deadline_ms: 5000}
fn nretry_stop_after_delay(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nretry_stop_after_delay", span)?;
    let ms = int_arg(args, 0, "nretry_stop_after_delay", span)?;
    if ms < 0 {
        return Ok(nretry_err(span, "stop_after_delay() must be >= 0"));
    }
    let mut map = HashMap::new();
    map.insert("deadline_ms".into(), Value::Int(ms).ref_cell());
    Ok(Value::Object(map).ref_cell())
}

// >>> nretry.merge_opts(nretry.stop_after_attempt(2), {jitter: "none"})
fn nretry_merge_opts(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 16, "nretry_merge_opts", span)?;
    let mut merged = policy_to_config_object(&RetryPolicy::default());
    for (i, arg) in args.iter().enumerate() {
        match &*arg.borrow() {
            Value::Object(map) => {
                for (k, v) in map {
                    merged.insert(k.clone(), Rc::clone(v));
                }
            }
            other => {
                return Err(type_err(
                    span,
                    format!(
                        "nretry_merge_opts() argument {} expects object, got {}",
                        i + 1,
                        other.type_name()
                    ),
                ));
            }
        }
    }
    Ok(Value::Object(merged).ref_cell())
}

// >>> nretry.stop_never()
// => {attempts: 0}
fn nretry_stop_never(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nretry_stop_never", span)?;
    let mut map = HashMap::new();
    map.insert("attempts".into(), Value::Int(0).ref_cell());
    Ok(Value::Object(map).ref_cell())
}

fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
    vec![
        ("nretry_call", "call", Rc::new(nretry_call)),
        ("nretry_call_ex", "call_ex", Rc::new(nretry_call_ex)),
        ("nretry_policy", "policy", Rc::new(nretry_policy)),
        ("nretry_policy_call", "policy_call", Rc::new(nretry_policy_call)),
        ("nretry_policy_wrap", "policy_wrap", Rc::new(nretry_policy_wrap)),
        ("nretry_wrap", "wrap", Rc::new(nretry_wrap)),
        ("nretry_backoff", "backoff", Rc::new(nretry_backoff)),
        ("nretry_exponential", "exponential", Rc::new(nretry_exponential)),
        ("nretry_jitter", "jitter", Rc::new(nretry_jitter)),
        ("nretry_sleep", "sleep", Rc::new(nretry_sleep)),
        ("nretry_is_error", "is_error", Rc::new(nretry_is_error)),
        ("nretry_is_nil", "is_nil", Rc::new(nretry_is_nil)),
        ("nretry_default_opts", "default_opts", Rc::new(nretry_default_opts)),
        ("nretry_validate", "validate", Rc::new(nretry_validate)),
        ("nretry_attempts_left", "attempts_left", Rc::new(nretry_attempts_left)),
        ("nretry_stop_after_attempt", "stop_after_attempt", Rc::new(nretry_stop_after_attempt)),
        ("nretry_stop_after_delay", "stop_after_delay", Rc::new(nretry_stop_after_delay)),
        ("nretry_stop_never", "stop_never", Rc::new(nretry_stop_never)),
        ("nretry_merge_opts", "merge_opts", Rc::new(nretry_merge_opts)),
    ]
}

pub const MODULE_NAME: &str = "nretry";
pub const MODULE_PATHS: &[&str] = &["nretry", "std/nretry"];

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

    #[test]
    fn default_opts_attempts() {
        let o = nretry_default_opts(&[], span()).unwrap();
        match &*o.borrow() {
            Value::Object(m) => match &*m["attempts"].borrow() {
                Value::Int(n) => assert_eq!(*n, 3),
                _ => panic!("bad attempts"),
            },
            _ => panic!("expected object"),
        }
    }

    #[test]
    fn backoff_positive() {
        let w = nretry_backoff(&[i(1)], span()).unwrap();
        assert!(matches!(&*w.borrow(), Value::Int(n) if *n > 0));
    }

    #[test]
    fn validate_ok() {
        let mut map = HashMap::new();
        map.insert("attempts".into(), i(2));
        let v = nretry_validate(&[Value::Object(map).ref_cell()], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Bool(true)));
    }

    #[test]
    fn merge_opts() {
        let a = nretry_stop_after_attempt(&[i(4)], span()).unwrap();
        let mut extra = HashMap::new();
        extra.insert("jitter".into(), Value::String("none".into()).ref_cell());
        let m = nretry_merge_opts(&[a, Value::Object(extra).ref_cell()], span()).unwrap();
        match &*m.borrow() {
            Value::Object(map) => {
                match &*map["attempts"].borrow() {
                    Value::Int(n) => assert_eq!(*n, 4),
                    _ => panic!("bad attempts"),
                }
                assert!(matches!(&*map["jitter"].borrow(), Value::String(s) if s == "none"));
            }
            _ => panic!("expected object"),
        }
    }
}
