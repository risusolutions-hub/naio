//! Native nsignal standard library — OS signal handlers, graceful-shutdown
//! patterns, and SIGTERM/SIGINT hooks (~Python `signal` stdlib subset).
//!
//! Handlers run when callers invoke `nsignal.poll()` (or blocking `wait` /
//! `pause`) from normal code; OS handlers only enqueue signal numbers.
//!
//! Import with `import "nsignal"` (or `import "std/nsignal"`).

use crate::{call_niao_function, error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_signal::{self, HandlerKind, SIG_DFL_SENTINEL, SIG_IGN_SENTINEL};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Per-process handler registry (Niao callables)
// ---------------------------------------------------------------------------

enum UserHandler {
    Callable(ValueRef),
    Ignore,
    Default,
}

struct ShutdownGuard {
    signals: Vec<i32>,
    handler: ValueRef,
}

thread_local! {
    static USER_HANDLERS: RefCell<HashMap<i32, UserHandler>> = RefCell::new(HashMap::new());
    static SHUTDOWN_GUARDS: RefCell<HashMap<i64, ShutdownGuard>> = RefCell::new(HashMap::new());
    static NEXT_GUARD: RefCell<i64> = const { RefCell::new(1) };
}

fn new_guard_id() -> i64 {
    NEXT_GUARD.with(|g| {
        let mut g = g.borrow_mut();
        let id = *g;
        *g += 1;
        id
    })
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E3482_NSIGNAL_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E3480_NSIGNAL_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E3480_NSIGNAL_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn nsignal_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E3481_NSIGNAL_ERROR, "nsignal_error", msg.into(), span)
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

fn optional_int(args: &[ValueRef], idx: usize, default: i64) -> i64 {
    if args.len() <= idx {
        return default;
    }
    match &*args[idx].borrow() {
        Value::Int(n) => *n,
        _ => default,
    }
}

fn callable_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<ValueRef> {
    let ok = matches!(
        &*args[idx].borrow(),
        Value::Function(_) | Value::NativeFunction(_)
    );
    if !ok {
        return Err(type_err(
            span,
            format!(
                "{name}() expects a function as argument {}, got {}",
                idx + 1,
                args[idx].borrow().type_name()
            ),
        ));
    }
    Ok(Rc::clone(&args[idx]))
}

fn array_val(items: Vec<ValueRef>) -> NiaoResult<ValueRef> {
    Ok(Value::Array(items).ref_cell())
}

fn bool_val(b: bool) -> NiaoResult<ValueRef> {
    Ok(Value::Bool(b).ref_cell())
}

fn nil_val() -> NiaoResult<ValueRef> {
    Ok(Value::Nil.ref_cell())
}

fn str_val(s: impl Into<String>) -> NiaoResult<ValueRef> {
    Ok(Value::String(s.into()).ref_cell())
}

/// Accept int signum or string name (`"SIGINT"`, `"int"`).
fn signal_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i32> {
    match &*args[idx].borrow() {
        Value::Int(n) => {
            let sig = *n as i32;
            if niao_signal::is_valid_signal(sig) {
                Ok(sig)
            } else {
                Err(type_err(
                    span,
                    format!("{name}() argument {}: invalid signal number {n}", idx + 1),
                ))
            }
        }
        Value::String(s) => niao_signal::parse_signal_name(s).ok_or_else(|| {
            type_err(
                span,
                format!(
                    "{name}() argument {}: unknown signal name '{s}'",
                    idx + 1
                ),
            )
        }),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects int or string signal as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn invoke_handler(handler: &ValueRef, sig: i32, span: Span) -> NiaoResult<ValueRef> {
    let info = signal_info_object(sig, span)?;
    match &*handler.borrow() {
        Value::NativeFunction(native) => native(&[info], span),
        Value::Function(_) => call_niao_function(Rc::clone(handler), &[info], span),
        other => Err(type_err(
            span,
            format!("expected callable handler, got {}", other.type_name()),
        )),
    }
}

fn signal_info_object(sig: i32, span: Span) -> NiaoResult<ValueRef> {
    let mut map = HashMap::new();
    map.insert("signum".to_string(), Value::Int(sig as i64).ref_cell());
    if let Some(name) = niao_signal::signal_name(sig) {
        map.insert("name".to_string(), Value::String(name.to_string()).ref_cell());
    }
    if let Some(desc) = niao_signal::strsignal(sig) {
        map.insert("description".to_string(), Value::String(desc).ref_cell());
    }
    let _ = span;
    Ok(Value::Object(map).ref_cell())
}

fn apply_os_kind(sig: i32, kind: HandlerKind, span: Span) -> Result<(), ValueRef> {
    niao_signal::set_handler_kind(sig, kind).map_err(|e| {
        nsignal_err(span, format!("failed to register signal {sig}: {e}"))
    })
}

fn store_user_handler(sig: i32, handler: UserHandler, span: Span) -> Result<(), ValueRef> {
    let os_kind = match &handler {
        UserHandler::Callable(_) => HandlerKind::Watched,
        UserHandler::Ignore => HandlerKind::Ignore,
        UserHandler::Default => HandlerKind::Default,
    };
    apply_os_kind(sig, os_kind, span)?;
    USER_HANDLERS.with(|h| {
        match &handler {
            UserHandler::Default => h.borrow_mut().remove(&sig),
            _ => h.borrow_mut().insert(sig, handler),
        };
    });
    Ok(())
}

fn handler_to_value(handler: Option<&UserHandler>) -> ValueRef {
    match handler {
        None | Some(UserHandler::Default) => Value::Int(SIG_DFL_SENTINEL).ref_cell(),
        Some(UserHandler::Ignore) => Value::Int(SIG_IGN_SENTINEL).ref_cell(),
        Some(UserHandler::Callable(f)) => Rc::clone(f),
    }
}

fn dispatch_pending(sig: i32, span: Span) -> NiaoResult<Option<ValueRef>> {
    let handler = USER_HANDLERS.with(|h| h.borrow().get(&sig).cloned());
    match handler {
        Some(UserHandler::Callable(f)) => invoke_handler(&f, sig, span).map(Some),
        Some(UserHandler::Ignore) | None => Ok(None),
        Some(UserHandler::Default) => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

// >>> nsignal.name(nsignal.SIGINT)
// => "sigint"
fn nsignal_name(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsignal_name", span)?;
    let sig = signal_arg(args, 0, "nsignal_name", span)?;
    niao_signal::signal_name(sig)
        .map(str_val)
        .ok_or_else(|| {
            RuntimeError::at(
                span,
                codes::E3483_NSIGNAL_INVALID,
                format!("invalid signal number {sig}"),
            )
        })
        .map_err(Into::into)
}

// >>> nsignal.number("SIGTERM")
// => 15
fn nsignal_number(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsignal_number", span)?;
    let sig = match &*args[0].borrow() {
        Value::String(s) => niao_signal::parse_signal_name(s).ok_or_else(|| {
            type_err(span, format!("nsignal_number() unknown signal name '{s}'"))
        })?,
        Value::Int(n) => {
            let sig = *n as i32;
            if niao_signal::is_valid_signal(sig) {
                sig
            } else {
                return Err(RuntimeError::at(
                    span,
                    codes::E3483_NSIGNAL_INVALID,
                    format!("invalid signal number {n}"),
                ));
            }
        }
        other => {
            return Err(type_err(
                span,
                format!(
                    "nsignal_number() expects string or int, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    Ok(Value::Int(sig as i64).ref_cell())
}

// >>> len(nsignal.valid())
// => platform-dependent
fn nsignal_valid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nsignal_valid", span)?;
    let _ = span;
    let items = niao_signal::valid_signals()
        .into_iter()
        .map(|n| Value::Int(n as i64).ref_cell())
        .collect();
    array_val(items)
}

// >>> nsignal.strsignal(nsignal.SIGINT)
// => "SIGINT (Interrupt)"
fn nsignal_strsignal(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsignal_strsignal", span)?;
    let sig = signal_arg(args, 0, "nsignal_strsignal", span)?;
    niao_signal::strsignal(sig)
        .map(str_val)
        .ok_or_else(|| {
            RuntimeError::at(
                span,
                codes::E3483_NSIGNAL_INVALID,
                format!("invalid signal number {sig}"),
            )
        })
        .map_err(Into::into)
}

// >>> nsignal.get(nsignal.SIGINT)
// => -1
fn nsignal_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsignal_get", span)?;
    let sig = signal_arg(args, 0, "nsignal_get", span)?;
    let user = USER_HANDLERS.with(|h| h.borrow().get(&sig).map(|u| match u {
        UserHandler::Callable(f) => UserHandler::Callable(Rc::clone(f)),
        UserHandler::Ignore => UserHandler::Ignore,
        UserHandler::Default => UserHandler::Default,
    }));
    Ok(handler_to_value(user.as_ref()))
}

// >>> nsignal.on(nsignal.SIGUSR1, fn(info) { print(info.name) })
// => true
fn nsignal_on(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsignal_on", span)?;
    let sig = signal_arg(args, 0, "nsignal_on", span)?;
    let handler = callable_arg(args, 1, "nsignal_on", span)?;
    match store_user_handler(sig, UserHandler::Callable(handler), span) {
        Ok(()) => bool_val(true),
        Err(e) => Ok(e),
    }
}

// >>> nsignal.off(nsignal.SIGUSR1)
// => true
fn nsignal_off(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsignal_off", span)?;
    let sig = signal_arg(args, 0, "nsignal_off", span)?;
    match store_user_handler(sig, UserHandler::Default, span) {
        Ok(()) => bool_val(true),
        Err(e) => Ok(e),
    }
}

// >>> nsignal.ignore(nsignal.SIGPIPE)
// => true
fn nsignal_ignore(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsignal_ignore", span)?;
    let sig = signal_arg(args, 0, "nsignal_ignore", span)?;
    match store_user_handler(sig, UserHandler::Ignore, span) {
        Ok(()) => bool_val(true),
        Err(e) => Ok(e),
    }
}

// >>> nsignal.default(nsignal.SIGPIPE)
// => true
// >>> nsignal.default(nsignal.SIGABRT)
// => true
fn nsignal_default(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsignal_default", span)?;
    let sig = signal_arg(args, 0, "nsignal_default", span)?;
    match store_user_handler(sig, UserHandler::Default, span) {
        Ok(()) => bool_val(true),
        Err(e) => Ok(e),
    }
}

// >>> nsignal.pending()
// => []
fn nsignal_pending(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nsignal_pending", span)?;
    let _ = span;
    let items = niao_signal::peek_pending()
        .into_iter()
        .map(|n| Value::Int(n as i64).ref_cell())
        .collect();
    array_val(items)
}

// >>> nsignal.poll()
// => []
fn nsignal_poll(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nsignal_poll", span)?;
    let pending = niao_signal::drain_pending();
    let mut handled = Vec::new();
    for sig in pending {
        if dispatch_pending(sig, span)?.is_some() {
            handled.push(Value::Int(sig as i64).ref_cell());
        } else {
            handled.push(Value::Int(sig as i64).ref_cell());
        }
    }
    array_val(handled)
}

// >>> nsignal.pause()
// => signum or nil
fn nsignal_pause(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nsignal_pause", span)?;
    match niao_signal::wait_for(None, None) {
        Some(sig) => {
            let _ = dispatch_pending(sig, span)?;
            Ok(Value::Int(sig as i64).ref_cell())
        }
        None => nil_val(),
    }
}

// >>> nsignal.wait(nsignal.SIGALRM, 100)
// => signum or nil
fn nsignal_wait(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nsignal_wait", span)?;
    let sig = signal_arg(args, 0, "nsignal_wait", span)?;
    let timeout_ms = optional_int(args, 1, -1);
    let timeout = if timeout_ms < 0 {
        None
    } else {
        Some(Duration::from_millis(timeout_ms as u64))
    };
    match niao_signal::wait_for(Some(sig), timeout) {
        Some(got) => {
            let _ = dispatch_pending(got, span)?;
            Ok(Value::Int(got as i64).ref_cell())
        }
        None => nil_val(),
    }
}

// >>> nsignal.alarm(0)
// => previous seconds
fn nsignal_alarm(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsignal_alarm", span)?;
    let secs = int_arg(args, 0, "nsignal_alarm", span)?;
    if secs < 0 {
        return Ok(nsignal_err(
            span,
            "nsignal_alarm() seconds must be >= 0",
        ));
    }
    #[cfg(not(unix))]
    {
        let _ = secs;
        return Ok(nsignal_err(
            span,
            "nsignal_alarm() is not supported on this platform",
        ));
    }
    #[cfg(unix)]
    {
        let prev = niao_signal::alarm(secs as u32);
        Ok(Value::Int(prev as i64).ref_cell())
    }
}

// >>> nsignal.raise(nsignal.SIGUSR1)  // may error if unsupported
fn nsignal_raise(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsignal_raise", span)?;
    let sig = signal_arg(args, 0, "nsignal_raise", span)?;
    match niao_signal::raise_signal(sig) {
        Ok(()) => bool_val(true),
        Err(e) => Ok(nsignal_err(span, format!("raise signal {sig}: {e}"))),
    }
}

// >>> nsignal.shutdown(fn() { print("bye") })
// => guard handle object
fn nsignal_shutdown(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nsignal_shutdown", span)?;
    let handler = callable_arg(args, 0, "nsignal_shutdown", span)?;
    let signals = if args.len() == 2 {
        match &*args[1].borrow() {
            Value::Array(items) => {
                let mut out = Vec::new();
                for (i, item) in items.iter().enumerate() {
                    let sig = match &*item.borrow() {
                        Value::Int(n) => {
                            let s = *n as i32;
                            if niao_signal::is_valid_signal(s) {
                                s
                            } else {
                                return Ok(nsignal_err(
                                    span,
                                    format!("shutdown signals[{}]: invalid signum {n}", i),
                                ));
                            }
                        }
                        Value::String(s) => match niao_signal::parse_signal_name(s) {
                            Some(sig) => sig,
                            None => {
                                return Ok(nsignal_err(
                                    span,
                                    format!("shutdown signals[{}]: unknown name '{s}'", i),
                                ));
                            }
                        },
                        other => {
                            return Ok(nsignal_err(
                                span,
                                format!(
                                    "shutdown signals[{}]: expected int or string, got {}",
                                    i,
                                    other.type_name()
                                ),
                            ));
                        }
                    };
                    out.push(sig);
                }
                out
            }
            other => {
                return Ok(nsignal_err(
                    span,
                    format!(
                        "nsignal_shutdown() signals must be an array, got {}",
                        other.type_name()
                    ),
                ));
            }
        }
    } else {
        vec![
            niao_signal::parse_signal_name("sigint").unwrap(),
            niao_signal::parse_signal_name("sigterm").unwrap(),
        ]
    };

    for &sig in &signals {
        if let Err(e) = store_user_handler(sig, UserHandler::Callable(Rc::clone(&handler)), span) {
            return Ok(e);
        }
    }

    let id = new_guard_id();
    SHUTDOWN_GUARDS.with(|g| {
        g.borrow_mut().insert(
            id,
            ShutdownGuard {
                signals,
                handler,
            },
        );
    });

    let mut map = HashMap::new();
    map.insert("id".to_string(), Value::Int(id).ref_cell());
    map.insert("kind".to_string(), Value::String("shutdown".into()).ref_cell());
    Ok(Value::Object(map).ref_cell())
}

// >>> nsignal.shutdown_cancel(guard.id)
// => true
fn nsignal_shutdown_cancel(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsignal_shutdown_cancel", span)?;
    let id = int_arg(args, 0, "nsignal_shutdown_cancel", span)?;
    let guard = SHUTDOWN_GUARDS.with(|g| g.borrow_mut().remove(&id));
    match guard {
        Some(g) => {
            for sig in g.signals {
                let _ = store_user_handler(sig, UserHandler::Default, span);
            }
            bool_val(true)
        }
        None => Ok(nsignal_err(
            span,
            format!("invalid shutdown guard id {id}"),
        )),
    }
}

// >>> nsignal.reset()
// => true
fn nsignal_reset(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nsignal_reset", span)?;
    USER_HANDLERS.with(|h| h.borrow_mut().clear());
    SHUTDOWN_GUARDS.with(|g| g.borrow_mut().clear());
    match niao_signal::reset_all() {
        Ok(()) => bool_val(true),
        Err(e) => Ok(nsignal_err(span, format!("reset signals: {e}"))),
    }
}

// >>> nsignal.info(nsignal.SIGINT)
// => {signum, name, description, handler}
fn nsignal_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsignal_info", span)?;
    let sig = signal_arg(args, 0, "nsignal_info", span)?;
    let mut map = HashMap::new();
    map.insert("signum".to_string(), Value::Int(sig as i64).ref_cell());
    if let Some(name) = niao_signal::signal_name(sig) {
        map.insert("name".to_string(), Value::String(name.to_string()).ref_cell());
    }
    if let Some(desc) = niao_signal::strsignal(sig) {
        map.insert("description".to_string(), Value::String(desc).ref_cell());
    }
    let user = USER_HANDLERS.with(|h| h.borrow().get(&sig).map(|u| match u {
        UserHandler::Callable(f) => UserHandler::Callable(Rc::clone(f)),
        UserHandler::Ignore => UserHandler::Ignore,
        UserHandler::Default => UserHandler::Default,
    }));
    map.insert(
        "handler".to_string(),
        handler_to_value(user.as_ref()),
    );
    Ok(Value::Object(map).ref_cell())
}

// ---------------------------------------------------------------------------
// Module exports
// ---------------------------------------------------------------------------

macro_rules! nsignal_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nsignal_fns![
    ("nsignal_name", "name", nsignal_name),
    ("nsignal_number", "number", nsignal_number),
    ("nsignal_valid", "valid", nsignal_valid),
    ("nsignal_strsignal", "strsignal", nsignal_strsignal),
    ("nsignal_get", "get", nsignal_get),
    ("nsignal_on", "on", nsignal_on),
    ("nsignal_off", "off", nsignal_off),
    ("nsignal_ignore", "ignore", nsignal_ignore),
    ("nsignal_default", "default", nsignal_default),
    ("nsignal_pending", "pending", nsignal_pending),
    ("nsignal_poll", "poll", nsignal_poll),
    ("nsignal_pause", "pause", nsignal_pause),
    ("nsignal_wait", "wait", nsignal_wait),
    ("nsignal_alarm", "alarm", nsignal_alarm),
    ("nsignal_raise", "raise", nsignal_raise),
    ("nsignal_shutdown", "shutdown", nsignal_shutdown),
    ("nsignal_shutdown_cancel", "shutdown_cancel", nsignal_shutdown_cancel),
    ("nsignal_reset", "reset", nsignal_reset),
    ("nsignal_info", "info", nsignal_info),
];

pub const MODULE_NAME: &str = "nsignal";
pub const MODULE_PATHS: &[&str] = &["nsignal", "std/nsignal"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    map.insert(
        "SIG_DFL".to_string(),
        Value::Int(SIG_DFL_SENTINEL).ref_cell(),
    );
    map.insert(
        "SIG_IGN".to_string(),
        Value::Int(SIG_IGN_SENTINEL).ref_cell(),
    );
    for (name, num) in niao_signal::signal_constants() {
        map.insert(name.to_ascii_uppercase(), Value::Int(num as i64).ref_cell());
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

    fn s(v: &str) -> ValueRef {
        Value::String(v.to_string()).ref_cell()
    }

    #[test]
    fn valid_nonempty() {
        let out = nsignal_valid(&[], span()).unwrap();
        match &*out.borrow() {
            Value::Array(items) => assert!(!items.is_empty()),
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[test]
    fn name_number_roundtrip() {
        let sigint = nsignal_number(&[s("SIGINT")], span()).unwrap();
        assert!(matches!(&*sigint.borrow(), Value::Int(_)));
        let name = nsignal_name(&[sigint], span()).unwrap();
        assert!(matches!(&*name.borrow(), Value::String(n) if n == "sigint"));
    }

    #[test]
    fn get_default_before_on() {
        let sig = Value::Int(niao_signal::parse_signal_name("sigterm").unwrap() as i64).ref_cell();
        let got = nsignal_get(&[sig.clone()], span()).unwrap();
        assert!(matches!(&*got.borrow(), Value::Int(SIG_DFL_SENTINEL)));
    }

    #[test]
    fn ignore_and_get() {
        let sig = Value::Int(niao_signal::parse_signal_name("sigabrt").unwrap() as i64).ref_cell();
        nsignal_ignore(&[sig.clone()], span()).unwrap();
        let got = nsignal_get(&[sig], span()).unwrap();
        assert!(matches!(&*got.borrow(), Value::Int(SIG_IGN_SENTINEL)));
        nsignal_reset(&[], span()).unwrap();
    }

    #[test]
    fn info_has_fields() {
        let sig = Value::Int(niao_signal::parse_signal_name("sigterm").unwrap() as i64).ref_cell();
        let info = nsignal_info(&[sig], span()).unwrap();
        match &*info.borrow() {
            Value::Object(map) => {
                assert!(map.contains_key("signum"));
                assert!(map.contains_key("name"));
                assert!(map.contains_key("handler"));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn shutdown_guard_lifecycle() {
        fn noop(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
            Ok(Value::Nil.ref_cell())
        }
        let guard = nsignal_shutdown(
            &[Value::NativeFunction(Rc::new(noop)).ref_cell()],
            span(),
        )
        .unwrap();
        let id = match &*guard.borrow() {
            Value::Object(map) => match &*map["id"].borrow() {
                Value::Int(n) => *n,
                _ => panic!("bad id"),
            },
            _ => panic!("expected guard object"),
        };
        let cancelled = nsignal_shutdown_cancel(&[i(id)], span()).unwrap();
        assert!(matches!(&*cancelled.borrow(), Value::Bool(true)));
        nsignal_reset(&[], span()).unwrap();
    }
}
