//! Native ncost standard library — rough preflight USD estimates for LLM
//! tokens, S3 storage, and Lambda compute. Built-in price table with
//! thread-local overrides; no external crates.
//!
//! Import with `import "ncost"` (or `import "std/ncost"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// Wired into codes.rs by central integration.
const E2950_NCOST_ARITY: u32 = 2950;
const E2951_NCOST_ERROR: u32 = 2951;
const E2952_NCOST_TYPE: u32 = 2952;

/// USD per GB-month (rough US East standard storage).
const S3_USD_PER_GB: f64 = 0.023;
/// USD per GB-second (Lambda compute, ~128MB–1GB ballpark; we treat as 1 GB).
const LAMBDA_USD_PER_GB_S: f64 = 0.0000166667;

#[derive(Clone, Copy, Debug, PartialEq)]
struct ModelPrice {
    in_per_mtok: f64,
    out_per_mtok: f64,
}

fn builtin_prices() -> HashMap<&'static str, ModelPrice> {
    let mut m = HashMap::new();
    m.insert(
        "gpt-4o",
        ModelPrice {
            in_per_mtok: 2.5,
            out_per_mtok: 10.0,
        },
    );
    m.insert(
        "gpt-4o-mini",
        ModelPrice {
            in_per_mtok: 0.15,
            out_per_mtok: 0.6,
        },
    );
    m.insert(
        "claude-sonnet",
        ModelPrice {
            in_per_mtok: 3.0,
            out_per_mtok: 15.0,
        },
    );
    m.insert(
        "llama-local",
        ModelPrice {
            in_per_mtok: 0.0,
            out_per_mtok: 0.0,
        },
    );
    m
}

thread_local! {
    static PRICE_OVERRIDES: RefCell<HashMap<String, ModelPrice>> =
        RefCell::new(HashMap::new());
}

fn lookup_price(model: &str) -> Option<ModelPrice> {
    PRICE_OVERRIDES.with(|over| {
        if let Some(p) = over.borrow().get(model) {
            return Some(*p);
        }
        builtin_prices().get(model).copied()
    })
}

fn token_usd(price: ModelPrice, tokens_in: f64, tokens_out: f64) -> f64 {
    (tokens_in / 1_000_000.0) * price.in_per_mtok + (tokens_out / 1_000_000.0) * price.out_per_mtok
}

fn s3_usd(gb: f64) -> f64 {
    gb * S3_USD_PER_GB
}

/// Rough Lambda cost: duration_ms × requests × (USD/GB-s) / 1000, assuming 1 GB.
fn lambda_usd(ms: f64, requests: f64) -> f64 {
    (ms / 1000.0) * requests * LAMBDA_USD_PER_GB_S
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E2950_NCOST_ARITY,
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
            E2950_NCOST_ARITY,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E2952_NCOST_TYPE, msg.into())
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

fn num_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<f64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n as f64),
        Value::Float(f) => Ok(*f),
        Value::BigInt(b) => Ok(b.to_string().parse::<f64>().unwrap_or(f64::INFINITY)),
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

