//! Native nprofile standard library — micro timing spans, named sample
//! recording, and latency stats (mean / min / max / p50 / p95).
//!
//! Import with `import "nprofile"` (or `import "std/nprofile"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

// Wired in codes.rs by central integration.
const E3150_NPROFILE_ARITY: u32 = 3150;
const E3151_NPROFILE_ERROR: u32 = 3151;
const E3152_NPROFILE_TYPE: u32 = 3152;

// ---------------------------------------------------------------------------
// Span + sample stores
// ---------------------------------------------------------------------------

struct ActiveSpan {
    label: String,
    start: Instant,
}

thread_local! {
    static SPANS: RefCell<HashMap<i64, ActiveSpan>> = RefCell::new(HashMap::new());
    static NEXT_HANDLE: RefCell<i64> = const { RefCell::new(1) };
    static SAMPLES: RefCell<HashMap<String, Vec<i64>>> = RefCell::new(HashMap::new());
}

fn new_handle() -> i64 {
    NEXT_HANDLE.with(|h| {
        let mut h = h.borrow_mut();
        let id = *h;
        *h += 1;
        id
    })
}

fn wall_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3150_NPROFILE_ARITY,
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
            E3150_NPROFILE_ARITY,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3152_NPROFILE_TYPE, msg.into())
}

fn profile_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3151_NPROFILE_ERROR, "nprofile_error", msg.into(), span)
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

fn ms_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        Value::Float(f) => Ok(f.round() as i64),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an int or float as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn collect_ms_array(v: &Value, name: &str, span: Span) -> NiaoResult<Vec<i64>> {
    match v {
        Value::IntArray(items) => Ok(items.clone()),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::Int(n) => out.push(*n),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() expects array of ints, got {} at index {i}",
                                other.type_name()
                            ),
                        ))
                    }
                }
            }
            Ok(out)
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an IntArray or Array of ints, got {}",
                other.type_name()
            ),
        )),
    }
}

// ---------------------------------------------------------------------------
// Stats helpers
// ---------------------------------------------------------------------------

