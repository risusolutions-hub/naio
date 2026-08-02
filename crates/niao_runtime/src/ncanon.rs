//! Native ncanon standard library — deterministic canonicalization of Niao
//! values to a JSON-like string (sorted object keys), plus FNV-1a 64 hashing
//! and equality via the canonical form.
//!
//! Import with `import "ncanon"` (or `import "std/ncanon"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::rc::Rc;

// Wired into niao_errors::codes by central integration.
const E3070_NCANON_ARITY: u32 = 3070;
const E3071_NCANON_ERROR: u32 = 3071;
const E3072_NCANON_TYPE: u32 = 3072;

const FNV64_OFFSET: u64 = 0xcbf29ce484222325;
const FNV64_PRIME: u64 = 0x100000001b3;
const MAX_DEPTH: usize = 256;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3070_NCANON_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3072_NCANON_TYPE, msg.into())
}

fn canon_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3071_NCANON_ERROR, "ncanon_error", msg.into(), span)
}

fn str_val(s: String) -> NiaoResult<ValueRef> {
    Ok(Value::String(s).ref_cell())
}

fn bool_val(b: bool) -> NiaoResult<ValueRef> {
    Ok(Value::Bool(b).ref_cell())
}

// ---------------------------------------------------------------------------
// Canonical JSON-like encoding
// ---------------------------------------------------------------------------

fn append_escaped_string(s: &str, out: &mut String) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn append_float(f: f64, out: &mut String, span: Span) -> NiaoResult<()> {
    if !f.is_finite() {
        return Err(type_err(
            span,
            "ncanon: non-finite float cannot be canonicalized",
        ));
    }
    // JSON-like: Rust Display (1.0 → "1", 1.5 → "1.5").
    out.push_str(&f.to_string());
    Ok(())
}

fn append_canon(v: &Value, out: &mut String, depth: usize, span: Span) -> NiaoResult<()> {
    if depth > MAX_DEPTH {
        return Err(RuntimeError::at(
            span,
            E3071_NCANON_ERROR,
            format!("ncanon: nesting depth exceeds {MAX_DEPTH}"),
        ));
    }
    match v {
        Value::Nil => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Int(n) => out.push_str(&n.to_string()),
        Value::BigInt(n) => out.push_str(&n.to_string()),
        Value::Float(f) => append_float(*f, out, span)?,
        Value::String(s) => append_escaped_string(s, out),
        Value::IntArray(items) => {
            out.push('[');
            for (i, n) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&n.to_string());
            }
            out.push(']');
        }
        Value::FloatArray(items) => {
            out.push('[');
            for (i, f) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                append_float(*f, out, span)?;
            }
            out.push(']');
        }
        Value::BoolArray(items) => {
            out.push('[');
            for (i, b) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(if *b != 0 { "true" } else { "false" });
            }
            out.push(']');
        }
        Value::ByteArray(items) => {
            out.push('[');
            for (i, b) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&(*b as i64).to_string());
            }
            out.push(']');
        }
        Value::StringArray(items) => {
            out.push('[');
            for i in 0..items.len() {
                if i > 0 {
                    out.push(',');
                }
                append_escaped_string(&items.get(i).unwrap_or_default(), out);
            }
            out.push(']');
        }
        Value::Array(items) => {
            out.push('[');
            for (i, slot) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                append_canon(&slot.borrow(), out, depth + 1, span)?;
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = Vec::with_capacity(map.len());
            keys.extend(map.keys());
            keys.sort();
            out.push('{');
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                append_escaped_string(key, out);
                out.push(':');
                let child = map.get(key.as_str()).unwrap();
                append_canon(&child.borrow(), out, depth + 1, span)?;
            }
            out.push('}');
        }
        other => {
            return Err(type_err(
                span,
                format!(
                    "ncanon: cannot canonicalize value of type {}",
                    other.type_name()
                ),
            ));
        }
    }
    Ok(())
}

