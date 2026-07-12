//! Native nbench standard library — micro-benchmark harness:
//! `run(name, fn, opts?)` with warmup and mean/p50/p95/p99, plus compare.
//!
//! Import with `import "nbench"` (or `import "std/nbench"`).

use crate::{call_niao_function, error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

// Wired in codes.rs by central integration.
const E3170_NBENCH_ARITY: u32 = 3170;
const E3171_NBENCH_ERROR: u32 = 3171;
const E3172_NBENCH_TYPE: u32 = 3172;

// ---------------------------------------------------------------------------
// Result store
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct BenchResult {
    name: String,
    n: i64,
    warmup: i64,
    samples_ns: Vec<i64>,
}

thread_local! {
    static RESULTS: RefCell<HashMap<String, BenchResult>> = RefCell::new(HashMap::new());
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3170_NBENCH_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3170_NBENCH_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3172_NBENCH_TYPE, msg.into())
}

fn bench_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3171_NBENCH_ERROR, "nbench_error", msg.into(), span)
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

fn obj_int(map: &HashMap<String, ValueRef>, key: &str, default: i64) -> i64 {
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Int(n)) if n > 0 => n,
        _ => default,
    }
}

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

#[inline]
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

fn stats_from_samples(samples_ns: &[i64]) -> HashMap<String, ValueRef> {
    let n = samples_ns.len() as i64;
    let mut out = HashMap::new();
    out.insert("n".to_string(), Value::Int(n).ref_cell());
    if samples_ns.is_empty() {
        out.insert("mean".to_string(), Value::Float(0.0).ref_cell());
        out.insert("min".to_string(), Value::Int(0).ref_cell());
        out.insert("max".to_string(), Value::Int(0).ref_cell());
        out.insert("p50".to_string(), Value::Int(0).ref_cell());
        out.insert("p95".to_string(), Value::Int(0).ref_cell());
        out.insert("p99".to_string(), Value::Int(0).ref_cell());
        return out;
    }
    let mut sorted = samples_ns.to_vec();
    sorted.sort_unstable();
    let sum: i64 = samples_ns.iter().sum();
    let mean = sum as f64 / samples_ns.len() as f64;
    out.insert("mean".to_string(), Value::Float(mean).ref_cell());
    out.insert("min".to_string(), Value::Int(sorted[0]).ref_cell());
    out.insert(
        "max".to_string(),
        Value::Int(*sorted.last().unwrap()).ref_cell(),
    );
    out.insert("p50".to_string(), Value::Int(percentile(&sorted, 0.50)).ref_cell());
    out.insert("p95".to_string(), Value::Int(percentile(&sorted, 0.95)).ref_cell());
    out.insert("p99".to_string(), Value::Int(percentile(&sorted, 0.99)).ref_cell());
    out
}

fn result_object(result: &BenchResult) -> HashMap<String, ValueRef> {
    let mut map = stats_from_samples(&result.samples_ns);
    map.insert("name".to_string(), Value::String(result.name.clone()).ref_cell());
    map.insert("warmup".to_string(), Value::Int(result.warmup).ref_cell());
    map
}

fn collect_ns_samples(
    func: &ValueRef,
    warmup: i64,
    iterations: i64,
    span: Span,
) -> Result<Vec<i64>, ValueRef> {
    for _ in 0..warmup {
        match call_niao_function(Rc::clone(func), &[], span) {
            Ok(v) => {
                if matches!(&*v.borrow(), Value::Error(_)) {
                    return Err(bench_err(span, "benchmark fn returned error during warmup"));
                }
            }
            Err(e) => return Err(bench_err(span, format!("benchmark fn failed during warmup: {e}"))),
        }
    }
    let mut samples = Vec::with_capacity(iterations as usize);
    for _ in 0..iterations {
        let start = Instant::now();
        let outcome = call_niao_function(Rc::clone(func), &[], span);
        let ns = start.elapsed().as_nanos() as i64;
        match outcome {
            Ok(v) => {
                if matches!(&*v.borrow(), Value::Error(_)) {
                    return Err(bench_err(span, "benchmark fn returned error"));
                }
            }
            Err(e) => return Err(bench_err(span, format!("benchmark fn failed: {e}"))),
        }
        samples.push(ns);
    }
    Ok(samples)
}

