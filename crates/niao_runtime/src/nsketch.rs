//! Native `nsketch` standard library — probabilistic sketches:
//! Bloom filters, HyperLogLog-lite, and Count-Min Sketch.
//! Zero external deps; hashes are hand-rolled FNV-1a + xorshift.
//!
//! Import with `import "nsketch"` (or `import "std/nsketch"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// Wired in codes.rs by central integration.
const E3000_NSKETCH_ARITY: u32 = 3000;
const E3001_NSKETCH_ERROR: u32 = 3001;
const E3002_NSKETCH_TYPE: u32 = 3002;
const E3003_NSKETCH_INVALID_HANDLE: u32 = 3003;

const HLL_M: usize = 64;
const HLL_P: u32 = 6; // log2(64)
const MAX_BLOOM_BITS: usize = 64 * 1024 * 1024; // 8 MiB bitset
const MAX_CMS_CELLS: usize = 16 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Hashing (FNV-1a + xorshift mix)
// ---------------------------------------------------------------------------

fn fnv1a64(data: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn xorshift_mix(mut x: u64) -> u64 {
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

/// Two independent 64-bit hashes for double-hashing schemes.
fn hash_pair(data: &[u8]) -> (u64, u64) {
    let h1 = fnv1a64(data);
    let h2 = xorshift_mix(h1 ^ 0x9E37_79B9_7F4A_7C15);
    let h2 = if h2 == 0 { 1 } else { h2 };
    (h1, h2)
}

fn nth_hash(h1: u64, h2: u64, i: u32) -> u64 {
    h1.wrapping_add((i as u64).wrapping_mul(h2))
}

// ---------------------------------------------------------------------------
// Sketch kinds
// ---------------------------------------------------------------------------

struct Bloom {
    bits: Vec<u64>,
    nbits: usize,
    k: u32,
}

impl Bloom {
    fn from_params(expected_n: u64, fp_rate: f64) -> Result<Self, String> {
        if expected_n == 0 {
            return Err("bloom_new() expected_n must be > 0".into());
        }
        if !(fp_rate > 0.0 && fp_rate < 1.0) {
            return Err("bloom_new() fp_rate must be in (0, 1)".into());
        }
        let ln2 = std::f64::consts::LN_2;
        let m_f = -(expected_n as f64) * fp_rate.ln() / (ln2 * ln2);
        let mut nbits = m_f.ceil() as usize;
        if nbits < 64 {
            nbits = 64;
        }
        if nbits > MAX_BLOOM_BITS {
            return Err(format!(
                "bloom_new() bitset too large ({nbits} bits; max {MAX_BLOOM_BITS})"
            ));
        }
        let k_f = (nbits as f64 / expected_n as f64) * ln2;
        let mut k = k_f.round() as u32;
        if k < 1 {
            k = 1;
        }
        if k > 32 {
            k = 32;
        }
        let words = (nbits + 63) / 64;
        Ok(Self {
            bits: vec![0u64; words],
            nbits,
            k,
        })
    }

    fn set_bit(&mut self, idx: usize) {
        let word = idx / 64;
        let bit = idx % 64;
        self.bits[word] |= 1u64 << bit;
    }

    fn get_bit(&self, idx: usize) -> bool {
        let word = idx / 64;
        let bit = idx % 64;
        (self.bits[word] >> bit) & 1 == 1
    }

    fn add(&mut self, s: &str) {
        let (h1, h2) = hash_pair(s.as_bytes());
        for i in 0..self.k {
            let idx = (nth_hash(h1, h2, i) as usize) % self.nbits;
            self.set_bit(idx);
        }
    }

    fn may_contain(&self, s: &str) -> bool {
        let (h1, h2) = hash_pair(s.as_bytes());
        for i in 0..self.k {
            let idx = (nth_hash(h1, h2, i) as usize) % self.nbits;
            if !self.get_bit(idx) {
                return false;
            }
        }
        true
    }

    fn clear(&mut self) {
        for w in &mut self.bits {
            *w = 0;
        }
    }
}

/// HyperLogLog-lite with fixed m=64 registers.
struct Hll {
    registers: [u8; HLL_M],
}

impl Hll {
    fn new() -> Self {
        Self {
            registers: [0u8; HLL_M],
        }
    }

    fn add(&mut self, s: &str) {
        let h = fnv1a64(s.as_bytes());
        let idx = (h & ((HLL_M as u64) - 1)) as usize;
        let w = h >> HLL_P;
        let rho = (w.trailing_zeros() + 1).min(64 - HLL_P) as u8;
        if rho > self.registers[idx] {
            self.registers[idx] = rho;
        }
    }

    fn count(&self) -> f64 {
        // α_m for m=64 ≈ 0.709
        let alpha = 0.709;
        let mut sum = 0.0_f64;
        let mut zeros = 0u32;
        for &r in &self.registers {
            sum += 2.0_f64.powi(-(r as i32));
            if r == 0 {
                zeros += 1;
            }
        }
        let m = HLL_M as f64;
        let mut est = alpha * m * m / sum;
        // Small-range correction
        if est <= 2.5 * m && zeros > 0 {
            est = m * (m / zeros as f64).ln();
        }
        // Large-range correction (32-bit hash space)
        let two32 = (1u64 << 32) as f64;
        if est > (1.0 / 30.0) * two32 {
            est = -two32 * (1.0 - est / two32).ln();
        }
        if est < 0.0 {
            0.0
        } else {
            est
        }
    }
}

struct Cms {
    width: usize,
    depth: usize,
    counters: Vec<u64>,
}

impl Cms {
    fn new(width: usize, depth: usize) -> Result<Self, String> {
        if width == 0 || depth == 0 {
            return Err("cms_new() width and depth must be > 0".into());
        }
        let cells = width.saturating_mul(depth);
        if cells == 0 || cells > MAX_CMS_CELLS {
            return Err(format!(
                "cms_new() table too large ({width}×{depth}; max {MAX_CMS_CELLS} cells)"
            ));
        }
        Ok(Self {
            width,
            depth,
            counters: vec![0u64; cells],
        })
    }

    fn add(&mut self, s: &str, count: u64) {
        let (h1, h2) = hash_pair(s.as_bytes());
        for row in 0..self.depth {
            let col = (nth_hash(h1, h2, row as u32) as usize) % self.width;
            let idx = row * self.width + col;
            self.counters[idx] = self.counters[idx].saturating_add(count);
        }
    }

    fn estimate(&self, s: &str) -> u64 {
        let (h1, h2) = hash_pair(s.as_bytes());
        let mut min = u64::MAX;
        for row in 0..self.depth {
            let col = (nth_hash(h1, h2, row as u32) as usize) % self.width;
            let idx = row * self.width + col;
            min = min.min(self.counters[idx]);
        }
        if min == u64::MAX {
            0
        } else {
            min
        }
    }
}

enum Sketch {
    Bloom(Bloom),
    Hll(Hll),
    Cms(Cms),
}

impl Sketch {
    fn kind_name(&self) -> &'static str {
        match self {
            Sketch::Bloom(_) => "bloom",
            Sketch::Hll(_) => "hll",
            Sketch::Cms(_) => "cms",
        }
    }
}

// ---------------------------------------------------------------------------
// Thread-local handle table
// ---------------------------------------------------------------------------

thread_local! {
    static HANDLES: RefCell<HashMap<i64, Sketch>> = RefCell::new(HashMap::new());
    static NEXT_ID: RefCell<i64> = const { RefCell::new(1) };
}

fn new_handle(sketch: Sketch) -> i64 {
    let id = NEXT_ID.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    HANDLES.with(|h| {
        h.borrow_mut().insert(id, sketch);
    });
    id
}

fn with_sketch<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut Sketch) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    HANDLES.with(|h| {
        let mut guard = h.borrow_mut();
        match guard.get_mut(&id) {
            Some(s) => Ok(Ok(f(s))),
            None => Ok(Err(error_value(
                E3003_NSKETCH_INVALID_HANDLE,
                "nsketch_error",
                format!("invalid or closed nsketch handle {id}"),
                span,
            ))),
        }
    })
}

