//! Native nrand standard library — fast random numbers built on xoshiro256**
//! (seeded via SplitMix64). 32 bytes of state, no locks, thread-local default
//! generator plus isolated seeded generator handles for reproducibility.
//!
//! Import with `import "nrand"` (or `import "std/nrand"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Light-RAM guard for generated buffers/strings.
const MAX_GEN_LEN: i64 = 16 * 1024 * 1024;

// ---------------------------------------------------------------------------
// xoshiro256** core
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Xoshiro256 {
    s: [u64; 4],
    /// Cached second normal deviate from Box–Muller.
    spare_normal: Option<f64>,
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl Xoshiro256 {
    fn seeded(seed: u64) -> Self {
        let mut sm = seed;
        let mut s = [0u64; 4];
        for slot in &mut s {
            *slot = splitmix64(&mut sm);
        }
        // xoshiro state must not be all-zero
        if s == [0, 0, 0, 0] {
            s[0] = 0x9E37_79B9_7F4A_7C15;
        }
        Self {
            s,
            spare_normal: None,
        }
    }

    fn from_entropy() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x5EED_5EED_5EED_5EED);
        let addr = &nanos as *const u64 as u64;
        Self::seeded(nanos ^ addr.rotate_left(32))
    }

    #[inline]
    fn next_u64(&mut self) -> u64 {
        let result = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    /// Uniform float in [0, 1) with 53-bit precision.
    #[inline]
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Unbiased integer in [0, bound) via Lemire-style rejection.
    fn next_below(&mut self, bound: u64) -> u64 {
        if bound == 0 {
            return 0;
        }
        loop {
            let x = self.next_u64();
            let hi = ((x as u128 * bound as u128) >> 64) as u64;
            let lo = x.wrapping_mul(bound);
            if lo >= bound || lo >= bound.wrapping_neg() % bound {
                return hi;
            }
        }
    }

    /// Inclusive integer range [lo, hi].
    fn int_range(&mut self, lo: i64, hi: i64) -> i64 {
        if lo == i64::MIN && hi == i64::MAX {
            return self.next_u64() as i64;
        }
        let width = (hi as i128 - lo as i128 + 1) as u64;
        lo.wrapping_add(self.next_below(width) as i64)
    }

    /// Standard normal via Box–Muller with spare caching.
    fn next_normal(&mut self) -> f64 {
        if let Some(spare) = self.spare_normal.take() {
            return spare;
        }
        loop {
            let u1 = self.next_f64();
            if u1 <= f64::MIN_POSITIVE {
                continue;
            }
            let u2 = self.next_f64();
            let r = (-2.0 * u1.ln()).sqrt();
            let theta = std::f64::consts::TAU * u2;
            self.spare_normal = Some(r * theta.sin());
            return r * theta.cos();
        }
    }
}

// ---------------------------------------------------------------------------
// Generator registry (default thread-local + seeded handles)
// ---------------------------------------------------------------------------

thread_local! {
    static DEFAULT_GEN: RefCell<Xoshiro256> = RefCell::new(Xoshiro256::from_entropy());
    static GENERATORS: RefCell<HashMap<i64, Xoshiro256>> = RefCell::new(HashMap::new());
    static NEXT_HANDLE: RefCell<i64> = const { RefCell::new(1) };
}

fn with_default<T>(f: impl FnOnce(&mut Xoshiro256) -> T) -> T {
    DEFAULT_GEN.with(|g| f(&mut g.borrow_mut()))
}

fn with_handle<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut Xoshiro256) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    GENERATORS.with(|gens| {
        let mut gens = gens.borrow_mut();
        match gens.get_mut(&id) {
            Some(g) => Ok(Ok(f(g))),
            None => Ok(Err(error_value(
                codes::E2623_NRAND_INVALID_HANDLE,
                "nrand_error",
                format!("invalid or closed generator handle {id}"),
                span,
            ))),
        }
    })
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

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
            codes::E2620_NRAND_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(
    args: &[ValueRef],
    min: usize,
    max: usize,
    name: &str,
    span: Span,
) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E2620_NRAND_ARITY,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
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

fn num_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<f64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n as f64),
        Value::Float(f) => Ok(*f),
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

fn nrand_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E2621_NRAND_ERROR, "nrand_error", msg.into(), span)
}