fn result_from_name(name: &str, span: Span) -> Result<BenchResult, ValueRef> {
    RESULTS.with(|results| {
        results
            .borrow()
            .get(name)
            .cloned()
            .ok_or_else(|| bench_err(span, format!("no benchmark result named '{name}'")))
    })
}

fn mean_of(result: &BenchResult) -> f64 {
    if result.samples_ns.is_empty() {
        0.0
    } else {
        result.samples_ns.iter().sum::<i64>() as f64 / result.samples_ns.len() as f64
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn nbench_run(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nbench_run", span)?;
    let name = string_arg(args, 0, "nbench_run", span)?;
    let func = callable_arg(args, 1, "nbench_run", span)?;
    let (warmup, iterations) = if args.len() == 3 {
        match &*args[2].borrow() {
            Value::Object(map) => (obj_int(map, "warmup", 3), obj_int(map, "iterations", 10)),
            other => {
                return Err(type_err(
                    span,
                    format!(
                        "nbench_run() expects opts object as argument 3, got {}",
                        other.type_name()
                    ),
                ));
            }
        }
    } else {
        (3, 10)
    };
    let samples = match collect_ns_samples(&func, warmup, iterations, span) {
        Ok(s) => s,
        Err(e) => return Ok(e),
    };
    let result = BenchResult {
        name: name.clone(),
        n: iterations,
        warmup,
        samples_ns: samples,
    };
    RESULTS.with(|results| {
        results.borrow_mut().insert(name, result.clone());
    });
    Ok(Value::Object(result_object(&result)).ref_cell())
}

fn nbench_compare(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nbench_compare", span)?;
    let a = match resolve_bench_input(&args[0], span) {
        Ok(r) => r,
        Err(e) => return Ok(e),
    };
    let b = match resolve_bench_input(&args[1], span) {
        Ok(r) => r,
        Err(e) => return Ok(e),
    };
    let mean_a = mean_of(&a);
    let mean_b = mean_of(&b);
    let delta = mean_b - mean_a;
    let ratio = if mean_a > 0.0 { mean_b / mean_a } else { 0.0 };
    let faster = if mean_a < mean_b {
        a.name.clone()
    } else if mean_b < mean_a {
        b.name.clone()
    } else {
        "tie".into()
    };
    let mut map = HashMap::new();
    map.insert("a".to_string(), Value::String(a.name).ref_cell());
    map.insert("b".to_string(), Value::String(b.name).ref_cell());
    map.insert("delta_mean".to_string(), Value::Float(delta).ref_cell());
    map.insert("ratio".to_string(), Value::Float(ratio).ref_cell());
    map.insert("faster".to_string(), Value::String(faster).ref_cell());
    Ok(Value::Object(map).ref_cell())
}

fn resolve_bench_input(arg: &ValueRef, span: Span) -> Result<BenchResult, ValueRef> {
    match &*arg.borrow() {
        Value::String(name) => result_from_name(name, span),
        Value::Object(map) => {
            let name = map
                .get("name")
                .and_then(|v| match &*v.borrow() {
                    Value::String(s) => Some(s.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| "bench".into());
            let n = map
                .get("n")
                .and_then(|v| match &*v.borrow() {
                    Value::Int(n) => Some(*n),
                    _ => None,
                })
                .unwrap_or(0);
            let warmup = map
                .get("warmup")
                .and_then(|v| match &*v.borrow() {
                    Value::Int(n) => Some(*n),
                    _ => None,
                })
                .unwrap_or(0);
            let mean = map
                .get("mean")
                .and_then(|v| match &*v.borrow() {
                    Value::Float(f) => Some(*f),
                    Value::Int(n) => Some(*n as f64),
                    _ => None,
                })
                .unwrap_or(0.0);
            let samples = if mean > 0.0 && n > 0 {
                vec![mean.round() as i64; n as usize]
            } else {
                Vec::new()
            };
            Ok(BenchResult {
                name,
                n,
                warmup,
                samples_ns: samples,
            })
        }
        other => Err(bench_err(
            span,
            format!(
                "nbench_compare() expects string name or result object, got {}",
                other.type_name()
            ),
        )),
    }
}

fn nbench_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nbench_get", span)?;
    let name = string_arg(args, 0, "nbench_get", span)?;
    match result_from_name(&name, span) {
        Ok(r) => Ok(Value::Object(result_object(&r)).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nbench_stats(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nbench_stats", span)?;
    let samples = match &*args[0].borrow() {
        Value::IntArray(items) => items.clone(),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::Int(n) => out.push(*n),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "nbench_stats() expects array of ints, got {} at index {i}",
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            out
        }
        other => {
            return Err(type_err(
                span,
                format!(
                    "nbench_stats() expects IntArray or Array of ints, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    Ok(Value::Object(stats_from_samples(&samples)).ref_cell())
}

fn nbench_clear(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nbench_clear", span)?;
    if args.is_empty() {
        RESULTS.with(|r| r.borrow_mut().clear());
    } else {
        let name = string_arg(args, 0, "nbench_clear", span)?;
        RESULTS.with(|r| {
            r.borrow_mut().remove(&name);
        });
    }
    Ok(Value::Nil.ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nbench_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nbench_fns![
    ("nbench_run", "run", nbench_run),
    ("nbench_compare", "compare", nbench_compare),
    ("nbench_get", "get", nbench_get),
    ("nbench_stats", "stats", nbench_stats),
    ("nbench_clear", "clear", nbench_clear),
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

pub const MODULE_NAME: &str = "nbench";
pub const MODULE_PATHS: &[&str] = &["nbench", "std/nbench"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::dummy()
    }

    #[test]
    fn stats_percentiles() {
        let samples: Vec<i64> = (1..=100).collect();
        let map = stats_from_samples(&samples);
        assert!(matches!(&*map["n"].borrow(), Value::Int(100)));
        assert!(matches!(&*map["min"].borrow(), Value::Int(1)));
        assert!(matches!(&*map["max"].borrow(), Value::Int(100)));
        assert!(matches!(&*map["p50"].borrow(), Value::Int(n) if *n >= 50 && *n <= 51));
        assert!(matches!(&*map["p95"].borrow(), Value::Int(n) if *n >= 95));
        assert!(matches!(&*map["p99"].borrow(), Value::Int(n) if *n >= 99));
    }

    #[test]
    fn compare_from_objects() {
        let a = BenchResult {
            name: "a".into(),
            n: 10,
            warmup: 1,
            samples_ns: vec![100; 10],
        };
        let b = BenchResult {
            name: "b".into(),
            n: 10,
            warmup: 1,
            samples_ns: vec![200; 10],
        };
        let out = nbench_compare(
            &[
                Value::Object(result_object(&a)).ref_cell(),
                Value::Object(result_object(&b)).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        let out_val = out.borrow().clone();
        match out_val {
            Value::Object(map) => {
                assert!(matches!(&*map["faster"].borrow(), Value::String(s) if s == "a"));
                assert!(matches!(&*map["ratio"].borrow(), Value::Float(r) if (*r - 2.0).abs() < 1e-9));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn clear_results() {
        let _ = nbench_clear(&[], span());
        RESULTS.with(|r| {
            r.borrow_mut().insert(
                "x".into(),
                BenchResult {
                    name: "x".into(),
                    n: 1,
                    warmup: 0,
                    samples_ns: vec![1],
                },
            );
        });
        nbench_clear(&[Value::String("x".into()).ref_cell()], span()).unwrap();
        assert!(RESULTS.with(|r| !r.borrow().contains_key("x")));
    }

    #[test]
    fn arity_errors() {
        assert!(nbench_run(&[], span()).is_err());
        assert!(nbench_stats(&[Value::String("nope".into()).ref_cell()], span()).is_err());
    }
}
