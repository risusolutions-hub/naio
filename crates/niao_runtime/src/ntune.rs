//! Native ntune standard library — hyperparameter search: grid, random,
//! successive halving over nlearn/neval budgets, train/test split, and k-fold.
//!
//! Import with `import "ntune"` (or `import "std/ntune"`).

use crate::{
    call_niao_function, error_value, resolve_niao_function_by_name, NativeFn, NiaoResult,
    RuntimeError, Value, ValueRef,
};
use niao_ast::Span;
use niao_errors::codes;
use niao_tune::{
    best_trial, grid_cartesian, grid_size, kfold_indices, run_grid, run_halving, run_random,
    sample_random, train_test_split_indices, validate_space, HalvingConfig, ParamValue,
    SearchDirection, SearchOpts, SearchResult, SpaceDim, TrialRecord, TuneError,
};
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E2752_NTUNE_TYPE, msg.into())
}

fn space_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E2753_NTUNE_SPACE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E2750_NTUNE_ARITY,
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
            codes::E2750_NTUNE_ARITY,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
        ));
    }
    Ok(())
}

fn ntune_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E2751_NTUNE_ERROR, "ntune_error", msg.into(), span)
}

fn tune_err_to_value(span: Span, e: TuneError) -> ValueRef {
    ntune_err(span, e.message())
}

fn callable_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<ValueRef> {
    match &*args[idx].borrow() {
        Value::Function(_) | Value::NativeFunction(_) => Ok(Rc::clone(&args[idx])),
        Value::String(s) => resolve_niao_function_by_name(s).ok_or_else(|| {
            type_err(
                span,
                format!("{name}() unknown function '{s}' as argument {}", idx + 1),
            )
        }),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a function or function name as argument {}, got {}",
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

fn float_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<f64> {
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

fn bool_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: bool) -> bool {
    let Some(map) = map else {
        return default;
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Bool(b)) => b,
        _ => default,
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

fn int_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: i64) -> i64 {
    let Some(map) = map else {
        return default;
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Int(n)) => n,
        _ => default,
    }
}

fn u64_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: u64) -> u64 {
    let n = int_field(map, key, default as i64);
    if n < 0 {
        default
    } else {
        n as u64
    }
}

fn invoke_callable(callee: &ValueRef, args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    match &*callee.borrow() {
        Value::NativeFunction(native) => native(args, span),
        Value::Function(_) => call_niao_function(Rc::clone(callee), args, span),
        other => Err(type_err(
            span,
            format!("expected callable, got {}", other.type_name()),
        )),
    }
}

fn value_to_f64(v: &Value, span: Span, ctx: &str) -> Result<f64, ValueRef> {
    match v {
        Value::Int(n) => Ok(*n as f64),
        Value::Float(f) => Ok(*f),
        other => Err(ntune_err(
            span,
            format!("{ctx} must return a number, got {}", other.type_name()),
        )),
    }
}

// ---------------------------------------------------------------------------
// Space / param conversion
// ---------------------------------------------------------------------------

fn value_to_param(v: &Value, span: Span, ctx: &str) -> Result<ParamValue, RuntimeError> {
    match v {
        Value::Int(n) => Ok(ParamValue::Int(*n)),
        Value::Float(f) => Ok(ParamValue::Float(*f)),
        Value::String(s) => Ok(ParamValue::String(s.clone())),
        Value::Bool(b) => Ok(ParamValue::Bool(*b)),
        other => Err(type_err(
            span,
            format!("{ctx}: unsupported param value type {}", other.type_name()),
        )),
    }
}

fn param_to_value(p: &ParamValue) -> ValueRef {
    match p {
        ParamValue::Int(n) => Value::Int(*n).ref_cell(),
        ParamValue::Float(f) => Value::Float(*f).ref_cell(),
        ParamValue::String(s) => Value::String(s.clone()).ref_cell(),
        ParamValue::Bool(b) => Value::Bool(*b).ref_cell(),
    }
}

fn params_to_object(params: &BTreeMap<String, ParamValue>) -> ValueRef {
    let mut map = HashMap::new();
    for (k, v) in params {
        map.insert(k.clone(), param_to_value(v));
    }
    Value::Object(map).ref_cell()
}