fn checked_len(n: i64, name: &str, span: Span) -> Result<usize, ValueRef> {
    if n < 0 {
        return Err(nrand_err(span, format!("{name}() length must be >= 0")));
    }
    if n > MAX_GEN_LEN {
        return Err(nrand_err(span, format!("{name}() length too large")));
    }
    Ok(n as usize)
}

// ---------------------------------------------------------------------------
// Default-generator builtins
// ---------------------------------------------------------------------------

fn nrand_seed(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrand_seed", span)?;
    let seed = int_arg(args, 0, "nrand_seed", span)?;
    with_default(|g| *g = Xoshiro256::seeded(seed as u64));
    Ok(Value::Nil.ref_cell())
}

fn nrand_int(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nrand_int", span)?;
    let lo = int_arg(args, 0, "nrand_int", span)?;
    let hi = int_arg(args, 1, "nrand_int", span)?;
    if lo > hi {
        return Ok(nrand_err(span, "nrand_int() requires lo <= hi"));
    }
    Ok(Value::Int(with_default(|g| g.int_range(lo, hi))).ref_cell())
}

fn nrand_float(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 2, "nrand_float", span)?;
    let (lo, hi) = match args.len() {
        0 => (0.0, 1.0),
        2 => (
            num_arg(args, 0, "nrand_float", span)?,
            num_arg(args, 1, "nrand_float", span)?,
        ),
        _ => {
            return Err(RuntimeError::at(
                span,
                codes::E2620_NRAND_ARITY,
                "nrand_float() expects 0 or 2 arguments",
            ))
        }
    };
    if lo > hi {
        return Ok(nrand_err(span, "nrand_float() requires lo <= hi"));
    }
    let x = with_default(|g| g.next_f64());
    Ok(Value::Float(lo + x * (hi - lo)).ref_cell())
}

fn nrand_fill_float(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if args.len() != 1 && args.len() != 3 {
        return Err(RuntimeError::at(
            span,
            codes::E2620_NRAND_ARITY,
            format!(
                "nrand_fill_float() expects 1 or 3 argument(s), got {}",
                args.len()
            ),
        ));
    }
    let n = int_arg(args, 0, "nrand_fill_float", span)?;
    let n = match checked_len(n, "nrand_fill_float", span) {
        Ok(n) => n,
        Err(e) => return Ok(e),
    };
    let (lo, hi) = match args.len() {
        1 => (0.0, 1.0),
        3 => (
            num_arg(args, 1, "nrand_fill_float", span)?,
            num_arg(args, 2, "nrand_fill_float", span)?,
        ),
        _ => unreachable!(),
    };
    if lo > hi {
        return Ok(nrand_err(span, "nrand_fill_float() requires lo <= hi"));
    }
    let mut out = Vec::with_capacity(n);
    with_default(|g| {
        for _ in 0..n {
            let x = g.next_f64();
            out.push(lo + x * (hi - lo));
        }
    });
    Ok(Value::FloatArray(out).ref_cell())
}

fn nrand_fill_int(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nrand_fill_int", span)?;
    let n = int_arg(args, 0, "nrand_fill_int", span)?;
    let lo = int_arg(args, 1, "nrand_fill_int", span)?;
    let hi = int_arg(args, 2, "nrand_fill_int", span)?;
    let n = match checked_len(n, "nrand_fill_int", span) {
        Ok(n) => n,
        Err(e) => return Ok(e),
    };
    if lo > hi {
        return Ok(nrand_err(span, "nrand_fill_int() requires lo <= hi"));
    }
    let mut out = Vec::with_capacity(n);
    with_default(|g| {
        for _ in 0..n {
            out.push(g.int_range(lo, hi));
        }
    });
    Ok(Value::IntArray(out).ref_cell())
}

fn nrand_bool(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nrand_bool", span)?;
    let p = if args.is_empty() {
        0.5
    } else {
        num_arg(args, 0, "nrand_bool", span)?
    };
    if !(0.0..=1.0).contains(&p) {
        return Ok(nrand_err(span, "nrand_bool() probability must be in 0..=1"));
    }
    Ok(Value::Bool(with_default(|g| g.next_f64()) < p).ref_cell())
}