/// Linear-interpolation percentile on a sorted ascending slice.
fn percentile(sorted: &[i64], p: f64) -> i64 {
    if sorted.is_empty() {
        return 0;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let rank = p.clamp(0.0, 1.0) * (sorted.len() - 1) as f64;
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    if lo == hi {
        sorted[lo]
    } else {
        let frac = rank - lo as f64;
        (sorted[lo] as f64 * (1.0 - frac) + sorted[hi] as f64 * frac).round() as i64
    }
}

fn compute_stats(samples: &[i64]) -> HashMap<String, ValueRef> {
    let n = samples.len() as i64;
    let mut out = HashMap::new();
    out.insert("n".to_string(), Value::Int(n).ref_cell());
    if samples.is_empty() {
        out.insert("mean".to_string(), Value::Float(0.0).ref_cell());
        out.insert("min".to_string(), Value::Int(0).ref_cell());
        out.insert("max".to_string(), Value::Int(0).ref_cell());
        out.insert("p50".to_string(), Value::Int(0).ref_cell());
        out.insert("p95".to_string(), Value::Int(0).ref_cell());
        return out;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let sum: i64 = samples.iter().sum();
    let mean = sum as f64 / samples.len() as f64;
    out.insert("mean".to_string(), Value::Float(mean).ref_cell());
    out.insert("min".to_string(), Value::Int(sorted[0]).ref_cell());
    out.insert(
        "max".to_string(),
        Value::Int(*sorted.last().unwrap()).ref_cell(),
    );
    out.insert(
        "p50".to_string(),
        Value::Int(percentile(&sorted, 0.50)).ref_cell(),
    );
    out.insert(
        "p95".to_string(),
        Value::Int(percentile(&sorted, 0.95)).ref_cell(),
    );
    out
}

fn start_span(label: String) -> i64 {
    let id = new_handle();
    SPANS.with(|spans| {
        spans.borrow_mut().insert(
            id,
            ActiveSpan {
                label,
                start: Instant::now(),
            },
        );
    });
    id
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// nprofile_now_ms() → int (wall-clock unix ms)
fn nprofile_now_ms(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nprofile_now_ms", span)?;
    Ok(Value::Int(wall_now_ms()).ref_cell())
}

/// nprofile_start(label) → handle int
fn nprofile_start(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nprofile_start", span)?;
    let label = string_arg(args, 0, "nprofile_start", span)?;
    Ok(Value::Int(start_span(label)).ref_cell())
}

/// nprofile_span(label) → handle int (alias of start)
fn nprofile_span(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nprofile_span", span)?;
    let label = string_arg(args, 0, "nprofile_span", span)?;
    Ok(Value::Int(start_span(label)).ref_cell())
}

/// nprofile_end(h) → {label, ms} and remove; catchable error on bad handle
fn nprofile_end(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nprofile_end", span)?;
    let id = int_arg(args, 0, "nprofile_end", span)?;
    let removed = SPANS.with(|spans| spans.borrow_mut().remove(&id));
    match removed {
        Some(ActiveSpan { label, start }) => {
            let mut map = HashMap::new();
            map.insert("label".to_string(), Value::String(label).ref_cell());
            map.insert("ms".to_string(), Value::Float(elapsed_ms(start)).ref_cell());
            Ok(Value::Object(map).ref_cell())
        }
        None => Ok(profile_err(
            span,
            format!("invalid or closed span handle {id}"),
        )),
    }
}

/// nprofile_stats(ms_array) → {n, mean, min, max, p50, p95}
fn nprofile_stats(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nprofile_stats", span)?;
    let samples = collect_ms_array(&args[0].borrow(), "nprofile_stats", span)?;
    Ok(Value::Object(compute_stats(&samples)).ref_cell())
}

/// nprofile_record(label, ms) → nil (append to named samples)
fn nprofile_record(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nprofile_record", span)?;
    let label = string_arg(args, 0, "nprofile_record", span)?;
    let ms = ms_arg(args, 1, "nprofile_record", span)?;
    SAMPLES.with(|samples| {
        samples.borrow_mut().entry(label).or_default().push(ms);
    });
    Ok(Value::Nil.ref_cell())
}

/// nprofile_samples(label) → array of ints
fn nprofile_samples(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nprofile_samples", span)?;
    let label = string_arg(args, 0, "nprofile_samples", span)?;
    let items = SAMPLES.with(|samples| samples.borrow().get(&label).cloned().unwrap_or_default());
    Ok(Value::Array(
        items
            .into_iter()
            .map(|n| Value::Int(n).ref_cell())
            .collect(),
    )
    .ref_cell())
}

/// nprofile_clear(label?) → nil — clear one label or all samples
fn nprofile_clear(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nprofile_clear", span)?;
    if args.is_empty() {
        SAMPLES.with(|samples| samples.borrow_mut().clear());
    } else {
        let label = string_arg(args, 0, "nprofile_clear", span)?;
        SAMPLES.with(|samples| {
            samples.borrow_mut().remove(&label);
        });
    }
    Ok(Value::Nil.ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nprofile_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nprofile_fns![
    ("nprofile_now_ms", "now_ms", nprofile_now_ms),
    ("nprofile_start", "start", nprofile_start),
    ("nprofile_end", "end", nprofile_end),
    ("nprofile_span", "span", nprofile_span),
    ("nprofile_stats", "stats", nprofile_stats),
    ("nprofile_record", "record", nprofile_record),
    ("nprofile_samples", "samples", nprofile_samples),
    ("nprofile_clear", "clear", nprofile_clear),
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

pub const MODULE_NAME: &str = "nprofile";
pub const MODULE_PATHS: &[&str] = &["nprofile", "std/nprofile"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;
    use std::thread;
    use std::time::Duration;

    fn span() -> Span {
        Span::dummy()
    }

    fn expect_object(result: NiaoResult<ValueRef>) -> HashMap<String, ValueRef> {
        match &*result.unwrap().borrow() {
            Value::Object(map) => map.clone(),
            other => panic!("expected object, got {other:?}"),
        }
    }

    fn expect_int(result: NiaoResult<ValueRef>) -> i64 {
        match &*result.unwrap().borrow() {
            Value::Int(n) => *n,
            other => panic!("expected int, got {other:?}"),
        }
    }

    fn expect_error(result: NiaoResult<ValueRef>) {
        match &*result.unwrap().borrow() {
            Value::Error(_) => {}
            other => panic!("expected error, got {other:?}"),
        }
    }

    #[test]
    fn now_ms_is_positive() {
        let n = expect_int(nprofile_now_ms(&[], span()));
        assert!(n > 0);
    }

    #[test]
    fn start_end_span() {
        let h = expect_int(nprofile_start(
            &[Value::String("work".into()).ref_cell()],
            span(),
        ));
        assert!(h > 0);
        thread::sleep(Duration::from_millis(2));
        let map = expect_object(nprofile_end(&[Value::Int(h).ref_cell()], span()));
        assert!(matches!(&*map["label"].borrow(), Value::String(s) if s == "work"));
        match &*map["ms"].borrow() {
            Value::Float(ms) => assert!(*ms >= 0.0),
            other => panic!("expected float ms, got {other:?}"),
        }
        // second end on same handle → catchable error
        expect_error(nprofile_end(&[Value::Int(h).ref_cell()], span()));
    }

    #[test]
    fn span_aliases_start() {
        let h = expect_int(nprofile_span(
            &[Value::String("nested".into()).ref_cell()],
            span(),
        ));
        let map = expect_object(nprofile_end(&[Value::Int(h).ref_cell()], span()));
        assert!(matches!(&*map["label"].borrow(), Value::String(s) if s == "nested"));
    }

    #[test]
    fn stats_from_array_and_int_array() {
        let arr = Value::Array(
            [10, 20, 30, 40, 50]
                .into_iter()
                .map(|n| Value::Int(n).ref_cell())
                .collect(),
        )
        .ref_cell();
        let map = expect_object(nprofile_stats(&[arr], span()));
        assert!(matches!(&*map["n"].borrow(), Value::Int(5)));
        assert!(matches!(&*map["min"].borrow(), Value::Int(10)));
        assert!(matches!(&*map["max"].borrow(), Value::Int(50)));
        assert!(matches!(&*map["mean"].borrow(), Value::Float(m) if (*m - 30.0).abs() < 1e-9));
        assert!(matches!(&*map["p50"].borrow(), Value::Int(30)));

        let ia = Value::IntArray(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 100]).ref_cell();
        let map2 = expect_object(nprofile_stats(&[ia], span()));
        assert!(matches!(&*map2["n"].borrow(), Value::Int(10)));
        assert!(matches!(&*map2["min"].borrow(), Value::Int(1)));
        assert!(matches!(&*map2["max"].borrow(), Value::Int(100)));
        assert!(matches!(&*map2["p50"].borrow(), Value::Int(n) if *n >= 5 && *n <= 6));
        assert!(matches!(&*map2["p95"].borrow(), Value::Int(n) if *n >= 9));
    }

    #[test]
    fn stats_empty() {
        let map = expect_object(nprofile_stats(
            &[Value::IntArray(vec![]).ref_cell()],
            span(),
        ));
        assert!(matches!(&*map["n"].borrow(), Value::Int(0)));
        assert!(matches!(&*map["mean"].borrow(), Value::Float(0.0)));
    }

    #[test]
    fn record_samples_clear() {
        // Isolate from other tests sharing the same TLS map.
        let _ = nprofile_clear(&[], span());

        let _ = nprofile_record(
            &[
                Value::String("db".into()).ref_cell(),
                Value::Int(12).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        let _ = nprofile_record(
            &[
                Value::String("db".into()).ref_cell(),
                Value::Float(8.4).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        let _ = nprofile_record(
            &[
                Value::String("http".into()).ref_cell(),
                Value::Int(3).ref_cell(),
            ],
            span(),
        )
        .unwrap();

        match &*nprofile_samples(&[Value::String("db".into()).ref_cell()], span())
            .unwrap()
            .borrow()
        {
            Value::Array(items) => {
                assert_eq!(items.len(), 2);
                assert!(matches!(&*items[0].borrow(), Value::Int(12)));
                assert!(matches!(&*items[1].borrow(), Value::Int(8)));
            }
            other => panic!("expected array, got {other:?}"),
        }

        let _ = nprofile_clear(&[Value::String("db".into()).ref_cell()], span()).unwrap();
        match &*nprofile_samples(&[Value::String("db".into()).ref_cell()], span())
            .unwrap()
            .borrow()
        {
            Value::Array(items) => assert!(items.is_empty()),
            other => panic!("expected array, got {other:?}"),
        }
        match &*nprofile_samples(&[Value::String("http".into()).ref_cell()], span())
            .unwrap()
            .borrow()
        {
            Value::Array(items) => assert_eq!(items.len(), 1),
            other => panic!("expected array, got {other:?}"),
        }

        let _ = nprofile_clear(&[], span()).unwrap();
        match &*nprofile_samples(&[Value::String("http".into()).ref_cell()], span())
            .unwrap()
            .borrow()
        {
            Value::Array(items) => assert!(items.is_empty()),
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[test]
    fn arity_and_type_errors() {
        assert!(nprofile_now_ms(&[Value::Int(1).ref_cell()], span()).is_err());
        assert!(nprofile_start(&[], span()).is_err());
        assert!(nprofile_start(&[Value::Int(1).ref_cell()], span()).is_err());
        assert!(nprofile_stats(&[Value::String("nope".into()).ref_cell()], span()).is_err());
        assert!(nprofile_stats(
            &[Value::Array(vec![Value::String("x".into()).ref_cell()]).ref_cell()],
            span()
        )
        .is_err());
    }

    #[test]
    fn namespace_exports_short_names() {
        match namespace() {
            Value::Object(map) => {
                for key in [
                    "now_ms", "start", "end", "span", "stats", "record", "samples", "clear",
                ] {
                    assert!(map.contains_key(key), "missing {key}");
                }
            }
            other => panic!("expected object namespace, got {other:?}"),
        }
    }
}