fn parse_space_dim(v: &Value, span: Span, name: &str) -> Result<SpaceDim, RuntimeError> {
    match v {
        Value::Array(items) => {
            let mut grid = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                grid.push(value_to_param(
                    &item.borrow(),
                    span,
                    &format!("space.{name}[{i}]"),
                )?);
            }
            Ok(SpaceDim::Grid(grid))
        }
        Value::Object(spec) => {
            // Grid list disguised as object values? Treat explicit array field `choices`.
            if let Some(choices) = spec.get("choices") {
                return parse_space_dim(&choices.borrow(), span, name);
            }
            let ty = spec
                .get("type")
                .map(|v| match &*v.borrow() {
                    Value::String(s) => s.clone(),
                    _ => String::new(),
                })
                .unwrap_or_default();
            let log = spec
                .get("log")
                .map(|v| matches!(&*v.borrow(), Value::Bool(true)))
                .unwrap_or(false);
            match ty.as_str() {
                "float" | "double" => {
                    let low = spec.get("low").ok_or_else(|| {
                        space_err(span, format!("space.{name}: float requires low"))
                    })?;
                    let high = spec.get("high").ok_or_else(|| {
                        space_err(span, format!("space.{name}: float requires high"))
                    })?;
                    Ok(SpaceDim::Float {
                        low: float_arg(&[low.clone()], 0, "space", span)?,
                        high: float_arg(&[high.clone()], 0, "space", span)?,
                        log,
                    })
                }
                "int" | "integer" => {
                    let low = spec.get("low").ok_or_else(|| {
                        space_err(span, format!("space.{name}: int requires low"))
                    })?;
                    let high = spec.get("high").ok_or_else(|| {
                        space_err(span, format!("space.{name}: int requires high"))
                    })?;
                    Ok(SpaceDim::Int {
                        low: int_arg(&[low.clone()], 0, "space", span)?,
                        high: int_arg(&[high.clone()], 0, "space", span)?,
                        log,
                    })
                }
                "categorical" | "cat" => {
                    let choices = spec.get("choices").ok_or_else(|| {
                        space_err(span, format!("space.{name}: categorical requires choices"))
                    })?;
                    parse_space_dim(&choices.borrow(), span, name)
                }
                "" if spec.contains_key("low") && spec.contains_key("high") => {
                    let low_v = spec.get("low").unwrap();
                    match &*low_v.borrow() {
                        Value::Int(_) => {
                            let high = spec.get("high").unwrap();
                            Ok(SpaceDim::Int {
                                low: int_arg(&[low_v.clone()], 0, "space", span)?,
                                high: int_arg(&[high.clone()], 0, "space", span)?,
                                log,
                            })
                        }
                        _ => {
                            let high = spec.get("high").unwrap();
                            Ok(SpaceDim::Float {
                                low: float_arg(&[low_v.clone()], 0, "space", span)?,
                                high: float_arg(&[high.clone()], 0, "space", span)?,
                                log,
                            })
                        }
                    }
                }
                other => Err(space_err(
                    span,
                    format!("space.{name}: unknown type '{other}'"),
                )),
            }
        }
        other => Err(type_err(
            span,
            format!(
                "space.{name}: expected array or spec object, got {}",
                other.type_name()
            ),
        )),
    }
}

fn parse_space(
    map: &HashMap<String, ValueRef>,
    span: Span,
) -> Result<BTreeMap<String, SpaceDim>, RuntimeError> {
    let mut space = BTreeMap::new();
    for (name, val) in map {
        space.insert(name.clone(), parse_space_dim(&val.borrow(), span, name)?);
    }
    Ok(space)
}

fn parse_direction(map: Option<&HashMap<String, ValueRef>>) -> Result<SearchDirection, TuneError> {
    let s = string_field(map, "direction", "minimize");
    SearchDirection::from_str(&s)
}

fn parse_search_opts(map: Option<&HashMap<String, ValueRef>>) -> Result<SearchOpts, TuneError> {
    Ok(SearchOpts {
        direction: parse_direction(map)?,
        seed: int_field(map, "seed", 0) as u64,
    })
}

fn trial_to_object(t: &TrialRecord) -> ValueRef {
    let mut map = HashMap::new();
    map.insert("trial".into(), Value::Int(t.trial as i64).ref_cell());
    map.insert("params".into(), params_to_object(&t.params));
    map.insert("value".into(), Value::Float(t.value).ref_cell());
    if let Some(b) = t.budget {
        map.insert("budget".into(), Value::Int(b as i64).ref_cell());
    }
    let status = match t.status {
        niao_tune::TrialStatus::Complete => "complete",
        niao_tune::TrialStatus::Pruned => "pruned",
    };
    map.insert("status".into(), Value::String(status.into()).ref_cell());
    Value::Object(map).ref_cell()
}