fn nrand_bytes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrand_bytes", span)?;
    let n = int_arg(args, 0, "nrand_bytes", span)?;
    let n = match checked_len(n, "nrand_bytes", span) {
        Ok(n) => n,
        Err(e) => return Ok(e),
    };
    let mut out = Vec::with_capacity(n);
    with_default(|g| {
        while out.len() + 8 <= n {
            out.extend_from_slice(&g.next_u64().to_le_bytes());
        }
        while out.len() < n {
            out.push(g.next_u64() as u8);
        }
    });
    Ok(Value::ByteArray(out).ref_cell())
}

const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";
const ALNUM_CHARS: &[u8; 62] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

fn nrand_hex(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrand_hex", span)?;
    let n = int_arg(args, 0, "nrand_hex", span)?;
    let n = match checked_len(n, "nrand_hex", span) {
        Ok(n) => n,
        Err(e) => return Ok(e),
    };
    let mut out = String::with_capacity(n);
    with_default(|g| {
        for _ in 0..n {
            out.push(HEX_CHARS[g.next_below(16) as usize] as char);
        }
    });
    Ok(Value::String(out).ref_cell())
}

fn nrand_alphanum(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrand_alphanum", span)?;
    let n = int_arg(args, 0, "nrand_alphanum", span)?;
    let n = match checked_len(n, "nrand_alphanum", span) {
        Ok(n) => n,
        Err(e) => return Ok(e),
    };
    let mut out = String::with_capacity(n);
    with_default(|g| {
        for _ in 0..n {
            out.push(ALNUM_CHARS[g.next_below(62) as usize] as char);
        }
    });
    Ok(Value::String(out).ref_cell())
}

fn nrand_string(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nrand_string", span)?;
    let n = int_arg(args, 0, "nrand_string", span)?;
    let charset = match &*args[1].borrow() {
        Value::String(s) => s.clone(),
        other => {
            return Err(type_err(
                span,
                format!(
                    "nrand_string() expects a charset string, got {}",
                    other.type_name()
                ),
            ))
        }
    };
    let n = match checked_len(n, "nrand_string", span) {
        Ok(n) => n,
        Err(e) => return Ok(e),
    };
    let chars: Vec<char> = charset.chars().collect();
    if chars.is_empty() {
        return Ok(nrand_err(span, "nrand_string() charset must not be empty"));
    }
    let mut out = String::with_capacity(n);
    with_default(|g| {
        for _ in 0..n {
            out.push(chars[g.next_below(chars.len() as u64) as usize]);
        }
    });
    Ok(Value::String(out).ref_cell())
}

fn array_items(value: &Value, name: &str, span: Span) -> NiaoResult<Vec<ValueRef>> {
    match value {
        Value::Array(items) => Ok(items.clone()),
        Value::IntArray(v) => Ok(v.iter().map(|n| Value::Int(*n).ref_cell()).collect()),
        Value::FloatArray(v) => Ok(v.iter().map(|f| Value::Float(*f).ref_cell()).collect()),
        Value::StringArray(sa) => Ok(sa
            .dense_vec()
            .into_iter()
            .map(|s| Value::String(s).ref_cell())
            .collect()),
        other => Err(type_err(
            span,
            format!("{name}() expects an array, got {}", other.type_name()),
        )),
    }
}

fn nrand_choice(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrand_choice", span)?;
    let items = array_items(&args[0].borrow(), "nrand_choice", span)?;
    if items.is_empty() {
        return Ok(nrand_err(span, "nrand_choice() on empty array"));
    }
    let idx = with_default(|g| g.next_below(items.len() as u64)) as usize;
    Ok(Rc::clone(&items[idx]))
}