fn estimate_canon_len(v: &Value) -> usize {
    match v {
        Value::Nil => 4,
        Value::Bool(_) => 5,
        Value::Int(n) => n.unsigned_abs().max(1).ilog10() as usize + 2,
        Value::BigInt(_) | Value::Float(_) => 24,
        Value::String(s) => s.len() + 2,
        Value::IntArray(items) => items.len() * 4 + 2,
        Value::FloatArray(items) => items.len() * 8 + 2,
        Value::BoolArray(items) => items.len() * 5 + 2,
        Value::ByteArray(items) => items.len() * 4 + 2,
        Value::StringArray(items) => {
            2 + (0..items.len())
                .map(|i| items.get(i).map(|s| s.len() + 2).unwrap_or(2))
                .sum::<usize>()
        }
        Value::Array(items) => {
            2 + items
                .iter()
                .map(|slot| estimate_canon_len(&slot.borrow()))
                .sum::<usize>()
        }
        Value::Object(map) => {
            2 + map
                .iter()
                .map(|(k, v)| k.len() + 3 + estimate_canon_len(&v.borrow()))
                .sum::<usize>()
        }
        _ => 32,
    }
}

fn canonize(value: &ValueRef, span: Span) -> NiaoResult<String> {
    let mut out = String::with_capacity(estimate_canon_len(&value.borrow()));
    append_canon(&value.borrow(), &mut out, 0, span)?;
    Ok(out)
}

/// Turn nesting-depth soft failures into catchable error values; rethrow the rest.
fn map_canon_err(err: RuntimeError, span: Span) -> NiaoResult<ValueRef> {
    if err.code() == E3071_NCANON_ERROR {
        Ok(canon_err(span, err.message()))
    } else {
        Err(err)
    }
}

// ---------------------------------------------------------------------------
// FNV-1a 64
// ---------------------------------------------------------------------------

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = FNV64_OFFSET;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(FNV64_PRIME);
    }
    hash
}

