//! Native neval standard library — model evaluation metrics: exact match,
//! token-F1, similarity, classification/regression scores, dataset runner,
//! and latency benchmarking.
//!
//! Import with `import "neval"` (or `import "std/neval"`).

use crate::{
    call_niao_function, error_value, NativeFn, NiaoResult, RuntimeError, StringArray, Value,
    ValueRef,
};
use niao_ast::Span;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

// Wired in codes.rs by central integration.
const E2760_NEVAL_ARITY: u32 = 2760;
const E2761_NEVAL_ERROR: u32 = 2761;
const E2762_NEVAL_TYPE: u32 = 2762;
const E2763_NEVAL_SHAPE: u32 = 2763;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E2760_NEVAL_ARITY,
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
            E2760_NEVAL_ARITY,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E2762_NEVAL_TYPE, msg.into())
}

fn shape_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E2763_NEVAL_SHAPE, msg.into())
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

fn callable_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<ValueRef> {
    match &*args[idx].borrow() {
        Value::Function(_) | Value::NativeFunction(_) => Ok(Rc::clone(&args[idx])),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a function as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn optional_object_arg(args: &[ValueRef], idx: usize) -> Option<HashMap<String, ValueRef>> {
    if args.len() <= idx {
        return None;
    }
    match &*args[idx].borrow() {
        Value::Object(map) => Some(map.clone()),
        Value::Nil => None,
        _ => None,
    }
}

fn string_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: &str) -> String {
    let Some(map) = map else {
        return default.to_string();
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::String(s)) if !s.is_empty() => s,
        _ => default.to_string(),
    }
}