fn search_result_to_object(result: SearchResult) -> ValueRef {
    let mut map = HashMap::new();
    let trials: Vec<ValueRef> = result.trials.iter().map(trial_to_object).collect();
    map.insert("trials".into(), Value::Array(trials).ref_cell());
    map.insert(
        "n_trials".into(),
        Value::Int(result.trials.len() as i64).ref_cell(),
    );
    if let Some(best) = result.best {
        map.insert("best".into(), trial_to_object(&best));
    } else {
        map.insert("best".into(), Value::Nil.ref_cell());
    }
    let dir = match result.direction {
        SearchDirection::Minimize => "minimize",
        SearchDirection::Maximize => "maximize",
    };
    map.insert("direction".into(), Value::String(dir.into()).ref_cell());
    Value::Object(map).ref_cell()
}

fn eval_objective(
    func: &ValueRef,
    params: &BTreeMap<String, ParamValue>,
    budget: Option<u64>,
    span: Span,
) -> Result<f64, ValueRef> {
    let params_obj = params_to_object(params);
    let out = if let Some(b) = budget {
        invoke_callable(func, &[params_obj, Value::Int(b as i64).ref_cell()], span)
    } else {
        invoke_callable(func, &[params_obj], span)
    };
    match out {
        Ok(v) => {
            if matches!(&*v.borrow(), Value::Error(_)) {
                return Err(v);
            }
            value_to_f64(&v.borrow(), span, "objective")
        }
        Err(e) => Err(ntune_err(span, e.message())),
    }
}

// ---------------------------------------------------------------------------
// Public builtins
// ---------------------------------------------------------------------------

// >>> ntune.grid_size({lr: [0.01, 0.1], depth: [3, 5]})
// => 4
fn ntune_grid_size(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntune_grid_size", span)?;
    let map = object_arg(args, 0, "ntune_grid_size", span)?;
    let space = parse_space(&map, span)?;
    match grid_size(&space) {
        Ok(n) => Ok(Value::Int(n as i64).ref_cell()),
        Err(e) => Ok(tune_err_to_value(span, e)),
    }
}

// >>> len(ntune.grid_points({x: [1, 2]}))
// => 2
fn ntune_grid_points(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntune_grid_points", span)?;
    let map = object_arg(args, 0, "ntune_grid_points", span)?;
    let space = parse_space(&map, span)?;
    match grid_cartesian(&space) {
        Ok(combos) => {
            let items: Vec<ValueRef> = combos.iter().map(params_to_object).collect();
            Ok(Value::Array(items).ref_cell())
        }
        Err(e) => Ok(tune_err_to_value(span, e)),
    }
}

// >>> len(ntune.sample({x: {type: "int", low: 0, high: 3}}, 5, 1))
// => 5
fn ntune_sample(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntune_sample", span)?;
    let map = object_arg(args, 0, "ntune_sample", span)?;
    let n = int_arg(args, 1, "ntune_sample", span)?;
    if n < 0 {
        return Ok(ntune_err(span, "sample() n must be >= 0"));
    }
    let seed = if args.len() > 2 {
        int_arg(args, 2, "ntune_sample", span)? as u64
    } else {
        0
    };
    let space = parse_space(&map, span)?;
    match sample_random(&space, n as usize, seed) {
        Ok(combos) => {
            let items: Vec<ValueRef> = combos.iter().map(params_to_object).collect();
            Ok(Value::Array(items).ref_cell())
        }
        Err(e) => Ok(tune_err_to_value(span, e)),
    }
}

// >>> ntune.validate_space({lr: [0.1]})
// => true
fn ntune_validate_space(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntune_validate_space", span)?;
    let map = object_arg(args, 0, "ntune_validate_space", span)?;
    let space = parse_space(&map, span)?;
    match validate_space(&space) {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(tune_err_to_value(span, e)),
    }
}

