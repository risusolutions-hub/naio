//! Native ncap standard library — cooperative capability sandbox.
//!
//! Thread-local grant set. Builtins do **not** auto-check; callers use
//! `require()` / `check()` around sensitive work.
//!
//! Import with `import "ncap"` (or `import "std/ncap"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

// Wired in codes.rs by central integration.
const E2980_NCAP_ARITY: u32 = 2980;
const E2981_NCAP_ERROR: u32 = 2981;
const E2982_NCAP_TYPE: u32 = 2982;
const E2983_NCAP_DENIED: u32 = 2983;

const VALID_CAPS: &[&str] = &["net", "fs", "env", "process", "gpu", "all"];

// ---------------------------------------------------------------------------
// Thread-local sandbox state
// ---------------------------------------------------------------------------

struct CapState {
    /// `false` after `allow_all` (default): unrestricted.
    /// `true` after `deny_all`: only granted caps pass checks.
    enabled: bool,
    granted: HashSet<String>,
}

impl CapState {
    fn allow_all() -> Self {
        Self {
            enabled: false,
            granted: HashSet::new(),
        }
    }

    fn has(&self, cap: &str) -> bool {
        if !self.enabled {
            return true;
        }
        self.granted.contains("all") || self.granted.contains(cap)
    }
}

thread_local! {
    static STATE: RefCell<CapState> = RefCell::new(CapState::allow_all());
}

