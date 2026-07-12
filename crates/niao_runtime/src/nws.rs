//! Native nws standard library — ergonomic WebSocket client wrapper over `net`.
//!
//! Uses the same handle IDs as `net` websocket connections (`net_ws_*` registry).
//!
//! Import with `import "nws"` (or `import "std/nws"`).

use crate::net::websocket::{net_ws_close, net_ws_connect, net_ws_recv, net_ws_send};
use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::collections::HashMap;
use std::rc::Rc;

// codes.rs integration pending — use local constants until wired.
const E_NWS_ARITY: u32 = 2870;
const E_NWS_ERROR: u32 = 2871;
const E_NWS_TYPE: u32 = 2872;
const E_NWS_INVALID_HANDLE: u32 = 2873;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E_NWS_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E_NWS_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn nws_error(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E_NWS_ERROR, "nws_error", msg.into(), span)
}

fn remap_msg(msg: &str) -> String {
    msg.replace("net_ws_", "nws_")
}

fn remap_net_err(err: RuntimeError) -> RuntimeError {
    match err {
        RuntimeError::Generic {
            code,
            message,
            line,
            col,
        } => {
            let new_code = match code {
                c if c == codes::E1400_NET_ARITY => E_NWS_ARITY,
                c if c == codes::E1401_NET_ERROR => E_NWS_ERROR,
                c if c == codes::E1402_NET_INVALID_HANDLE => E_NWS_INVALID_HANDLE,
                _ => code,
            };
            RuntimeError::Generic {
                code: new_code,
                message: remap_msg(&message),
                line,
                col,
            }
        }
        RuntimeError::TypeError {
            message,
            line,
            col,
        } => RuntimeError::Generic {
            code: E_NWS_TYPE,
            message: remap_msg(&message),
            line,
            col,
        },
        other => other,
    }
}

fn remap_ok(v: ValueRef, span: Span) -> NiaoResult<ValueRef> {
    let net_err_msg = match &*v.borrow() {
        Value::Error(e) if e.code == codes::E1401_NET_ERROR => Some(remap_msg(&e.message)),
        _ => None,
    };
    if let Some(msg) = net_err_msg {
        Ok(nws_error(span, msg))
    } else {
        Ok(v)
    }
}

fn delegate(result: NiaoResult<ValueRef>, span: Span) -> NiaoResult<ValueRef> {
    match result {
        Ok(v) => remap_ok(v, span),
        Err(e) => Err(remap_net_err(e)),
    }
}

fn nil_to_true(v: ValueRef) -> ValueRef {
    if matches!(&*v.borrow(), Value::Nil) {
        Value::Bool(true).ref_cell()
    } else {
        v
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// nws_connect(url, opts?) -> handle int
fn nws_connect(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nws_connect", span)?;
    delegate(net_ws_connect(args, span), span)
}

/// nws_send(id, message) -> true on success, catchable nws_error on failure
fn nws_send(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nws_send", span)?;
    delegate(net_ws_send(args, span), span).map(nil_to_true)
}

/// nws_recv(id) -> string, byte array, or nil on close frame
fn nws_recv(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nws_recv", span)?;
    delegate(net_ws_recv(args, span), span)
}

/// nws_close(id) -> true on success
fn nws_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nws_close", span)?;
    delegate(net_ws_close(args, span), span).map(nil_to_true)
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nws_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nws_fns![
    ("nws_connect", "connect", nws_connect),
    ("nws_send", "send", nws_send),
    ("nws_recv", "recv", nws_recv),
    ("nws_close", "close", nws_close),
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

pub const MODULE_NAME: &str = "nws";
pub const MODULE_PATHS: &[&str] = &["nws", "std/nws"];

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
    fn connect_arity() {
        let err = nws_connect(&[], span()).unwrap_err();
        assert_eq!(err.code(), E_NWS_ARITY);
        let err = nws_connect(
            &[
                Value::String("ws://example.com".into()).ref_cell(),
                Value::Object(HashMap::new()).ref_cell(),
                Value::Nil.ref_cell(),
            ],
            span(),
        )
        .unwrap_err();
        assert_eq!(err.code(), E_NWS_ARITY);
    }

    #[test]
    fn send_arity() {
        let err = nws_send(&[], span()).unwrap_err();
        assert_eq!(err.code(), E_NWS_ARITY);
        let err = nws_send(
            &[Value::Int(1).ref_cell(), Value::String("hi".into()).ref_cell(), Value::Nil.ref_cell()],
            span(),
        )
        .unwrap_err();
        assert_eq!(err.code(), E_NWS_ARITY);
    }

    #[test]
    fn recv_arity() {
        let err = nws_recv(&[], span()).unwrap_err();
        assert_eq!(err.code(), E_NWS_ARITY);
    }

    #[test]
    fn close_arity() {
        let err = nws_close(&[], span()).unwrap_err();
        assert_eq!(err.code(), E_NWS_ARITY);
    }
}
