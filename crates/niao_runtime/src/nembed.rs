//! Native nembed standard library — content-hash embedding cache with a local
//! deterministic embedder (SHA-256 seeded, L2-normalized float vectors).
//!
//! Import with `import "nembed"` (or `import "std/nembed"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_crypto::{hex, sha256};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const E3310_NEMBED_ARITY: u32 = 3310;
const E3311_NEMBED_ERROR: u32 = 3311;
const E3312_NEMBED_TYPE: u32 = 3312;
const E3313_NEMBED_INVALID_HANDLE: u32 = 3313;

const DEFAULT_DIM: usize = 384;
const MIN_DIM: usize = 8;
const MAX_DIM: usize = 4096;

// ---------------------------------------------------------------------------
// Deterministic embedder
// ---------------------------------------------------------------------------

fn content_hash_bytes(text: &str) -> [u8; 32] {
    sha256(text.as_bytes())
}

fn content_hash_hex(text: &str) -> String {
    hex::encode(&content_hash_bytes(text))
}

/// SHA-256 seeded pseudo-random float in [-1, 1] for dimension `i`.
fn dim_unit(seed: &[u8; 32], i: usize) -> f64 {
    let mut buf = [0u8; 40];
    buf[..32].copy_from_slice(seed);
    buf[32..40].copy_from_slice(&(i as u64).to_le_bytes());
    let h = sha256(&buf);
    let n = u64::from_le_bytes(h[..8].try_into().unwrap());
    (n as f64 / u64::MAX as f64) * 2.0 - 1.0
}

fn l2_normalize(v: &mut [f64]) {
    let sum: f64 = v.iter().map(|x| x * x).sum();
    if sum > 0.0 {
        let inv = sum.sqrt().recip();
        for x in v.iter_mut() {
            *x *= inv;
        }
    }
}

fn embed_text(text: &str, dim: usize) -> Vec<f64> {
    let seed = content_hash_bytes(text);
    let mut out = Vec::with_capacity(dim);
    for i in 0..dim {
        out.push(dim_unit(&seed, i));
    }
    l2_normalize(&mut out);
    out
}

fn vec_to_float_array(v: &[f64]) -> ValueRef {
    Value::FloatArray(v.to_vec()).ref_cell()
}