fn with_state<R>(f: impl FnOnce(&mut CapState) -> R) -> R {
    STATE.with(|cell| f(&mut cell.borrow_mut()))
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E2980_NCAP_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn validate_cap(cap: &str, span: Span) -> NiaoResult<()> {
    if VALID_CAPS.contains(&cap) {
        Ok(())
    } else {
        Err(RuntimeError::at(
            span,
            E2981_NCAP_ERROR,
            format!(
                "unknown capability '{cap}'; expected one of: net, fs, env, process, gpu, all"
            ),
        ))
    }
}

/// Parse a single capability string or an array of strings.
fn parse_caps(arg: &ValueRef, name: &str, span: Span) -> NiaoResult<Vec<String>> {
    match &*arg.borrow() {
        Value::String(s) => {
            validate_cap(s, span)?;
            Ok(vec![s.clone()])
        }
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::String(s) => {
                        validate_cap(s, span)?;
                        out.push(s.clone());
                    }
                    other => {
                        return Err(RuntimeError::at(
                            span,
                            E2982_NCAP_TYPE,
                            format!(
                                "{name}() array element {} must be a string, got {}",
                                i + 1,
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(out)
        }
        other => Err(RuntimeError::at(
            span,
            E2982_NCAP_TYPE,
            format!(
                "{name}() expects a string or array of strings, got {}",
                other.type_name()
            ),
        )),
    }
}

fn string_cap(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => {
            validate_cap(s, span)?;
            Ok(s.clone())
        }
        other => Err(RuntimeError::at(
            span,
            E2982_NCAP_TYPE,
            format!(
                "{name}() expects a string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn denied(span: Span, cap: &str) -> ValueRef {
    error_value(
        E2983_NCAP_DENIED,
        "ncap_error",
        format!("capability '{cap}' not granted"),
        span,
    )
}

fn ok_bool(b: bool) -> NiaoResult<ValueRef> {
    Ok(Value::Bool(b).ref_cell())
}

fn ok_true() -> NiaoResult<ValueRef> {
    ok_bool(true)
}

fn ok_nil() -> NiaoResult<ValueRef> {
    Ok(Value::Nil.ref_cell())
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn ncap_allow_all(_args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(_args, 0, "ncap_allow_all", span)?;
    with_state(|s| *s = CapState::allow_all());
    ok_nil()
}

fn ncap_deny_all(_args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(_args, 0, "ncap_deny_all", span)?;
    with_state(|s| {
        s.enabled = true;
        s.granted.clear();
    });
    ok_nil()
}

fn ncap_grant(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncap_grant", span)?;
    let caps = parse_caps(&args[0], "ncap_grant", span)?;
    with_state(|s| {
        for c in caps {
            s.granted.insert(c);
        }
    });
    ok_nil()
}

fn ncap_revoke(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncap_revoke", span)?;
    let caps = parse_caps(&args[0], "ncap_revoke", span)?;
    with_state(|s| {
        for c in caps {
            s.granted.remove(&c);
        }
    });
    ok_nil()
}

fn ncap_list(_args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(_args, 0, "ncap_list", span)?;
    let mut caps = with_state(|s| s.granted.iter().cloned().collect::<Vec<_>>());
    caps.sort();
    Ok(Value::Array(
        caps.into_iter()
            .map(|c| Value::String(c).ref_cell())
            .collect(),
    )
    .ref_cell())
}

fn ncap_check(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncap_check", span)?;
    let cap = string_cap(args, 0, "ncap_check", span)?;
    let ok = with_state(|s| s.has(&cap));
    ok_bool(ok)
}

fn ncap_require(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncap_require", span)?;
    let cap = string_cap(args, 0, "ncap_require", span)?;
    let ok = with_state(|s| s.has(&cap));
    if ok {
        ok_true()
    } else {
        Ok(denied(span, &cap))
    }
}

fn ncap_enabled(_args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(_args, 0, "ncap_enabled", span)?;
    let enabled = with_state(|s| s.enabled);
    ok_bool(enabled)
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ncap_fns {
    ($(($flat:expr, $short:expr, $f:expr)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ncap_fns![
    ("ncap_allow_all", "allow_all", ncap_allow_all),
    ("ncap_deny_all", "deny_all", ncap_deny_all),
    ("ncap_grant", "grant", ncap_grant),
    ("ncap_revoke", "revoke", ncap_revoke),
    ("ncap_list", "list", ncap_list),
    ("ncap_check", "check", ncap_check),
    ("ncap_require", "require", ncap_require),
    ("ncap_enabled", "enabled", ncap_enabled),
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

pub const MODULE_NAME: &str = "ncap";
pub const MODULE_PATHS: &[&str] = &["ncap", "std/ncap"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::dummy()
    }

    fn reset() {
        with_state(|s| *s = CapState::allow_all());
    }

    fn s(v: &str) -> ValueRef {
        Value::String(v.into()).ref_cell()
    }

    fn arr(caps: &[&str]) -> ValueRef {
        Value::Array(caps.iter().map(|c| s(c)).collect()).ref_cell()
    }

    #[test]
    fn default_allow_all() {
        reset();
        assert!(matches!(
            &*ncap_enabled(&[], span()).unwrap().borrow(),
            Value::Bool(false)
        ));
        assert!(matches!(
            &*ncap_check(&[s("net")], span()).unwrap().borrow(),
            Value::Bool(true)
        ));
        assert!(matches!(
            &*ncap_require(&[s("fs")], span()).unwrap().borrow(),
            Value::Bool(true)
        ));
    }

    #[test]
    fn deny_all_blocks_until_grant() {
        reset();
        ncap_deny_all(&[], span()).unwrap();
        assert!(matches!(
            &*ncap_enabled(&[], span()).unwrap().borrow(),
            Value::Bool(true)
        ));
        assert!(matches!(
            &*ncap_check(&[s("net")], span()).unwrap().borrow(),
            Value::Bool(false)
        ));
        match &*ncap_require(&[s("net")], span()).unwrap().borrow() {
            Value::Error(e) => assert_eq!(e.code, E2983_NCAP_DENIED),
            other => panic!("expected denied error, got {other:?}"),
        }

        ncap_grant(&[s("net")], span()).unwrap();
        assert!(matches!(
            &*ncap_check(&[s("net")], span()).unwrap().borrow(),
            Value::Bool(true)
        ));
        assert!(matches!(
            &*ncap_require(&[s("net")], span()).unwrap().borrow(),
            Value::Bool(true)
        ));
        assert!(matches!(
            &*ncap_check(&[s("fs")], span()).unwrap().borrow(),
            Value::Bool(false)
        ));
    }

    #[test]
    fn grant_array_and_list_sorted() {
        reset();
        ncap_deny_all(&[], span()).unwrap();
        ncap_grant(&[arr(&["gpu", "fs", "net"])], span()).unwrap();
        match &*ncap_list(&[], span()).unwrap().borrow() {
            Value::Array(items) => {
                let names: Vec<String> = items
                    .iter()
                    .map(|v| match &*v.borrow() {
                        Value::String(s) => s.clone(),
                        _ => panic!("expected string"),
                    })
                    .collect();
                assert_eq!(names, vec!["fs", "gpu", "net"]);
            }
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[test]
    fn revoke_and_all_cap() {
        reset();
        ncap_deny_all(&[], span()).unwrap();
        ncap_grant(&[s("all")], span()).unwrap();
        assert!(matches!(
            &*ncap_check(&[s("env")], span()).unwrap().borrow(),
            Value::Bool(true)
        ));
        ncap_revoke(&[s("all")], span()).unwrap();
        assert!(matches!(
            &*ncap_check(&[s("env")], span()).unwrap().borrow(),
            Value::Bool(false)
        ));

        ncap_grant(&[arr(&["process", "env"])], span()).unwrap();
        ncap_revoke(&[s("env")], span()).unwrap();
        assert!(matches!(
            &*ncap_check(&[s("process")], span()).unwrap().borrow(),
            Value::Bool(true)
        ));
        assert!(matches!(
            &*ncap_check(&[s("env")], span()).unwrap().borrow(),
            Value::Bool(false)
        ));
    }

    #[test]
    fn allow_all_disables_sandbox() {
        reset();
        ncap_deny_all(&[], span()).unwrap();
        ncap_grant(&[s("net")], span()).unwrap();
        ncap_allow_all(&[], span()).unwrap();
        assert!(matches!(
            &*ncap_enabled(&[], span()).unwrap().borrow(),
            Value::Bool(false)
        ));
        assert!(matches!(
            &*ncap_check(&[s("gpu")], span()).unwrap().borrow(),
            Value::Bool(true)
        ));
        match &*ncap_list(&[], span()).unwrap().borrow() {
            Value::Array(items) => assert!(items.is_empty()),
            other => panic!("expected empty list after allow_all, got {other:?}"),
        }
    }

    #[test]
    fn unknown_cap_and_arity() {
        reset();
        let err = ncap_grant(&[s("disk")], span()).unwrap_err();
        assert!(err.to_string().contains("unknown capability"));
        let err = ncap_check(&[], span()).unwrap_err();
        assert!(err.to_string().contains("expects 1"));
        let err = ncap_grant(&[Value::Int(1).ref_cell()], span()).unwrap_err();
        assert!(err.to_string().contains("string or array"));
    }
}