fn neval_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E2761_NEVAL_ERROR, "neval_error", msg.into(), span)
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn string_array_arg(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> NiaoResult<Vec<String>> {
    match &*args[idx].borrow() {
        Value::StringArray(items) => Ok(items.dense_vec()),
        Value::Array(items) => {
            let mut out = Vec::new();
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::String(s) => out.push(s.clone()),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() expects string array at argument {}, element {} is {}",
                                idx + 1,
                                i + 1,
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
                "{name}() expects a string array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn float_array_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<f64>> {
    match &*args[idx].borrow() {
        Value::FloatArray(items) => Ok(items.clone()),
        Value::IntArray(items) => Ok(items.iter().map(|&n| n as f64).collect()),
        Value::Array(items) => {
            let mut out = Vec::new();
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::Int(n) => out.push(*n as f64),
                    Value::Float(f) => out.push(*f),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() expects number array at argument {}, element {} is {}",
                                idx + 1,
                                i + 1,
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
                "{name}() expects a number array as argument {}, got {}",
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

fn dataset_arg(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> NiaoResult<Vec<HashMap<String, ValueRef>>> {
    match &*args[idx].borrow() {
        Value::Array(items) => {
            let mut out = Vec::new();
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::Object(map) => out.push(map.clone()),
                    other => {
                        return Err(shape_err(
                            span,
                            format!(
                                "{name}() dataset element {} must be an object, got {}",
                                i + 1,
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(out)
        }
        other => Err(shape_err(
            span,
            format!(
                "{name}() expects an array of objects as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

// ---------------------------------------------------------------------------
// Metric kernels
// ---------------------------------------------------------------------------

fn tokenize_words(s: &str) -> Vec<String> {
    s.split_whitespace()
        .map(|w| w.to_ascii_lowercase())
        .collect()
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let n = a_chars.len();
    let m = b_chars.len();
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut cur = vec![0; m + 1];
    for i in 1..=n {
        cur[0] = i;
        for j in 1..=m {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[m]
}

fn similarity_score(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    let max_len = a.chars().count().max(b.chars().count());
    if max_len == 0 {
        return 1.0;
    }
    let dist = levenshtein(a, b);
    1.0 - (dist as f64 / max_len as f64)
}

fn token_f1_scores(pred: &str, reference: &str) -> (f64, f64, f64) {
    let pred_tokens = tokenize_words(pred);
    let ref_tokens = tokenize_words(reference);
    if pred_tokens.is_empty() && ref_tokens.is_empty() {
        return (1.0, 1.0, 1.0);
    }
    if pred_tokens.is_empty() || ref_tokens.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let mut overlap = 0usize;
    let mut pred_counts: HashMap<&str, usize> = HashMap::new();
    let mut ref_counts: HashMap<&str, usize> = HashMap::new();
    for t in &pred_tokens {
        *pred_counts.entry(t.as_str()).or_default() += 1;
    }
    for t in &ref_tokens {
        *ref_counts.entry(t.as_str()).or_default() += 1;
    }
    for (tok, pc) in &pred_counts {
        if let Some(rc) = ref_counts.get(tok) {
            overlap += (*pc).min(*rc);
        }
    }
    let precision = overlap as f64 / pred_tokens.len() as f64;
    let recall = overlap as f64 / ref_tokens.len() as f64;
    let f1 = if precision + recall == 0.0 {
        0.0
    } else {
        2.0 * precision * recall / (precision + recall)
    };
    (precision, recall, f1)
}

fn classification_counts(
    preds: &[String],
    labels: &[String],
) -> (usize, HashMap<String, HashMap<String, i64>>) {
    let mut correct = 0usize;
    let mut matrix: HashMap<String, HashMap<String, i64>> = HashMap::new();
    for (p, l) in preds.iter().zip(labels.iter()) {
        if p == l {
            correct += 1;
        }
        matrix
            .entry(l.clone())
            .or_default()
            .entry(p.clone())
            .and_modify(|c| *c += 1)
            .or_insert(1);
    }
    (correct, matrix)
}

fn macro_prf(preds: &[String], labels: &[String]) -> (f64, f64, f64) {
    let classes: Vec<String> = labels
        .iter()
        .chain(preds.iter())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    if classes.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let mut p_sum = 0.0;
    let mut r_sum = 0.0;
    let mut f_sum = 0.0;
    for cls in &classes {
        let tp = preds
            .iter()
            .zip(labels.iter())
            .filter(|(p, l)| *p == cls && *l == cls)
            .count() as f64;
        let fp = preds
            .iter()
            .zip(labels.iter())
            .filter(|(p, l)| *p == cls && *l != cls)
            .count() as f64;
        let fn_ = preds
            .iter()
            .zip(labels.iter())
            .filter(|(p, l)| *p != cls && *l == cls)
            .count() as f64;
        let precision = if tp + fp == 0.0 { 0.0 } else { tp / (tp + fp) };
        let recall = if tp + fn_ == 0.0 {
            0.0
        } else {
            tp / (tp + fn_)
        };
        let f1 = if precision + recall == 0.0 {
            0.0
        } else {
            2.0 * precision * recall / (precision + recall)
        };
        p_sum += precision;
        r_sum += recall;
        f_sum += f1;
    }
    let n = classes.len() as f64;
    (p_sum / n, r_sum / n, f_sum / n)
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let rank = (p / 100.0) * (sorted.len() - 1) as f64;
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    if lo == hi {
        sorted[lo]
    } else {
        let w = rank - lo as f64;
        sorted[lo] * (1.0 - w) + sorted[hi] * w
    }
}

fn metrics_object(map: HashMap<String, ValueRef>) -> ValueRef {
    Value::Object(map).ref_cell()
}

fn float_metric(key: &str, v: f64) -> (String, ValueRef) {
    (key.to_string(), Value::Float(v).ref_cell())
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn neval_exact(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "neval_exact", span)?;
    let a = string_arg(args, 0, "neval_exact", span)?;
    let b = string_arg(args, 1, "neval_exact", span)?;
    Ok(Value::Bool(a == b).ref_cell())
}

fn neval_similarity(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "neval_similarity", span)?;
    let a = string_arg(args, 0, "neval_similarity", span)?;
    let b = string_arg(args, 1, "neval_similarity", span)?;
    Ok(Value::Float(similarity_score(&a, &b)).ref_cell())
}

fn neval_token_f1(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "neval_token_f1", span)?;
    let pred = string_arg(args, 0, "neval_token_f1", span)?;
    let reference = string_arg(args, 1, "neval_token_f1", span)?;
    let (precision, recall, f1) = token_f1_scores(&pred, &reference);
    Ok(metrics_object(HashMap::from([
        float_metric("precision", precision),
        float_metric("recall", recall),
        float_metric("f1", f1),
    ])))
}

fn neval_accuracy(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "neval_accuracy", span)?;
    let preds = string_array_arg(args, 0, "neval_accuracy", span)?;
    let labels = string_array_arg(args, 1, "neval_accuracy", span)?;
    if preds.len() != labels.len() {
        return Err(shape_err(
            span,
            format!(
                "neval_accuracy() preds and labels must have equal length, got {} and {}",
                preds.len(),
                labels.len()
            ),
        ));
    }
    if preds.is_empty() {
        return Ok(neval_err(
            span,
            "neval_accuracy() requires at least one sample",
        ));
    }
    let (correct, _) = classification_counts(&preds, &labels);
    Ok(Value::Float(correct as f64 / preds.len() as f64).ref_cell())
}

fn neval_precision(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "neval_precision", span)?;
    let preds = string_array_arg(args, 0, "neval_precision", span)?;
    let labels = string_array_arg(args, 1, "neval_precision", span)?;
    if preds.len() != labels.len() {
        return Err(shape_err(
            span,
            "neval_precision() preds and labels length mismatch",
        ));
    }
    let (p, _, _) = macro_prf(&preds, &labels);
    Ok(Value::Float(p).ref_cell())
}

fn neval_recall(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "neval_recall", span)?;
    let preds = string_array_arg(args, 0, "neval_recall", span)?;
    let labels = string_array_arg(args, 1, "neval_recall", span)?;
    if preds.len() != labels.len() {
        return Err(shape_err(
            span,
            "neval_recall() preds and labels length mismatch",
        ));
    }
    let (_, r, _) = macro_prf(&preds, &labels);
    Ok(Value::Float(r).ref_cell())
}

fn neval_f1(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "neval_f1", span)?;
    let preds = string_array_arg(args, 0, "neval_f1", span)?;
    let labels = string_array_arg(args, 1, "neval_f1", span)?;
    if preds.len() != labels.len() {
        return Err(shape_err(
            span,
            "neval_f1() preds and labels length mismatch",
        ));
    }
    let (_, _, f) = macro_prf(&preds, &labels);
    Ok(Value::Float(f).ref_cell())
}

fn neval_confusion(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "neval_confusion", span)?;
    let preds = string_array_arg(args, 0, "neval_confusion", span)?;
    let labels = string_array_arg(args, 1, "neval_confusion", span)?;
    if preds.len() != labels.len() {
        return Err(shape_err(
            span,
            "neval_confusion() preds and labels length mismatch",
        ));
    }
    let (_, matrix) = classification_counts(&preds, &labels);
    let mut out = HashMap::new();
    for (label, row) in matrix {
        let mut row_map = HashMap::new();
        for (pred, count) in row {
            row_map.insert(pred, Value::Int(count).ref_cell());
        }
        out.insert(label, Value::Object(row_map).ref_cell());
    }
    Ok(Value::Object(out).ref_cell())
}

fn neval_mae(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "neval_mae", span)?;
    let preds = float_array_arg(args, 0, "neval_mae", span)?;
    let labels = float_array_arg(args, 1, "neval_mae", span)?;
    if preds.len() != labels.len() || preds.is_empty() {
        return Err(shape_err(
            span,
            "neval_mae() requires equal non-empty arrays",
        ));
    }
    let sum: f64 = preds
        .iter()
        .zip(labels.iter())
        .map(|(p, l)| (p - l).abs())
        .sum();
    Ok(Value::Float(sum / preds.len() as f64).ref_cell())
}

fn neval_mse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "neval_mse", span)?;
    let preds = float_array_arg(args, 0, "neval_mse", span)?;
    let labels = float_array_arg(args, 1, "neval_mse", span)?;
    if preds.len() != labels.len() || preds.is_empty() {
        return Err(shape_err(
            span,
            "neval_mse() requires equal non-empty arrays",
        ));
    }
    let sum: f64 = preds
        .iter()
        .zip(labels.iter())
        .map(|(p, l)| {
            let d = p - l;
            d * d
        })
        .sum();
    Ok(Value::Float(sum / preds.len() as f64).ref_cell())
}

fn neval_rmse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "neval_rmse", span)?;
    let preds = float_array_arg(args, 0, "neval_rmse", span)?;
    let labels = float_array_arg(args, 1, "neval_rmse", span)?;
    if preds.len() != labels.len() || preds.is_empty() {
        return Err(shape_err(
            span,
            "neval_rmse() requires equal non-empty arrays",
        ));
    }
    let sum: f64 = preds
        .iter()
        .zip(labels.iter())
        .map(|(p, l)| {
            let d = p - l;
            d * d
        })
        .sum();
    Ok(Value::Float((sum / preds.len() as f64).sqrt()).ref_cell())
}

fn neval_r2(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "neval_r2", span)?;
    let preds = float_array_arg(args, 0, "neval_r2", span)?;
    let labels = float_array_arg(args, 1, "neval_r2", span)?;
    if preds.len() != labels.len() || preds.is_empty() {
        return Err(shape_err(
            span,
            "neval_r2() requires equal non-empty arrays",
        ));
    }
    let mean = labels.iter().sum::<f64>() / labels.len() as f64;
    let ss_tot: f64 = labels.iter().map(|y| (y - mean).powi(2)).sum();
    if ss_tot == 0.0 {
        return Ok(Value::Float(1.0).ref_cell());
    }
    let ss_res: f64 = preds
        .iter()
        .zip(labels.iter())
        .map(|(p, l)| (l - p).powi(2))
        .sum();
    Ok(Value::Float(1.0 - ss_res / ss_tot).ref_cell())
}

fn neval_bench(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "neval_bench", span)?;
    let func = callable_arg(args, 0, "neval_bench", span)?;
    let iters = if args.len() > 1 {
        let n = int_arg(args, 1, "neval_bench", span)?;
        if n <= 0 {
            return Ok(neval_err(span, "neval_bench() iters must be > 0"));
        }
        n as usize
    } else {
        100
    };
    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let start = Instant::now();
        match call_niao_function(func.clone(), &[], span) {
            Ok(result) => {
                if matches!(&*result.borrow(), Value::Error(_)) {
                    return Ok(result);
                }
            }
            Err(e) => return Err(e),
        }
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let sum: f64 = samples.iter().sum();
    let mean = sum / samples.len() as f64;
    Ok(metrics_object(HashMap::from([
        ("iters".to_string(), Value::Int(iters as i64).ref_cell()),
        float_metric("mean_ms", mean),
        float_metric("min_ms", *samples.first().unwrap_or(&0.0)),
        float_metric("max_ms", *samples.last().unwrap_or(&0.0)),
        float_metric("p50_ms", percentile(&samples, 50.0)),
        float_metric("p95_ms", percentile(&samples, 95.0)),
    ])))
}

fn neval_run(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "neval_run", span)?;
    let dataset = dataset_arg(args, 0, "neval_run", span)?;
    let predict_fn = callable_arg(args, 1, "neval_run", span)?;
    let opts = optional_object_arg(args, 2);
    let input_key = string_field(opts.as_ref(), "input_key", "input");
    let expected_key = string_field(opts.as_ref(), "expected_key", "expected");
    if dataset.is_empty() {
        return Ok(neval_err(span, "neval_run() dataset must not be empty"));
    }

    let mut preds = Vec::new();
    let mut labels = Vec::new();
    let mut exact = 0i64;
    let mut sim_sum = 0.0;
    let mut f1_sum = 0.0;

    for (i, row) in dataset.iter().enumerate() {
        let input = row.get(&input_key).ok_or_else(|| {
            shape_err(
                span,
                format!("neval_run() dataset[{i}] missing '{input_key}' field"),
            )
        })?;
        let expected = row.get(&expected_key).ok_or_else(|| {
            shape_err(
                span,
                format!("neval_run() dataset[{i}] missing '{expected_key}' field"),
            )
        })?;
        let expected_s = value_to_string(&expected.borrow());
        let pred_val = call_niao_function(predict_fn.clone(), &[input.clone()], span)?;
        if matches!(&*pred_val.borrow(), Value::Error(_)) {
            return Ok(pred_val);
        }
        let pred_s = value_to_string(&pred_val.borrow());
        if pred_s == expected_s {
            exact += 1;
        }
        sim_sum += similarity_score(&pred_s, &expected_s);
        f1_sum += token_f1_scores(&pred_s, &expected_s).2;
        preds.push(pred_s);
        labels.push(expected_s);
    }

    let n = preds.len() as f64;
    let accuracy = exact as f64 / n;
    let (_, _, macro_f1) = macro_prf(&preds, &labels);
    Ok(metrics_object(HashMap::from([
        (
            "count".to_string(),
            Value::Int(preds.len() as i64).ref_cell(),
        ),
        ("exact".to_string(), Value::Int(exact).ref_cell()),
        float_metric("accuracy", accuracy),
        float_metric("avg_similarity", sim_sum / n),
        float_metric("avg_token_f1", f1_sum / n),
        float_metric("macro_f1", macro_f1),
    ])))
}

fn neval_compare(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "neval_compare", span)?;
    let a = object_arg(args, 0, "neval_compare", span)?;
    let b = object_arg(args, 1, "neval_compare", span)?;
    let mut keys: Vec<String> = a.keys().chain(b.keys()).cloned().collect();
    keys.sort();
    keys.dedup();
    let mut deltas = HashMap::new();
    for key in keys {
        let av = a.get(&key).map(|v| v.borrow().clone());
        let bv = b.get(&key).map(|v| v.borrow().clone());
        match (av, bv) {
            (Some(Value::Float(x)), Some(Value::Float(y))) => {
                deltas.insert(key, Value::Float(x - y).ref_cell());
            }
            (Some(Value::Int(x)), Some(Value::Int(y))) => {
                deltas.insert(key, Value::Float(x as f64 - y as f64).ref_cell());
            }
            (Some(Value::Float(x)), Some(Value::Int(y))) => {
                deltas.insert(key, Value::Float(x - y as f64).ref_cell());
            }
            (Some(Value::Int(x)), Some(Value::Float(y))) => {
                deltas.insert(key, Value::Float(x as f64 - y).ref_cell());
            }
            _ => {}
        }
    }
    Ok(Value::Object(deltas).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! neval_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

neval_fns![
    ("neval_exact", "exact", neval_exact),
    ("neval_similarity", "similarity", neval_similarity),
    ("neval_token_f1", "token_f1", neval_token_f1),
    ("neval_accuracy", "accuracy", neval_accuracy),
    ("neval_precision", "precision", neval_precision),
    ("neval_recall", "recall", neval_recall),
    ("neval_f1", "f1", neval_f1),
    ("neval_confusion", "confusion", neval_confusion),
    ("neval_mae", "mae", neval_mae),
    ("neval_mse", "mse", neval_mse),
    ("neval_rmse", "rmse", neval_rmse),
    ("neval_r2", "r2", neval_r2),
    ("neval_bench", "bench", neval_bench),
    ("neval_run", "run", neval_run),
    ("neval_compare", "compare", neval_compare),
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

pub const MODULE_NAME: &str = "neval";
pub const MODULE_PATHS: &[&str] = &["neval", "std/neval"];

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
    fn exact_and_similarity() {
        assert!(matches!(
            &*neval_exact(
                &[
                    Value::String("abc".into()).ref_cell(),
                    Value::String("abc".into()).ref_cell()
                ],
                span()
            )
            .unwrap()
            .borrow(),
            Value::Bool(true)
        ));
        let sim_val = neval_similarity(
            &[
                Value::String("kitten".into()).ref_cell(),
                Value::String("sitting".into()).ref_cell(),
            ],
            span(),
        )
        .unwrap()
        .borrow()
        .clone();
        match sim_val {
            Value::Float(f) => assert!(f > 0.0 && f < 1.0),
            other => panic!("expected float, got {other:?}"),
        }
    }

    #[test]
    fn token_f1_and_classification() {
        let f1_val = neval_token_f1(
            &[
                Value::String("a b c".into()).ref_cell(),
                Value::String("a b d".into()).ref_cell(),
            ],
            span(),
        )
        .unwrap()
        .borrow()
        .clone();
        match f1_val {
            Value::Object(map) => {
                let f1v = match &*map.get("f1").unwrap().borrow() {
                    Value::Float(f) => *f,
                    other => panic!("expected float, got {other:?}"),
                };
                assert!(f1v > 0.0 && f1v < 1.0);
            }
            other => panic!("expected object, got {other:?}"),
        }
        let acc_val = neval_accuracy(
            &[
                Value::StringArray(StringArray::dense(vec!["a".into(), "b".into()])).ref_cell(),
                Value::StringArray(StringArray::dense(vec!["a".into(), "c".into()])).ref_cell(),
            ],
            span(),
        )
        .unwrap()
        .borrow()
        .clone();
        match acc_val {
            Value::Float(f) => assert!((f - 0.5).abs() < 1e-9),
            other => panic!("expected float, got {other:?}"),
        }
    }

    #[test]
    fn regression_metrics() {
        let mae_val = neval_mae(
            &[
                Value::FloatArray(vec![1.0, 2.0]).ref_cell(),
                Value::FloatArray(vec![1.5, 2.5]).ref_cell(),
            ],
            span(),
        )
        .unwrap()
        .borrow()
        .clone();
        match mae_val {
            Value::Float(f) => assert!((f - 0.5).abs() < 1e-9),
            other => panic!("expected float, got {other:?}"),
        }
    }
}