fn nrand_weighted(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nrand_weighted", span)?;
    let items = array_items(&args[0].borrow(), "nrand_weighted", span)?;
    let weights = match &*args[1].borrow() {
        Value::Array(ws) => {
            let mut out = Vec::with_capacity(ws.len());
            for w in ws {
                match &*w.borrow() {
                    Value::Int(n) => out.push(*n as f64),
                    Value::Float(f) => out.push(*f),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "nrand_weighted() weights must be numbers, found {}",
                                other.type_name()
                            ),
                        ))
                    }
                }
            }
            out
        }
        Value::IntArray(v) => v.iter().map(|n| *n as f64).collect(),
        Value::FloatArray(v) => v.clone(),
        other => {
            return Err(type_err(
                span,
                format!(
                    "nrand_weighted() expects a weights array, got {}",
                    other.type_name()
                ),
            ))
        }
    };
    if items.is_empty() || items.len() != weights.len() {
        return Ok(nrand_err(
            span,
            "nrand_weighted() items and weights must be non-empty and equal length",
        ));
    }
    if weights.iter().any(|w| *w < 0.0 || !w.is_finite()) {
        return Ok(nrand_err(
            span,
            "nrand_weighted() weights must be finite and >= 0",
        ));
    }
    let total: f64 = weights.iter().sum();
    if total <= 0.0 {
        return Ok(nrand_err(span, "nrand_weighted() total weight must be > 0"));
    }
    let mut target = with_default(|g| g.next_f64()) * total;
    for (item, w) in items.iter().zip(&weights) {
        target -= w;
        if target < 0.0 {
            return Ok(Rc::clone(item));
        }
    }
    Ok(Rc::clone(items.last().unwrap()))
}

/// Fisher–Yates shuffle. Shuffles Array / IntArray / FloatArray in place and
/// returns the same reference.
fn nrand_shuffle(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrand_shuffle", span)?;
    {
        let mut value = args[0].borrow_mut();
        match &mut *value {
            Value::Array(items) => with_default(|g| {
                for i in (1..items.len()).rev() {
                    let j = g.next_below(i as u64 + 1) as usize;
                    items.swap(i, j);
                }
            }),
            Value::IntArray(items) => with_default(|g| {
                for i in (1..items.len()).rev() {
                    let j = g.next_below(i as u64 + 1) as usize;
                    items.swap(i, j);
                }
            }),
            Value::FloatArray(items) => with_default(|g| {
                for i in (1..items.len()).rev() {
                    let j = g.next_below(i as u64 + 1) as usize;
                    items.swap(i, j);
                }
            }),
            other => {
                let t = other.type_name();
                drop(value);
                return Err(type_err(
                    span,
                    format!("nrand_shuffle() expects an array, got {t}"),
                ));
            }
        }
    }
    Ok(Rc::clone(&args[0]))
}

/// Reservoir-free sample without replacement (partial Fisher–Yates on a copy).
fn nrand_sample(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nrand_sample", span)?;
    let mut items = array_items(&args[0].borrow(), "nrand_sample", span)?;
    let k = int_arg(args, 1, "nrand_sample", span)?;
    if k < 0 || k as usize > items.len() {
        return Ok(nrand_err(
            span,
            format!("nrand_sample() k must be in 0..={}", items.len()),
        ));
    }
    let k = k as usize;
    with_default(|g| {
        for i in 0..k {
            let j = i + g.next_below((items.len() - i) as u64) as usize;
            items.swap(i, j);
        }
    });
    items.truncate(k);
    Ok(Value::Array(items).ref_cell())
}

fn nrand_normal(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 2, "nrand_normal", span)?;
    let (mu, sigma) = match args.len() {
        0 => (0.0, 1.0),
        2 => (
            num_arg(args, 0, "nrand_normal", span)?,
            num_arg(args, 1, "nrand_normal", span)?,
        ),
        _ => {
            return Err(RuntimeError::at(
                span,
                codes::E2620_NRAND_ARITY,
                "nrand_normal() expects 0 or 2 arguments",
            ))
        }
    };
    if sigma < 0.0 {
        return Ok(nrand_err(span, "nrand_normal() sigma must be >= 0"));
    }
    Ok(Value::Float(mu + sigma * with_default(|g| g.next_normal())).ref_cell())
}

fn nrand_exponential(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrand_exponential", span)?;
    let lambda = num_arg(args, 0, "nrand_exponential", span)?;
    if lambda <= 0.0 {
        return Ok(nrand_err(span, "nrand_exponential() lambda must be > 0"));
    }
    let u = with_default(|g| loop {
        let x = g.next_f64();
        if x > 0.0 {
            return x;
        }
    });
    Ok(Value::Float(-u.ln() / lambda).ref_cell())
}

// ---------------------------------------------------------------------------
// Seeded generator handles
// ---------------------------------------------------------------------------