// >>> ntune.default_opts().direction
// => "minimize"
fn ntune_default_opts(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ntune_default_opts", span)?;
    let opts = SearchOpts::default();
    let mut map = HashMap::new();
    map.insert(
        "direction".into(),
        Value::String("minimize".into()).ref_cell(),
    );
    map.insert("seed".into(), Value::Int(opts.seed as i64).ref_cell());
    map.insert("n_trials".into(), Value::Int(10).ref_cell());
    Ok(Value::Object(map).ref_cell())
}

// >>> ntune.grid_search(fn(p) { return p.lr }, {lr: [0.2, 0.1]}).best.value
fn ntune_grid_search(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntune_grid_search", span)?;
    let func = callable_arg(args, 0, "ntune_grid_search", span)?;
    let map = object_arg(args, 1, "ntune_grid_search", span)?;
    let opts_map = optional_object_arg(args, 2);
    let space = parse_space(&map, span)?;
    let opts = match parse_search_opts(opts_map.as_ref()) {
        Ok(o) => o,
        Err(e) => return Ok(tune_err_to_value(span, e)),
    };
    let func = Rc::clone(&func);
    let result = run_grid(
        &space,
        |params| {
            eval_objective(&func, params, None, span).map_err(|v| {
                TuneError::InvalidConfig(match &*v.borrow() {
                    Value::Error(e) => e.message.clone(),
                    other => other.to_string(),
                })
            })
        },
        &opts,
    );
    match result {
        Ok(r) => Ok(search_result_to_object(r)),
        Err(e) => Ok(tune_err_to_value(span, e)),
    }
}

// >>> ntune.random_search(fn(p) { return p.x }, {x: {type: "float", low: 0, high: 1}}, {n_trials: 3})
fn ntune_random_search(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntune_random_search", span)?;
    let func = callable_arg(args, 0, "ntune_random_search", span)?;
    let map = object_arg(args, 1, "ntune_random_search", span)?;
    let opts_map = optional_object_arg(args, 2);
    let n_trials = int_field(opts_map.as_ref(), "n_trials", 10);
    if n_trials <= 0 {
        return Ok(ntune_err(span, "random_search() n_trials must be > 0"));
    }
    let space = parse_space(&map, span)?;
    let opts = match parse_search_opts(opts_map.as_ref()) {
        Ok(o) => o,
        Err(e) => return Ok(tune_err_to_value(span, e)),
    };
    let func = Rc::clone(&func);
    let result = run_random(
        &space,
        n_trials as usize,
        |params| {
            eval_objective(&func, params, None, span).map_err(|v| {
                TuneError::InvalidConfig(match &*v.borrow() {
                    Value::Error(e) => e.message.clone(),
                    other => other.to_string(),
                })
            })
        },
        &opts,
    );
    match result {
        Ok(r) => Ok(search_result_to_object(r)),
        Err(e) => Ok(tune_err_to_value(span, e)),
    }
}

// >>> ntune.halving(fn(p, b) { return p.x * b }, {x: {type: "float", low: 0, high: 1}})
fn ntune_halving(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntune_halving", span)?;
    let func = callable_arg(args, 0, "ntune_halving", span)?;
    let map = object_arg(args, 1, "ntune_halving", span)?;
    let opts_map = optional_object_arg(args, 2);
    let space = parse_space(&map, span)?;
    let direction = match parse_direction(opts_map.as_ref()) {
        Ok(d) => d,
        Err(e) => return Ok(tune_err_to_value(span, e)),
    };
    let cfg = HalvingConfig {
        n_trials: int_field(opts_map.as_ref(), "n_trials", 27) as usize,
        min_resource: u64_field(opts_map.as_ref(), "min_resource", 1),
        max_resource: u64_field(opts_map.as_ref(), "max_resource", 81),
        reduction_factor: u64_field(opts_map.as_ref(), "reduction_factor", 3),
        direction,
        seed: int_field(opts_map.as_ref(), "seed", 0) as u64,
    };
    let func = Rc::clone(&func);
    let result = run_halving(&space, &cfg, |params, budget| {
        eval_objective(&func, params, Some(budget), span).map_err(|v| {
            TuneError::InvalidConfig(match &*v.borrow() {
                Value::Error(e) => e.message.clone(),
                other => other.to_string(),
            })
        })
    });
    match result {
        Ok(r) => Ok(search_result_to_object(r)),
        Err(e) => Ok(tune_err_to_value(span, e)),
    }
}