fn wrong_kind(span: Span, want: &str, got: &str) -> ValueRef {
    error_value(
        E3001_NSKETCH_ERROR,
        "nsketch_error",
        format!("expected {want} handle, got {got}"),
        span,
    )
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3000_NSKETCH_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3000_NSKETCH_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn type_code_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3002_NSKETCH_TYPE, msg.into())
}

fn int_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        other => Err(type_code_err(
            span,
            format!(
                "{name}() expects int as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn string_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(type_code_err(
            span,
            format!(
                "{name}() expects string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn number_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<f64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n as f64),
        Value::Float(f) => Ok(*f),
        other => Err(type_code_err(
            span,
            format!(
                "{name}() expects number as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn sketch_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3001_NSKETCH_ERROR, "nsketch_error", msg.into(), span)
}

// ---------------------------------------------------------------------------
// Builtins — Bloom
// ---------------------------------------------------------------------------

fn nsketch_bloom_new(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nsketch_bloom_new", span)?;
    let expected_n = int_arg(args, 0, "nsketch_bloom_new", span)?;
    if expected_n < 0 {
        return Ok(sketch_err(span, "bloom_new() expected_n must be > 0"));
    }
    let fp = if args.len() >= 2 {
        number_arg(args, 1, "nsketch_bloom_new", span)?
    } else {
        0.01
    };
    match Bloom::from_params(expected_n as u64, fp) {
        Ok(b) => Ok(Value::Int(new_handle(Sketch::Bloom(b))).ref_cell()),
        Err(msg) => Ok(sketch_err(span, msg)),
    }
}

fn nsketch_bloom_add(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsketch_bloom_add", span)?;
    let id = int_arg(args, 0, "nsketch_bloom_add", span)?;
    let key = string_arg(args, 1, "nsketch_bloom_add", span)?;
    match with_sketch(id, span, |s| match s {
        Sketch::Bloom(b) => {
            b.add(&key);
            Ok(Value::Bool(true).ref_cell())
        }
        other => Err(wrong_kind(span, "bloom", other.kind_name())),
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

fn nsketch_bloom_may_contain(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsketch_bloom_may_contain", span)?;
    let id = int_arg(args, 0, "nsketch_bloom_may_contain", span)?;
    let key = string_arg(args, 1, "nsketch_bloom_may_contain", span)?;
    match with_sketch(id, span, |s| match s {
        Sketch::Bloom(b) => Ok(Value::Bool(b.may_contain(&key)).ref_cell()),
        other => Err(wrong_kind(span, "bloom", other.kind_name())),
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

fn nsketch_bloom_clear(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsketch_bloom_clear", span)?;
    let id = int_arg(args, 0, "nsketch_bloom_clear", span)?;
    match with_sketch(id, span, |s| match s {
        Sketch::Bloom(b) => {
            b.clear();
            Ok(Value::Bool(true).ref_cell())
        }
        other => Err(wrong_kind(span, "bloom", other.kind_name())),
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Builtins — HyperLogLog
// ---------------------------------------------------------------------------

fn nsketch_hll_new(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nsketch_hll_new", span)?;
    Ok(Value::Int(new_handle(Sketch::Hll(Hll::new()))).ref_cell())
}

fn nsketch_hll_add(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsketch_hll_add", span)?;
    let id = int_arg(args, 0, "nsketch_hll_add", span)?;
    let key = string_arg(args, 1, "nsketch_hll_add", span)?;
    match with_sketch(id, span, |s| match s {
        Sketch::Hll(h) => {
            h.add(&key);
            Ok(Value::Bool(true).ref_cell())
        }
        other => Err(wrong_kind(span, "hll", other.kind_name())),
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

fn nsketch_hll_count(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsketch_hll_count", span)?;
    let id = int_arg(args, 0, "nsketch_hll_count", span)?;
    match with_sketch(id, span, |s| match s {
        Sketch::Hll(h) => {
            let est = h.count();
            // Prefer Int when close to an integer.
            if est.fract().abs() < 1e-9 && est >= i64::MIN as f64 && est <= i64::MAX as f64 {
                Ok(Value::Int(est.round() as i64).ref_cell())
            } else {
                Ok(Value::Float(est).ref_cell())
            }
        }
        other => Err(wrong_kind(span, "hll", other.kind_name())),
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Builtins — Count-Min Sketch
// ---------------------------------------------------------------------------

fn nsketch_cms_new(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsketch_cms_new", span)?;
    let width = int_arg(args, 0, "nsketch_cms_new", span)?;
    let depth = int_arg(args, 1, "nsketch_cms_new", span)?;
    if width <= 0 || depth <= 0 {
        return Ok(sketch_err(span, "cms_new() width and depth must be > 0"));
    }
    match Cms::new(width as usize, depth as usize) {
        Ok(c) => Ok(Value::Int(new_handle(Sketch::Cms(c))).ref_cell()),
        Err(msg) => Ok(sketch_err(span, msg)),
    }
}

fn nsketch_cms_add(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nsketch_cms_add", span)?;
    let id = int_arg(args, 0, "nsketch_cms_add", span)?;
    let key = string_arg(args, 1, "nsketch_cms_add", span)?;
    let count = if args.len() >= 3 {
        let c = int_arg(args, 2, "nsketch_cms_add", span)?;
        if c < 0 {
            return Ok(sketch_err(span, "cms_add() count must be >= 0"));
        }
        c as u64
    } else {
        1
    };
    match with_sketch(id, span, |s| match s {
        Sketch::Cms(c) => {
            c.add(&key, count);
            Ok(Value::Bool(true).ref_cell())
        }
        other => Err(wrong_kind(span, "cms", other.kind_name())),
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

fn nsketch_cms_estimate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsketch_cms_estimate", span)?;
    let id = int_arg(args, 0, "nsketch_cms_estimate", span)?;
    let key = string_arg(args, 1, "nsketch_cms_estimate", span)?;
    match with_sketch(id, span, |s| match s {
        Sketch::Cms(c) => Ok(Value::Int(c.estimate(&key) as i64).ref_cell()),
        other => Err(wrong_kind(span, "cms", other.kind_name())),
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Builtins — common
// ---------------------------------------------------------------------------

fn nsketch_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsketch_close", span)?;
    let id = int_arg(args, 0, "nsketch_close", span)?;
    let removed = HANDLES.with(|h| h.borrow_mut().remove(&id).is_some());
    if removed {
        Ok(Value::Bool(true).ref_cell())
    } else {
        Ok(error_value(
            E3003_NSKETCH_INVALID_HANDLE,
            "nsketch_error",
            format!("invalid or closed nsketch handle {id}"),
            span,
        ))
    }
}

fn nsketch_kind(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsketch_kind", span)?;
    let id = int_arg(args, 0, "nsketch_kind", span)?;
    match with_sketch(id, span, |s| s.kind_name().to_string())? {
        Ok(name) => Ok(Value::String(name).ref_cell()),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nsketch_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nsketch_fns![
    ("nsketch_bloom_new", "bloom_new", nsketch_bloom_new),
    ("nsketch_bloom_add", "bloom_add", nsketch_bloom_add),
    ("nsketch_bloom_may_contain", "bloom_may_contain", nsketch_bloom_may_contain),
    ("nsketch_bloom_clear", "bloom_clear", nsketch_bloom_clear),
    ("nsketch_hll_new", "hll_new", nsketch_hll_new),
    ("nsketch_hll_add", "hll_add", nsketch_hll_add),
    ("nsketch_hll_count", "hll_count", nsketch_hll_count),
    ("nsketch_cms_new", "cms_new", nsketch_cms_new),
    ("nsketch_cms_add", "cms_add", nsketch_cms_add),
    ("nsketch_cms_estimate", "cms_estimate", nsketch_cms_estimate),
    ("nsketch_close", "close", nsketch_close),
    ("nsketch_kind", "kind", nsketch_kind),
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

pub const MODULE_NAME: &str = "nsketch";
pub const MODULE_PATHS: &[&str] = &["nsketch", "std/nsketch"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::dummy()
    }

    fn i(v: i64) -> ValueRef {
        Value::Int(v).ref_cell()
    }

    fn f(v: f64) -> ValueRef {
        Value::Float(v).ref_cell()
    }

    fn s(v: &str) -> ValueRef {
        Value::String(v.to_string()).ref_cell()
    }

    fn handle(r: NiaoResult<ValueRef>) -> ValueRef {
        let v = r.unwrap();
        assert!(
            matches!(&*v.borrow(), Value::Int(_)),
            "expected handle int, got {:?}",
            v.borrow()
        );
        v
    }

    fn as_bool(v: &ValueRef) -> bool {
        match &*v.borrow() {
            Value::Bool(b) => *b,
            other => panic!("expected bool, got {other:?}"),
        }
    }

    fn as_number(v: &ValueRef) -> f64 {
        match &*v.borrow() {
            Value::Int(n) => *n as f64,
            Value::Float(x) => *x,
            other => panic!("expected number, got {other:?}"),
        }
    }

    #[test]
    fn bloom_add_and_may_contain() {
        let h = handle(nsketch_bloom_new(&[i(100), f(0.01)], span()));
        assert_eq!(
            match &*nsketch_kind(&[h.clone()], span()).unwrap().borrow() {
                Value::String(k) => k.as_str(),
                _ => panic!(),
            },
            "bloom"
        );
        nsketch_bloom_add(&[h.clone(), s("alice")], span()).unwrap();
        nsketch_bloom_add(&[h.clone(), s("bob")], span()).unwrap();
        let yes = nsketch_bloom_may_contain(&[h.clone(), s("alice")], span()).unwrap();
        assert!(as_bool(&yes));
        let yes2 = nsketch_bloom_may_contain(&[h.clone(), s("bob")], span()).unwrap();
        assert!(as_bool(&yes2));
        let no = nsketch_bloom_may_contain(&[h.clone(), s("carol")], span()).unwrap();
        // Probabilistic — with empty filter region for "carol" should usually be false.
        // After only 2 inserts into a filter sized for 100, FP chance is tiny.
        assert!(!as_bool(&no));
        nsketch_bloom_clear(&[h.clone()], span()).unwrap();
        let after = nsketch_bloom_may_contain(&[h.clone(), s("alice")], span()).unwrap();
        assert!(!as_bool(&after));
        nsketch_close(&[h], span()).unwrap();
    }

    #[test]
    fn hll_count_grows_with_distinct() {
        let h = handle(nsketch_hll_new(&[], span()));
        let empty = as_number(&nsketch_hll_count(&[h.clone()], span()).unwrap());
        assert!(empty < 1.5, "empty HLL estimate={empty}");

        for i in 0..50 {
            nsketch_hll_add(&[h.clone(), s(&format!("item-{i}"))], span()).unwrap();
        }
        let mid = as_number(&nsketch_hll_count(&[h.clone()], span()).unwrap());
        assert!(mid > 20.0, "after 50 distinct, estimate={mid}");

        for i in 50..200 {
            nsketch_hll_add(&[h.clone(), s(&format!("item-{i}"))], span()).unwrap();
        }
        let large = as_number(&nsketch_hll_count(&[h.clone()], span()).unwrap());
        assert!(
            large > mid,
            "HLL should grow: mid={mid} large={large}"
        );
        // Rough ballpark for 200 distinct with m=64
        assert!(
            large > 80.0 && large < 500.0,
            "HLL estimate for ~200 items out of range: {large}"
        );
        nsketch_close(&[h], span()).unwrap();
    }

    #[test]
    fn cms_estimate_at_least_true_count() {
        let h = handle(nsketch_cms_new(&[i(256), i(4)], span()));
        nsketch_cms_add(&[h.clone(), s("x"), i(5)], span()).unwrap();
        nsketch_cms_add(&[h.clone(), s("x")], span()).unwrap(); // +1
        nsketch_cms_add(&[h.clone(), s("y"), i(3)], span()).unwrap();
        let ex = as_number(&nsketch_cms_estimate(&[h.clone(), s("x")], span()).unwrap());
        let ey = as_number(&nsketch_cms_estimate(&[h.clone(), s("y")], span()).unwrap());
        let ez = as_number(&nsketch_cms_estimate(&[h.clone(), s("z")], span()).unwrap());
        assert!(ex >= 6.0, "cms estimate for x={ex}");
        assert!(ey >= 3.0, "cms estimate for y={ey}");
        assert!(ez >= 0.0);
        assert_eq!(
            match &*nsketch_kind(&[h.clone()], span()).unwrap().borrow() {
                Value::String(k) => k.as_str(),
                _ => panic!(),
            },
            "cms"
        );
        nsketch_close(&[h], span()).unwrap();
    }

    #[test]
    fn invalid_handle_is_error_value() {
        let v = nsketch_kind(&[i(999_999)], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn wrong_kind_is_error_value() {
        let h = handle(nsketch_hll_new(&[], span()));
        let v = nsketch_bloom_add(&[h.clone(), s("x")], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
        nsketch_close(&[h], span()).unwrap();
    }
}