fn hash_hex(canon: &str) -> String {
    format!("{:016x}", fnv1a64(canon.as_bytes()))
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// ncanon_canon(value) → canonical JSON-like string
fn ncanon_canon(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncanon_canon", span)?;
    match canonize(&args[0], span) {
        Ok(s) => str_val(s),
        Err(e) => map_canon_err(e, span),
    }
}

/// ncanon_hash(value) → lowercase 16-char hex of FNV-1a 64 over canon bytes
fn ncanon_hash(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncanon_hash", span)?;
    match canonize(&args[0], span) {
        Ok(s) => str_val(hash_hex(&s)),
        Err(e) => map_canon_err(e, span),
    }
}

/// ncanon_equal(a, b) → true when canon(a) == canon(b)
fn ncanon_equal(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncanon_equal", span)?;
    let a = match canonize(&args[0], span) {
        Ok(s) => s,
        Err(e) => return map_canon_err(e, span),
    };
    let b = match canonize(&args[1], span) {
        Ok(s) => s,
        Err(e) => return map_canon_err(e, span),
    };
    bool_val(a == b)
}

/// ncanon_fingerprint(value) → first 8 hex chars of hash(value)
fn ncanon_fingerprint(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncanon_fingerprint", span)?;
    match canonize(&args[0], span) {
        Ok(s) => {
            let hex = hash_hex(&s);
            str_val(hex[..8].to_string())
        }
        Err(e) => map_canon_err(e, span),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ncanon_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ncanon_fns![
    ("ncanon_canon", "canon", ncanon_canon),
    ("ncanon_hash", "hash", ncanon_hash),
    ("ncanon_equal", "equal", ncanon_equal),
    ("ncanon_fingerprint", "fingerprint", ncanon_fingerprint),
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

pub const MODULE_NAME: &str = "ncanon";
pub const MODULE_PATHS: &[&str] = &["ncanon", "std/ncanon"];

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

    fn obj(pairs: &[(&str, Value)]) -> ValueRef {
        let mut map = HashMap::new();
        for (k, v) in pairs {
            map.insert((*k).to_string(), v.clone().ref_cell());
        }
        Value::Object(map).ref_cell()
    }

    fn expect_str(v: ValueRef) -> String {
        match &*v.borrow() {
            Value::String(s) => s.clone(),
            other => panic!("expected string, got {other:?}"),
        }
    }

    fn expect_bool(v: ValueRef) -> bool {
        match &*v.borrow() {
            Value::Bool(b) => *b,
            other => panic!("expected bool, got {other:?}"),
        }
    }

    #[test]
    fn canon_scalars_and_escape() {
        assert_eq!(
            expect_str(ncanon_canon(&[Value::Nil.ref_cell()], span()).unwrap()),
            "null"
        );
        assert_eq!(
            expect_str(ncanon_canon(&[Value::Bool(true).ref_cell()], span()).unwrap()),
            "true"
        );
        assert_eq!(
            expect_str(ncanon_canon(&[Value::Int(42).ref_cell()], span()).unwrap()),
            "42"
        );
        assert_eq!(
            expect_str(ncanon_canon(&[Value::Float(1.5).ref_cell()], span()).unwrap()),
            "1.5"
        );
        assert_eq!(
            expect_str(ncanon_canon(&[Value::Float(2.0).ref_cell()], span()).unwrap()),
            "2"
        );
        assert_eq!(
            expect_str(
                ncanon_canon(&[Value::String("a\"b\\c\nd".into()).ref_cell()], span()).unwrap()
            ),
            r#""a\"b\\c\nd""#
        );
    }

    #[test]
    fn canon_sorts_object_keys() {
        let a = obj(&[("b", Value::Int(2)), ("a", Value::Int(1))]);
        assert_eq!(
            expect_str(ncanon_canon(&[a], span()).unwrap()),
            r#"{"a":1,"b":2}"#
        );
    }

    #[test]
    fn equal_via_canon_ignores_key_order() {
        let left = obj(&[("z", Value::Int(9)), ("a", Value::Int(1))]);
        let right = obj(&[("a", Value::Int(1)), ("z", Value::Int(9))]);
        assert!(expect_bool(ncanon_equal(&[left, right], span()).unwrap()));
        let other = obj(&[("a", Value::Int(1)), ("z", Value::Int(8))]);
        let left2 = obj(&[("z", Value::Int(9)), ("a", Value::Int(1))]);
        assert!(!expect_bool(ncanon_equal(&[left2, other], span()).unwrap()));
    }

    #[test]
    fn hash_and_fingerprint() {
        let v = obj(&[("x", Value::Int(1))]);
        let hex = expect_str(ncanon_hash(&[Rc::clone(&v)], span()).unwrap());
        assert_eq!(hex.len(), 16);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(hex.chars().all(|c| !c.is_ascii_uppercase()));

        let fp = expect_str(ncanon_fingerprint(&[v], span()).unwrap());
        assert_eq!(fp, &hex[..8]);

        // Known FNV-1a 64 of the canon bytes for {"x":1}
        let expected = format!("{:016x}", fnv1a64(br#"{"x":1}"#));
        assert_eq!(hex, expected);
    }

    #[test]
    fn arrays_compact() {
        let arr = Value::Array(vec![
            Value::Int(1).ref_cell(),
            Value::String("hi".into()).ref_cell(),
            Value::Nil.ref_cell(),
        ])
        .ref_cell();
        assert_eq!(
            expect_str(ncanon_canon(&[arr], span()).unwrap()),
            r#"[1,"hi",null]"#
        );
    }

    #[test]
    fn arity_and_nonfinite() {
        let err = ncanon_canon(&[], span()).unwrap_err();
        assert_eq!(err.code(), E3070_NCANON_ARITY);
        let err = ncanon_canon(&[Value::Float(f64::NAN).ref_cell()], span()).unwrap_err();
        assert_eq!(err.code(), E3072_NCANON_TYPE);
    }

    #[test]
    fn fnv1a_empty() {
        assert_eq!(fnv1a64(b""), FNV64_OFFSET);
        assert_eq!(fnv1a64(b"a"), {
            let mut h = FNV64_OFFSET;
            h ^= b'a' as u64;
            h.wrapping_mul(FNV64_PRIME)
        });
    }
}