// >>> ntune.train_test_split(10, {test_size: 0.2, seed: 1}).train
fn ntune_train_test_split(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ntune_train_test_split", span)?;
    let n = int_arg(args, 0, "ntune_train_test_split", span)?;
    if n <= 0 {
        return Ok(ntune_err(span, "train_test_split() n must be > 0"));
    }
    let opts = optional_object_arg(args, 1);
    let test_size = if let Some(m) = opts.as_ref() {
        match m.get("test_size").map(|v| v.borrow().clone()) {
            Some(Value::Float(f)) if f > 0.0 && f < 1.0 => f,
            Some(Value::Int(i)) if i > 0 && (i as usize) < n as usize => i as f64 / n as f64,
            Some(Value::Int(_)) => {
                return Ok(ntune_err(
                    span,
                    "train_test_split() test_size count must be in (0, n)",
                ));
            }
            Some(_) => {
                return Ok(ntune_err(
                    span,
                    "train_test_split() test_size must be fraction in (0, 1) or positive int count",
                ));
            }
            None => 0.2,
        }
    } else {
        0.2
    };
    if !(test_size > 0.0 && test_size < 1.0) {
        return Ok(ntune_err(
            span,
            "train_test_split() test_size must be in (0, 1)",
        ));
    }
    let seed = int_field(opts.as_ref(), "seed", 0) as u64;
    match train_test_split_indices(n as usize, test_size, seed) {
        Ok(split) => {
            let mut map = HashMap::new();
            let train: Vec<ValueRef> = split
                .train
                .iter()
                .map(|&i| Value::Int(i as i64).ref_cell())
                .collect();
            let test: Vec<ValueRef> = split
                .test
                .iter()
                .map(|&i| Value::Int(i as i64).ref_cell())
                .collect();
            map.insert("train".into(), Value::Array(train).ref_cell());
            map.insert("test".into(), Value::Array(test).ref_cell());
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(tune_err_to_value(span, e)),
    }
}

// >>> len(ntune.kfold(9, {n_splits: 3}))
// => 3
fn ntune_kfold(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ntune_kfold", span)?;
    let n = int_arg(args, 0, "ntune_kfold", span)?;
    if n <= 0 {
        return Ok(ntune_err(span, "kfold() n must be > 0"));
    }
    let opts = optional_object_arg(args, 1);
    let n_splits = int_field(opts.as_ref(), "n_splits", 5);
    let shuffle = bool_field(opts.as_ref(), "shuffle", false);
    let seed = int_field(opts.as_ref(), "seed", 0) as u64;
    match kfold_indices(n as usize, n_splits as usize, shuffle, seed) {
        Ok(folds) => {
            let items: Vec<ValueRef> = folds
                .into_iter()
                .map(|f| {
                    let mut map = HashMap::new();
                    let train: Vec<ValueRef> = f
                        .train
                        .iter()
                        .map(|&i| Value::Int(i as i64).ref_cell())
                        .collect();
                    let test: Vec<ValueRef> = f
                        .test
                        .iter()
                        .map(|&i| Value::Int(i as i64).ref_cell())
                        .collect();
                    map.insert("train".into(), Value::Array(train).ref_cell());
                    map.insert("test".into(), Value::Array(test).ref_cell());
                    Value::Object(map).ref_cell()
                })
                .collect();
            Ok(Value::Array(items).ref_cell())
        }
        Err(e) => Ok(tune_err_to_value(span, e)),
    }
}

// >>> ntune.best([{value: 0.2}, {value: 0.1}]).value
fn ntune_best(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ntune_best", span)?;
    let trials_val = &*args[0].borrow();
    let opts = optional_object_arg(args, 1);
    let direction = match parse_direction(opts.as_ref()) {
        Ok(d) => d,
        Err(e) => return Ok(tune_err_to_value(span, e)),
    };
    let items = match trials_val {
        Value::Array(arr) => arr.clone(),
        other => {
            return Err(type_err(
                span,
                format!(
                    "ntune_best() expects trial array, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    let mut parsed = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let obj_map = match item.borrow().clone() {
            Value::Object(m) => m,
            other => {
                return Err(type_err(
                    span,
                    format!(
                        "ntune_best() trial[{i}] must be object, got {}",
                        other.type_name()
                    ),
                ));
            }
        };
        let value = match obj_map.get("value").map(|v| v.borrow().clone()) {
            Some(Value::Int(n)) => n as f64,
            Some(Value::Float(f)) => f,
            _ => {
                return Ok(ntune_err(
                    span,
                    format!("ntune_best() trial[{i}] missing numeric value"),
                ));
            }
        };
        let params = if let Some(p) = obj_map.get("params") {
            match &*p.borrow() {
                Value::Object(pm) => {
                    let mut out = BTreeMap::new();
                    for (k, v) in pm {
                        out.insert(k.clone(), value_to_param(&v.borrow(), span, "params")?);
                    }
                    out
                }
                _ => BTreeMap::new(),
            }
        } else {
            BTreeMap::new()
        };
        parsed.push(TrialRecord {
            trial: i,
            params,
            value,
            budget: obj_map.get("budget").and_then(|v| match &*v.borrow() {
                Value::Int(n) if *n >= 0 => Some(*n as u64),
                _ => None,
            }),
            status: niao_tune::TrialStatus::Complete,
        });
    }
    match best_trial(&parsed, direction) {
        Some(t) => Ok(trial_to_object(&t)),
        None => Ok(ntune_err(span, "best() requires at least one trial")),
    }
}

// >>> ntune.is_better(0.1, 0.2)
// => true
fn ntune_is_better(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntune_is_better", span)?;
    let a = float_arg(args, 0, "ntune_is_better", span)?;
    let b = float_arg(args, 1, "ntune_is_better", span)?;
    let opts = optional_object_arg(args, 2);
    let direction = match parse_direction(opts.as_ref()) {
        Ok(d) => d,
        Err(e) => return Ok(tune_err_to_value(span, e)),
    };
    Ok(Value::Bool(direction.is_better(a, b)).ref_cell())
}

fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
    vec![
        ("ntune_grid_size", "grid_size", Rc::new(ntune_grid_size)),
        (
            "ntune_grid_points",
            "grid_points",
            Rc::new(ntune_grid_points),
        ),
        ("ntune_sample", "sample", Rc::new(ntune_sample)),
        (
            "ntune_validate_space",
            "validate_space",
            Rc::new(ntune_validate_space),
        ),
        (
            "ntune_default_opts",
            "default_opts",
            Rc::new(ntune_default_opts),
        ),
        (
            "ntune_grid_search",
            "grid_search",
            Rc::new(ntune_grid_search),
        ),
        (
            "ntune_random_search",
            "random_search",
            Rc::new(ntune_random_search),
        ),
        ("ntune_halving", "halving", Rc::new(ntune_halving)),
        (
            "ntune_train_test_split",
            "train_test_split",
            Rc::new(ntune_train_test_split),
        ),
        ("ntune_kfold", "kfold", Rc::new(ntune_kfold)),
        ("ntune_best", "best", Rc::new(ntune_best)),
        ("ntune_is_better", "is_better", Rc::new(ntune_is_better)),
    ]
}

pub const MODULE_NAME: &str = "ntune";
pub const MODULE_PATHS: &[&str] = &["ntune", "std/ntune"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::dummy()
    }

    #[test]
    fn grid_size_builtin() {
        let mut space = HashMap::new();
        space.insert(
            "lr".into(),
            Value::Array(vec![
                Value::Float(0.01).ref_cell(),
                Value::Float(0.1).ref_cell(),
            ])
            .ref_cell(),
        );
        space.insert(
            "d".into(),
            Value::Array(vec![Value::Int(3).ref_cell(), Value::Int(5).ref_cell()]).ref_cell(),
        );
        let n = ntune_grid_size(&[Value::Object(space).ref_cell()], span()).unwrap();
        assert!(matches!(&*n.borrow(), Value::Int(4)));
    }

    #[test]
    fn train_test_split_builtin() {
        let r = ntune_train_test_split(&[Value::Int(20).ref_cell()], span()).unwrap();
        match r.borrow().clone() {
            Value::Object(m) => {
                let train = match &*m["train"].borrow() {
                    Value::Array(a) => a.len(),
                    _ => 0,
                };
                let test = match &*m["test"].borrow() {
                    Value::Array(a) => a.len(),
                    _ => 0,
                };
                assert_eq!(train + test, 20);
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn is_better_minimize() {
        let b = ntune_is_better(
            &[Value::Float(0.1).ref_cell(), Value::Float(0.2).ref_cell()],
            span(),
        )
        .unwrap();
        assert!(matches!(&*b.borrow(), Value::Bool(true)));
    }
}