fn object_arg(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> NiaoResult<HashMap<String, ValueRef>> {
    match &*args[idx].borrow() {
        Value::Object(map) => Ok(map.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an object as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn opt_num_field(
    map: &HashMap<String, ValueRef>,
    key: &str,
    span: Span,
) -> NiaoResult<Option<f64>> {
    match map.get(key) {
        None => Ok(None),
        Some(v) => match &*v.borrow() {
            Value::Nil => Ok(None),
            Value::Int(n) => Ok(Some(*n as f64)),
            Value::Float(f) => Ok(Some(*f)),
            Value::BigInt(b) => Ok(Some(b.to_string().parse::<f64>().unwrap_or(f64::INFINITY))),
            other => Err(type_err(
                span,
                format!(
                    "ncost_estimate() field '{key}' expects a number, got {}",
                    other.type_name()
                ),
            )),
        },
    }
}

fn opt_string_field(
    map: &HashMap<String, ValueRef>,
    key: &str,
    span: Span,
) -> NiaoResult<Option<String>> {
    match map.get(key) {
        None => Ok(None),
        Some(v) => match &*v.borrow() {
            Value::Nil => Ok(None),
            Value::String(s) => Ok(Some(s.clone())),
            other => Err(type_err(
                span,
                format!(
                    "ncost_estimate() field '{key}' expects a string, got {}",
                    other.type_name()
                ),
            )),
        },
    }
}

fn ncost_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E2951_NCOST_ERROR, "ncost_error", msg.into(), span)
}

fn float_val(f: f64) -> NiaoResult<ValueRef> {
    Ok(Value::Float(f).ref_cell())
}

fn price_obj(p: ModelPrice) -> ValueRef {
    let mut m = HashMap::new();
    m.insert(
        "in_per_mtok".to_string(),
        Value::Float(p.in_per_mtok).ref_cell(),
    );
    m.insert(
        "out_per_mtok".to_string(),
        Value::Float(p.out_per_mtok).ref_cell(),
    );
    Value::Object(m).ref_cell()
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// ncost_price(model, tokens_in, tokens_out?) → float USD
fn ncost_price(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ncost_price", span)?;
    let model = string_arg(args, 0, "ncost_price", span)?;
    let tokens_in = num_arg(args, 1, "ncost_price", span)?;
    let tokens_out = if args.len() > 2 {
        num_arg(args, 2, "ncost_price", span)?
    } else {
        0.0
    };
    if tokens_in < 0.0 || tokens_out < 0.0 {
        return Ok(ncost_err(span, "ncost_price() token counts must be >= 0"));
    }
    match lookup_price(&model) {
        Some(p) => float_val(token_usd(p, tokens_in, tokens_out)),
        None => Ok(ncost_err(
            span,
            format!("ncost_price() unknown model '{model}' — use set_price() or table()"),
        )),
    }
}

/// ncost_estimate({model?, tokens_in?, tokens_out?, s3_gb?, lambda_ms?, requests?})
/// → {usd, breakdown: object}
fn ncost_estimate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncost_estimate", span)?;
    let map = object_arg(args, 0, "ncost_estimate", span)?;

    let model = opt_string_field(&map, "model", span)?;
    let tokens_in = opt_num_field(&map, "tokens_in", span)?.unwrap_or(0.0);
    let tokens_out = opt_num_field(&map, "tokens_out", span)?.unwrap_or(0.0);
    let s3_gb = opt_num_field(&map, "s3_gb", span)?;
    let lambda_ms = opt_num_field(&map, "lambda_ms", span)?;
    let requests = opt_num_field(&map, "requests", span)?.unwrap_or(1.0);

    if tokens_in < 0.0 || tokens_out < 0.0 {
        return Ok(ncost_err(
            span,
            "ncost_estimate() tokens_in/tokens_out must be >= 0",
        ));
    }
    if let Some(gb) = s3_gb {
        if gb < 0.0 {
            return Ok(ncost_err(span, "ncost_estimate() s3_gb must be >= 0"));
        }
    }
    if let Some(ms) = lambda_ms {
        if ms < 0.0 || requests < 0.0 {
            return Ok(ncost_err(
                span,
                "ncost_estimate() lambda_ms/requests must be >= 0",
            ));
        }
    }

    let mut breakdown = HashMap::new();
    let mut total = 0.0;

    if let Some(model_name) = model {
        match lookup_price(&model_name) {
            Some(p) => {
                let llm = token_usd(p, tokens_in, tokens_out);
                total += llm;
                breakdown.insert("llm".to_string(), Value::Float(llm).ref_cell());
                breakdown.insert("model".to_string(), Value::String(model_name).ref_cell());
            }
            None => {
                return Ok(ncost_err(
                    span,
                    format!("ncost_estimate() unknown model '{model_name}'"),
                ));
            }
        }
    } else if tokens_in != 0.0 || tokens_out != 0.0 {
        return Ok(ncost_err(
            span,
            "ncost_estimate() tokens_in/tokens_out require a model field",
        ));
    }

    if let Some(gb) = s3_gb {
        let s3 = s3_usd(gb);
        total += s3;
        breakdown.insert("s3".to_string(), Value::Float(s3).ref_cell());
    }

    if let Some(ms) = lambda_ms {
        let lam = lambda_usd(ms, requests);
        total += lam;
        breakdown.insert("lambda".to_string(), Value::Float(lam).ref_cell());
    }

    let mut out = HashMap::new();
    out.insert("usd".to_string(), Value::Float(total).ref_cell());
    out.insert("breakdown".to_string(), Value::Object(breakdown).ref_cell());
    Ok(Value::Object(out).ref_cell())
}

/// ncost_table() → {model: {in_per_mtok, out_per_mtok}, ...}
fn ncost_table(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ncost_table", span)?;
    let mut out = HashMap::new();
    for (name, p) in builtin_prices() {
        out.insert(name.to_string(), price_obj(p));
    }
    PRICE_OVERRIDES.with(|over| {
        for (name, p) in over.borrow().iter() {
            out.insert(name.clone(), price_obj(*p));
        }
    });
    Ok(Value::Object(out).ref_cell())
}

/// ncost_set_price(model, in_per_mtok, out_per_mtok) → nil
fn ncost_set_price(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "ncost_set_price", span)?;
    let model = string_arg(args, 0, "ncost_set_price", span)?;
    let in_per = num_arg(args, 1, "ncost_set_price", span)?;
    let out_per = num_arg(args, 2, "ncost_set_price", span)?;
    if in_per < 0.0 || out_per < 0.0 {
        return Ok(ncost_err(span, "ncost_set_price() prices must be >= 0"));
    }
    if model.is_empty() {
        return Ok(ncost_err(span, "ncost_set_price() model must be non-empty"));
    }
    PRICE_OVERRIDES.with(|over| {
        over.borrow_mut().insert(
            model,
            ModelPrice {
                in_per_mtok: in_per,
                out_per_mtok: out_per,
            },
        );
    });
    Ok(Value::Nil.ref_cell())
}

/// ncost_s3_cost(gb) → float USD
fn ncost_s3_cost(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncost_s3_cost", span)?;
    let gb = num_arg(args, 0, "ncost_s3_cost", span)?;
    if gb < 0.0 {
        return Ok(ncost_err(span, "ncost_s3_cost() gb must be >= 0"));
    }
    float_val(s3_usd(gb))
}

/// ncost_lambda_cost(ms, requests?) → float USD
fn ncost_lambda_cost(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ncost_lambda_cost", span)?;
    let ms = num_arg(args, 0, "ncost_lambda_cost", span)?;
    let requests = if args.len() > 1 {
        num_arg(args, 1, "ncost_lambda_cost", span)?
    } else {
        1.0
    };
    if ms < 0.0 || requests < 0.0 {
        return Ok(ncost_err(
            span,
            "ncost_lambda_cost() ms/requests must be >= 0",
        ));
    }
    float_val(lambda_usd(ms, requests))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ncost_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ncost_fns![
    ("ncost_price", "price", ncost_price),
    ("ncost_estimate", "estimate", ncost_estimate),
    ("ncost_table", "table", ncost_table),
    ("ncost_set_price", "set_price", ncost_set_price),
    ("ncost_s3_cost", "s3_cost", ncost_s3_cost),
    ("ncost_lambda_cost", "lambda_cost", ncost_lambda_cost),
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

pub const MODULE_NAME: &str = "ncost";
pub const MODULE_PATHS: &[&str] = &["ncost", "std/ncost"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::dummy()
    }

    fn clear_overrides() {
        PRICE_OVERRIDES.with(|o| o.borrow_mut().clear());
    }

    #[test]
    fn price_gpt4o() {
        clear_overrides();
        // 1M in + 1M out → 2.5 + 10 = 12.5
        let r = ncost_price(
            &[
                Value::String("gpt-4o".into()).ref_cell(),
                Value::Int(1_000_000).ref_cell(),
                Value::Int(1_000_000).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        match &*r.borrow() {
            Value::Float(f) => assert!((f - 12.5).abs() < 1e-9),
            other => panic!("expected float, got {other:?}"),
        }
    }

    #[test]
    fn price_mini_and_local() {
        clear_overrides();
        let mini = ncost_price(
            &[
                Value::String("gpt-4o-mini".into()).ref_cell(),
                Value::Float(1_000_000.0).ref_cell(),
                Value::Float(0.0).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        assert!(matches!(&*mini.borrow(), Value::Float(f) if (f - 0.15).abs() < 1e-9));

        let local = ncost_price(
            &[
                Value::String("llama-local".into()).ref_cell(),
                Value::Int(50_000).ref_cell(),
                Value::Int(10_000).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        assert!(matches!(&*local.borrow(), Value::Float(f) if *f == 0.0));
    }

    #[test]
    fn price_unknown_is_error() {
        clear_overrides();
        let r = ncost_price(
            &[
                Value::String("mystery-model".into()).ref_cell(),
                Value::Int(100).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        assert!(matches!(&*r.borrow(), Value::Error(_)));
    }

    #[test]
    fn set_price_override() {
        clear_overrides();
        ncost_set_price(
            &[
                Value::String("custom".into()).ref_cell(),
                Value::Float(1.0).ref_cell(),
                Value::Float(2.0).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        let r = ncost_price(
            &[
                Value::String("custom".into()).ref_cell(),
                Value::Int(1_000_000).ref_cell(),
                Value::Int(1_000_000).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        assert!(matches!(&*r.borrow(), Value::Float(f) if (f - 3.0).abs() < 1e-9));

        // Override built-in
        ncost_set_price(
            &[
                Value::String("gpt-4o".into()).ref_cell(),
                Value::Float(0.0).ref_cell(),
                Value::Float(0.0).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        let r2 = ncost_price(
            &[
                Value::String("gpt-4o".into()).ref_cell(),
                Value::Int(1_000_000).ref_cell(),
                Value::Int(1_000_000).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        assert!(matches!(&*r2.borrow(), Value::Float(f) if *f == 0.0));
        clear_overrides();
    }

    #[test]
    fn table_lists_builtins() {
        clear_overrides();
        let t = ncost_table(&[], span()).unwrap();
        match &*t.borrow() {
            Value::Object(map) => {
                assert!(map.contains_key("gpt-4o"));
                assert!(map.contains_key("gpt-4o-mini"));
                assert!(map.contains_key("claude-sonnet"));
                assert!(map.contains_key("llama-local"));
                match &*map.get("claude-sonnet").unwrap().borrow() {
                    Value::Object(p) => {
                        assert!(matches!(
                            &*p.get("in_per_mtok").unwrap().borrow(),
                            Value::Float(f) if (*f - 3.0).abs() < 1e-9
                        ));
                        assert!(matches!(
                            &*p.get("out_per_mtok").unwrap().borrow(),
                            Value::Float(f) if (*f - 15.0).abs() < 1e-9
                        ));
                    }
                    other => panic!("expected price object, got {other:?}"),
                }
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn s3_and_lambda_helpers() {
        let s3 = ncost_s3_cost(&[Value::Float(10.0).ref_cell()], span()).unwrap();
        assert!(matches!(&*s3.borrow(), Value::Float(f) if (f - 0.23).abs() < 1e-9));

        // 1000 ms × 1 request × 0.0000166667 = 0.0000166667
        let lam = ncost_lambda_cost(
            &[Value::Int(1000).ref_cell(), Value::Int(1).ref_cell()],
            span(),
        )
        .unwrap();
        assert!(matches!(
            &*lam.borrow(),
            Value::Float(f) if (f - LAMBDA_USD_PER_GB_S).abs() < 1e-12
        ));
    }

    #[test]
    fn estimate_combines_parts() {
        clear_overrides();
        let mut obj = HashMap::new();
        obj.insert(
            "model".to_string(),
            Value::String("gpt-4o-mini".into()).ref_cell(),
        );
        obj.insert("tokens_in".to_string(), Value::Int(1_000_000).ref_cell());
        obj.insert("tokens_out".to_string(), Value::Int(0).ref_cell());
        obj.insert("s3_gb".to_string(), Value::Float(1.0).ref_cell());
        obj.insert("lambda_ms".to_string(), Value::Int(1000).ref_cell());
        obj.insert("requests".to_string(), Value::Int(1).ref_cell());

        let r = ncost_estimate(&[Value::Object(obj).ref_cell()], span()).unwrap();
        match &*r.borrow() {
            Value::Object(map) => {
                let usd = match &*map.get("usd").unwrap().borrow() {
                    Value::Float(f) => *f,
                    other => panic!("expected float usd, got {other:?}"),
                };
                // 0.15 + 0.023 + 0.0000166667
                let expected = 0.15 + S3_USD_PER_GB + LAMBDA_USD_PER_GB_S;
                assert!((usd - expected).abs() < 1e-9);
                match &*map.get("breakdown").unwrap().borrow() {
                    Value::Object(b) => {
                        assert!(b.contains_key("llm"));
                        assert!(b.contains_key("s3"));
                        assert!(b.contains_key("lambda"));
                    }
                    other => panic!("expected breakdown object, got {other:?}"),
                }
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn arity_and_type_errors() {
        clear_overrides();
        assert!(ncost_price(&[], span()).is_err());
        assert!(ncost_table(&[Value::Int(1).ref_cell()], span()).is_err());
        assert!(ncost_s3_cost(&[Value::String("x".into()).ref_cell()], span()).is_err());
        assert!(ncost_estimate(&[Value::Int(1).ref_cell()], span()).is_err());
    }
}
