//! Native crypto standard library — SHA-256/512, HMAC, hex digests.

use crate::{NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_crypto::{hex, hmac, hmac_sha256, sha256, sha512, HmacAlgorithm};
use niao_errors::codes;
use std::collections::HashMap;
use std::rc::Rc;

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::TypeError {
        message: msg.into(),
        line: span.line,
        col: span.col,
    }
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E1040_CRYPTO_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn bytes_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<u8>> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        Value::ByteArray(b) => Ok(b.iter().map(|&x| x as u8).collect()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects string or bytes as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn crypto_sha256(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "crypto_sha256", span)?;
    let data = bytes_arg(args, 0, "crypto_sha256", span)?;
    Ok(Value::String(hex::encode(&sha256(&data))).ref_cell())
}

fn crypto_sha512(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "crypto_sha512", span)?;
    let data = bytes_arg(args, 0, "crypto_sha512", span)?;
    Ok(Value::String(hex::encode(&sha512(&data))).ref_cell())
}

fn crypto_hmac(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "crypto_hmac", span)?;
    let algo = match &*args[0].borrow() {
        Value::String(s) => match s.to_ascii_lowercase().as_str() {
            "sha256" | "hs256" => HmacAlgorithm::Sha256,
            "sha512" | "hs512" => HmacAlgorithm::Sha512,
            other => {
                return Err(type_err(
                    span,
                    format!("crypto_hmac: unsupported algorithm '{other}'"),
                ));
            }
        },
        other => {
            return Err(type_err(
                span,
                format!(
                    "crypto_hmac() expects a string as argument 1, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    let key = bytes_arg(args, 1, "crypto_hmac", span)?;
    let data = bytes_arg(args, 2, "crypto_hmac", span)?;
    let out = hmac(algo, &key, &data);
    Ok(Value::String(hex::encode(&out)).ref_cell())
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    let bind = |map: &mut HashMap<String, ValueRef>, name: &str, f: NativeFn| {
        map.insert(name.to_string(), Value::NativeFunction(f).ref_cell());
    };
    bind(&mut map, "sha256", Rc::new(crypto_sha256));
    bind(&mut map, "sha512", Rc::new(crypto_sha512));
    bind(&mut map, "hmac", Rc::new(crypto_hmac));
    Value::Object(map)
}

pub const MODULE_NAME: &str = "crypto";
pub const MODULE_PATHS: &[&str] = &["crypto", "std/crypto"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    vec![
        ("crypto_sha256", Rc::new(crypto_sha256)),
        ("crypto_sha512", Rc::new(crypto_sha512)),
        ("crypto_hmac", Rc::new(crypto_hmac)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    #[test]
    fn sha256_builtin() {
        let span = Span::dummy();
        let args = vec![Value::String("abc".into()).ref_cell()];
        let out = crypto_sha256(&args, span).unwrap();
        let s = match &*out.borrow() {
            Value::String(s) => s.clone(),
            other => panic!("expected string, got {other:?}"),
        };
        assert_eq!(
            s,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn hmac_builtin() {
        let span = Span::dummy();
        let args = vec![
            Value::String("sha256".into()).ref_cell(),
            Value::String("key".into()).ref_cell(),
            Value::String("data".into()).ref_cell(),
        ];
        let out = crypto_hmac(&args, span).unwrap();
        let digest = hmac_sha256(b"key", b"data");
        let s = match &*out.borrow() {
            Value::String(s) => s.clone(),
            other => panic!("expected string, got {other:?}"),
        };
        assert_eq!(s, hex::encode(&digest));
    }
}