fn nrand_new_gen(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrand_new_gen", span)?;
    let seed = int_arg(args, 0, "nrand_new_gen", span)?;
    let id = NEXT_HANDLE.with(|h| {
        let mut h = h.borrow_mut();
        let id = *h;
        *h += 1;
        id
    });
    GENERATORS.with(|gens| {
        gens.borrow_mut()
            .insert(id, Xoshiro256::seeded(seed as u64));
    });
    Ok(Value::Int(id).ref_cell())
}

fn nrand_close_gen(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrand_close_gen", span)?;
    let id = int_arg(args, 0, "nrand_close_gen", span)?;
    let removed = GENERATORS.with(|gens| gens.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

fn nrand_gen_int(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nrand_gen_int", span)?;
    let id = int_arg(args, 0, "nrand_gen_int", span)?;
    let lo = int_arg(args, 1, "nrand_gen_int", span)?;
    let hi = int_arg(args, 2, "nrand_gen_int", span)?;
    if lo > hi {
        return Ok(nrand_err(span, "nrand_gen_int() requires lo <= hi"));
    }
    match with_handle(id, span, |g| g.int_range(lo, hi))? {
        Ok(v) => Ok(Value::Int(v).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nrand_gen_float(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrand_gen_float", span)?;
    let id = int_arg(args, 0, "nrand_gen_float", span)?;
    match with_handle(id, span, |g| g.next_f64())? {
        Ok(v) => Ok(Value::Float(v).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nrand_gen_normal(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrand_gen_normal", span)?;
    let id = int_arg(args, 0, "nrand_gen_normal", span)?;
    match with_handle(id, span, |g| g.next_normal())? {
        Ok(v) => Ok(Value::Float(v).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nrand_gen_bytes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nrand_gen_bytes", span)?;
    let id = int_arg(args, 0, "nrand_gen_bytes", span)?;
    let n = int_arg(args, 1, "nrand_gen_bytes", span)?;
    let n = match checked_len(n, "nrand_gen_bytes", span) {
        Ok(n) => n,
        Err(e) => return Ok(e),
    };
    let result = with_handle(id, span, |g| {
        let mut out = Vec::with_capacity(n);
        while out.len() + 8 <= n {
            out.extend_from_slice(&g.next_u64().to_le_bytes());
        }
        while out.len() < n {
            out.push(g.next_u64() as u8);
        }
        out
    })?;
    match result {
        Ok(v) => Ok(Value::ByteArray(v).ref_cell()),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nrand_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nrand_fns![
    ("nrand_seed", "seed", nrand_seed),
    ("nrand_int", "int", nrand_int),
    ("nrand_float", "float", nrand_float),
    ("nrand_fill_float", "fill_float", nrand_fill_float),
    ("nrand_fill_int", "fill_int", nrand_fill_int),
    ("nrand_bool", "bool", nrand_bool),
    ("nrand_bytes", "bytes", nrand_bytes),
    ("nrand_hex", "hex", nrand_hex),
    ("nrand_alphanum", "alphanum", nrand_alphanum),
    ("nrand_string", "string", nrand_string),
    ("nrand_choice", "choice", nrand_choice),
    ("nrand_weighted", "weighted", nrand_weighted),
    ("nrand_shuffle", "shuffle", nrand_shuffle),
    ("nrand_sample", "sample", nrand_sample),
    ("nrand_normal", "normal", nrand_normal),
    ("nrand_exponential", "exponential", nrand_exponential),
    ("nrand_new_gen", "new_gen", nrand_new_gen),
    ("nrand_close_gen", "close_gen", nrand_close_gen),
    ("nrand_gen_int", "gen_int", nrand_gen_int),
    ("nrand_gen_float", "gen_float", nrand_gen_float),
    ("nrand_gen_normal", "gen_normal", nrand_gen_normal),
    ("nrand_gen_bytes", "gen_bytes", nrand_gen_bytes),
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

pub const MODULE_NAME: &str = "nrand";
pub const MODULE_PATHS: &[&str] = &["nrand", "std/nrand"];

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

    fn f(v: f64) -> ValueRef {
        Value::Float(v).ref_cell()
    }

    #[test]
    fn seeded_is_reproducible() {
        let h1 = nrand_new_gen(&[i(42)], span()).unwrap();
        let h2 = nrand_new_gen(&[i(42)], span()).unwrap();
        for _ in 0..16 {
            let a = nrand_gen_int(&[h1.clone(), i(0), i(1_000_000)], span()).unwrap();
            let b = nrand_gen_int(&[h2.clone(), i(0), i(1_000_000)], span()).unwrap();
            let (a, b) = (a.borrow().clone(), b.borrow().clone());
            match (a, b) {
                (Value::Int(x), Value::Int(y)) => assert_eq!(x, y),
                other => panic!("expected ints, got {other:?}"),
            }
        }
        nrand_close_gen(&[h1], span()).unwrap();
        nrand_close_gen(&[h2], span()).unwrap();
    }

    #[test]
    fn int_range_bounds() {
        nrand_seed(&[i(7)], span()).unwrap();
        for _ in 0..1000 {
            let v = nrand_int(&[i(-3), i(3)], span()).unwrap();
            match &*v.borrow() {
                Value::Int(n) => assert!((-3..=3).contains(n)),
                other => panic!("expected int, got {other:?}"),
            };
        }
    }

    #[test]
    fn float_unit_interval() {
        for _ in 0..1000 {
            let v = nrand_float(&[], span()).unwrap();
            match &*v.borrow() {
                Value::Float(f) => assert!((0.0..1.0).contains(f)),
                other => panic!("expected float, got {other:?}"),
            };
        }
    }

    #[test]
    fn fill_int_length_and_bounds() {
        nrand_seed(&[i(7)], span()).unwrap();
        let v = nrand_fill_int(&[i(500), i(-3), i(3)], span()).unwrap();
        match &*v.borrow() {
            Value::IntArray(items) => {
                assert_eq!(items.len(), 500);
                assert!(items.iter().all(|n| (-3..=3).contains(n)));
            }
            other => panic!("expected int array, got {other:?}"),
        }
    }

    #[test]
    fn fill_float_length_and_bounds() {
        nrand_seed(&[i(7)], span()).unwrap();
        let v = nrand_fill_float(&[i(500), f(2.0), f(5.0)], span()).unwrap();
        match &*v.borrow() {
            Value::FloatArray(items) => {
                assert_eq!(items.len(), 500);
                assert!(items.iter().all(|f| (2.0..=5.0).contains(f)));
            }
            other => panic!("expected float array, got {other:?}"),
        }
        let v = nrand_fill_float(&[i(100)], span()).unwrap();
        match &*v.borrow() {
            Value::FloatArray(items) => {
                assert_eq!(items.len(), 100);
                assert!(items.iter().all(|f| (0.0..1.0).contains(f)));
            }
            other => panic!("expected float array, got {other:?}"),
        }
    }

    #[test]
    fn fill_rejects_invalid_range() {
        let v = nrand_fill_int(&[i(10), i(5), i(1)], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
        let v = nrand_fill_float(&[i(10), f(3.0), f(1.0)], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn shuffle_keeps_elements() {
        let arr = Value::IntArray((0..100).collect()).ref_cell();
        nrand_shuffle(&[arr.clone()], span()).unwrap();
        match &*arr.borrow() {
            Value::IntArray(v) => {
                let mut sorted = v.clone();
                sorted.sort_unstable();
                assert_eq!(sorted, (0..100).collect::<Vec<i64>>());
            }
            other => panic!("expected int array, got {other:?}"),
        };
    }

    #[test]
    fn sample_size_and_uniqueness() {
        let arr = Value::IntArray((0..50).collect()).ref_cell();
        let s = nrand_sample(&[arr, i(10)], span()).unwrap();
        match &*s.borrow() {
            Value::Array(items) => {
                assert_eq!(items.len(), 10);
                let mut seen: Vec<i64> = items
                    .iter()
                    .map(|v| match &*v.borrow() {
                        Value::Int(n) => *n,
                        other => panic!("expected int, got {other:?}"),
                    })
                    .collect();
                seen.sort_unstable();
                seen.dedup();
                assert_eq!(seen.len(), 10);
            }
            other => panic!("expected array, got {other:?}"),
        };
    }

    #[test]
    fn invalid_handle_is_error_value() {
        let v = nrand_gen_float(&[i(99_999)], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }
}