fn float_array_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<f64>> {
    match &*args[idx].borrow() {
        Value::FloatArray(v) => Ok(v.clone()),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::Float(f) => out.push(*f),
                    Value::Int(n) => out.push(*n as f64),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() expects float[] or float array at index {}, element {} is {}",
                                idx + 1,
                                i,
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(out)
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() expects float[] as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

// ---------------------------------------------------------------------------
// Cache model
// ---------------------------------------------------------------------------

struct EmbedCache {
    dim: usize,
    map: HashMap<String, ValueRef>,
    hits: u64,
    misses: u64,
}

impl EmbedCache {
    fn new(dim: usize) -> Self {
        EmbedCache {
            dim,
            map: HashMap::new(),
            hits: 0,
            misses: 0,
        }
    }

    fn get(&mut self, key: &str) -> Option<ValueRef> {
        match self.map.get(key) {
            Some(v) => {
                self.hits += 1;
                Some(Rc::clone(v))
            }
            None => {
                self.misses += 1;
                None
            }
        }
    }

    fn get_or_embed(&mut self, text: &str) -> ValueRef {
        let key = content_hash_hex(text);
        if let Some(v) = self.map.get(&key) {
            self.hits += 1;
            return Rc::clone(v);
        }
        self.misses += 1;
        let vec = embed_text(text, self.dim);
        let cell = vec_to_float_array(&vec);
        self.map.insert(key, Rc::clone(&cell));
        cell
    }
}

thread_local! {
    static CACHES: RefCell<HashMap<i64, EmbedCache>> = RefCell::new(HashMap::new());
    static NEXT_HANDLE: RefCell<i64> = const { RefCell::new(1) };
}

fn new_handle() -> i64 {
    NEXT_HANDLE.with(|h| {
        let mut h = h.borrow_mut();
        let id = *h;
        *h += 1;
        id
    })
}

fn with_cache<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut EmbedCache) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    CACHES.with(|caches| {
        let mut caches = caches.borrow_mut();
        match caches.get_mut(&id) {
            Some(c) => Ok(Ok(f(c))),
            None => Ok(Err(error_value(
                E3313_NEMBED_INVALID_HANDLE,
                "nembed_error",
                format!("invalid or closed nembed handle {id}"),
                span,
            ))),
        }
    })
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3312_NEMBED_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3310_NEMBED_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3310_NEMBED_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
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

fn string_array_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<String>> {
    match &*args[idx].borrow() {
        Value::StringArray(sa) => Ok(sa.dense_vec()),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::String(s) => out.push(s.clone()),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() expects string[] at argument {}, element {} is {}",
                                idx + 1,
                                i,
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(out)
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() expects string[] as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn parse_dim(n: i64, span: Span) -> Result<usize, ValueRef> {
    if n < MIN_DIM as i64 || n > MAX_DIM as i64 {
        return Err(nembed_err(
            span,
            format!("dimension must be in {MIN_DIM}..={MAX_DIM}, got {n}"),
        ));
    }
    Ok(n as usize)
}

fn nembed_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3311_NEMBED_ERROR, "nembed_error", msg.into(), span)
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn nembed_open(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nembed_open", span)?;
    let dim = if args.is_empty() {
        DEFAULT_DIM
    } else {
        match parse_dim(int_arg(args, 0, "nembed_open", span)?, span) {
            Ok(d) => d,
            Err(e) => return Ok(e),
        }
    };
    let id = new_handle();
    CACHES.with(|caches| {
        caches.borrow_mut().insert(id, EmbedCache::new(dim));
    });
    Ok(Value::Int(id).ref_cell())
}

fn nembed_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nembed_close", span)?;
    let id = int_arg(args, 0, "nembed_close", span)?;
    let removed = CACHES.with(|caches| caches.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

fn nembed_dim(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nembed_dim", span)?;
    let id = int_arg(args, 0, "nembed_dim", span)?;
    match with_cache(id, span, |c| c.dim as i64)? {
        Ok(n) => Ok(Value::Int(n).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nembed_hash(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nembed_hash", span)?;
    let text = string_arg(args, 0, "nembed_hash", span)?;
    Ok(Value::String(content_hash_hex(&text)).ref_cell())
}

/// nembed_embed(text, dim?) — pure deterministic embed (no cache).
fn nembed_embed(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nembed_embed", span)?;
    let text = string_arg(args, 0, "nembed_embed", span)?;
    let dim = if args.len() > 1 {
        match parse_dim(int_arg(args, 1, "nembed_embed", span)?, span) {
            Ok(d) => d,
            Err(e) => return Ok(e),
        }
    } else {
        DEFAULT_DIM
    };
    Ok(vec_to_float_array(&embed_text(&text, dim)))
}

fn nembed_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nembed_get", span)?;
    let id = int_arg(args, 0, "nembed_get", span)?;
    let text = string_arg(args, 1, "nembed_get", span)?;
    let key = content_hash_hex(&text);
    match with_cache(id, span, |c| c.get(&key))? {
        Ok(Some(v)) => Ok(v),
        Ok(None) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nembed_get_or_embed(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nembed_get_or_embed", span)?;
    let id = int_arg(args, 0, "nembed_get_or_embed", span)?;
    let text = string_arg(args, 1, "nembed_get_or_embed", span)?;
    match with_cache(id, span, |c| c.get_or_embed(&text))? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

fn nembed_embed_batch(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nembed_embed_batch", span)?;
    let id = int_arg(args, 0, "nembed_embed_batch", span)?;
    let texts = string_array_arg(args, 1, "nembed_embed_batch", span)?;
    match with_cache(id, span, |c| {
        texts
            .iter()
            .map(|t| c.get_or_embed(t))
            .collect::<Vec<ValueRef>>()
    })? {
        Ok(items) => Ok(Value::Array(items).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nembed_has(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nembed_has", span)?;
    let id = int_arg(args, 0, "nembed_has", span)?;
    let text = string_arg(args, 1, "nembed_has", span)?;
    let key = content_hash_hex(&text);
    match with_cache(id, span, |c| c.map.contains_key(&key))? {
        Ok(b) => Ok(Value::Bool(b).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nembed_clear(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nembed_clear", span)?;
    let id = int_arg(args, 0, "nembed_clear", span)?;
    match with_cache(id, span, |c| c.map.clear())? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nembed_len(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nembed_len", span)?;
    let id = int_arg(args, 0, "nembed_len", span)?;
    match with_cache(id, span, |c| c.map.len() as i64)? {
        Ok(n) => Ok(Value::Int(n).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nembed_stats(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nembed_stats", span)?;
    let id = int_arg(args, 0, "nembed_stats", span)?;
    match with_cache(id, span, |c| (c.hits, c.misses, c.map.len(), c.dim))? {
        Ok((hits, misses, len, dim)) => {
            let mut map = HashMap::new();
            map.insert("hits".to_string(), Value::Int(hits as i64).ref_cell());
            map.insert("misses".to_string(), Value::Int(misses as i64).ref_cell());
            map.insert("len".to_string(), Value::Int(len as i64).ref_cell());
            map.insert("dim".to_string(), Value::Int(dim as i64).ref_cell());
            let total = hits + misses;
            let rate = if total == 0 {
                0.0
            } else {
                hits as f64 / total as f64
            };
            map.insert("hit_rate".to_string(), Value::Float(rate).ref_cell());
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(e),
    }
}

fn nembed_cosine(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nembed_cosine", span)?;
    let a = float_array_arg(args, 0, "nembed_cosine", span)?;
    let b = float_array_arg(args, 1, "nembed_cosine", span)?;
    if a.len() != b.len() {
        return Ok(nembed_err(
            span,
            format!(
                "nembed_cosine() vectors must have equal length, got {} and {}",
                a.len(),
                b.len()
            ),
        ));
    }
    if a.is_empty() {
        return Ok(nembed_err(span, "nembed_cosine() vectors must not be empty"));
    }
    let mut dot = 0.0;
    let mut na = 0.0;
    let mut nb = 0.0;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    let denom = na.sqrt() * nb.sqrt();
    let score = if denom == 0.0 { 0.0 } else { dot / denom };
    Ok(Value::Float(score).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nembed_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nembed_fns![
    ("nembed_open", "open", nembed_open),
    ("nembed_close", "close", nembed_close),
    ("nembed_dim", "dim", nembed_dim),
    ("nembed_hash", "hash", nembed_hash),
    ("nembed_embed", "embed", nembed_embed),
    ("nembed_get", "get", nembed_get),
    ("nembed_get_or_embed", "get_or_embed", nembed_get_or_embed),
    ("nembed_embed_batch", "embed_batch", nembed_embed_batch),
    ("nembed_has", "has", nembed_has),
    ("nembed_clear", "clear", nembed_clear),
    ("nembed_len", "len", nembed_len),
    ("nembed_stats", "stats", nembed_stats),
    ("nembed_cosine", "cosine", nembed_cosine),
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

pub const MODULE_NAME: &str = "nembed";
pub const MODULE_PATHS: &[&str] = &["nembed", "std/nembed"];

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

    fn s(v: &str) -> ValueRef {
        Value::String(v.to_string()).ref_cell()
    }

    fn handle(r: NiaoResult<ValueRef>) -> ValueRef {
        let v = r.unwrap();
        assert!(matches!(&*v.borrow(), Value::Int(_)));
        v
    }

    #[test]
    fn embed_is_deterministic() {
        let a = nembed_embed(&[s("hello world")], span()).unwrap();
        let b = nembed_embed(&[s("hello world")], span()).unwrap();
        let ar = a.borrow();
        let br = b.borrow();
        match (&*ar, &*br) {
            (Value::FloatArray(va), Value::FloatArray(vb)) => assert_eq!(va, vb),
            _ => panic!("expected float arrays"),
        }
    }

    #[test]
    fn cache_hit_miss_stats() {
        let h = handle(nembed_open(&[i(16)], span()));
        nembed_get_or_embed(&[h.clone(), s("alpha")], span()).unwrap();
        nembed_get_or_embed(&[h.clone(), s("alpha")], span()).unwrap();
        nembed_get(&[h.clone(), s("missing")], span()).unwrap();
        let stats = nembed_stats(&[h.clone()], span()).unwrap();
        match &*stats.borrow() {
            Value::Object(map) => {
                assert!(matches!(&*map.get("hits").unwrap().borrow(), Value::Int(1)));
                assert!(matches!(&*map.get("misses").unwrap().borrow(), Value::Int(2)));
                assert!(matches!(&*map.get("len").unwrap().borrow(), Value::Int(1)));
            }
            other => panic!("expected object, got {other:?}"),
        }
        nembed_close(&[h], span()).unwrap();
    }

    #[test]
    fn hash_is_stable() {
        let h1 = nembed_hash(&[s("test")], span()).unwrap();
        let h2 = nembed_hash(&[s("test")], span()).unwrap();
        let r1 = h1.borrow();
        let r2 = h2.borrow();
        match (&*r1, &*r2) {
            (Value::String(a), Value::String(b)) => assert_eq!(a, b),
            _ => panic!("expected strings"),
        }
    }

    #[test]
    fn invalid_handle_error_value() {
        let v = nembed_get(&[i(999_999), s("x")], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }
}
