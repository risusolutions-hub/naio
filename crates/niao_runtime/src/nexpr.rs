//! Native nexpr standard library — safe sandboxed expression evaluator for user
//! formulas and config logic (~Python `simpleeval` / `asteval` subset).
//!
//! Import with `import "nexpr"` (or `import "std/nexpr"`).

use crate::{call_niao_function, error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_expr::{
    parse, valid, BinOpTag, Compiled, Evaluator, ExprError, ExternalFn, Value as ExprValue,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Handle stores
// ---------------------------------------------------------------------------

struct EvalEntry {
    ev: Evaluator,
    fns: HashMap<Arc<str>, ValueRef>,
}

thread_local! {
    static COMPILED: RefCell<HashMap<i64, Compiled>> = RefCell::new(HashMap::new());
    static EVALUATORS: RefCell<HashMap<i64, EvalEntry>> = RefCell::new(HashMap::new());
    static NEXT_COMPILED: RefCell<i64> = const { RefCell::new(1) };
    static NEXT_EVAL: RefCell<i64> = const { RefCell::new(1) };
}

fn alloc_compiled(c: Compiled) -> i64 {
    let id = NEXT_COMPILED.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    COMPILED.with(|m| m.borrow_mut().insert(id, c));
    id
}

fn alloc_evaluator(ev: Evaluator, fns: HashMap<Arc<str>, ValueRef>) -> i64 {
    let id = NEXT_EVAL.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    EVALUATORS.with(|m| m.borrow_mut().insert(id, EvalEntry { ev, fns }));
    id
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E4200_NEXPR_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E4200_NEXPR_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E4202_NEXPR_TYPE, msg.into())
}

fn nexpr_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E4201_NEXPR_ERROR, "nexpr_error", msg.into(), span)
}

fn parse_err(span: Span, err: ExprError) -> ValueRef {
    error_value(codes::E4203_NEXPR_PARSE, "nexpr_error", err.message(), span)
}

fn string_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn handle_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(id) if *id > 0 => Ok(*id),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a positive handle as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn to_expr(v: &Value, span: Span) -> NiaoResult<ExprValue> {
    match v {
        Value::Nil => Ok(ExprValue::Nil),
        Value::Bool(b) => Ok(ExprValue::Bool(*b)),
        Value::Int(n) => Ok(ExprValue::Int(*n)),
        Value::Float(f) => Ok(ExprValue::Float(*f)),
        Value::String(s) => Ok(ExprValue::String(Arc::from(s.as_str()))),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(to_expr(&*item.borrow(), span)?);
            }
            Ok(ExprValue::Array(out))
        }
        Value::Object(map) => {
            let mut out = HashMap::with_capacity(map.len());
            for (k, vr) in map {
                out.insert(Arc::from(k.as_str()), to_expr(&*vr.borrow(), span)?);
            }
            Ok(ExprValue::Object(out))
        }
        Value::BigInt(n) => {
            let s = n.to_string();
            if let Ok(i) = s.parse::<i64>() {
                Ok(ExprValue::Int(i))
            } else {
                Ok(ExprValue::String(Arc::from(s)))
            }
        }
        other => Err(type_err(
            span,
            format!(
                "expression values must be nil, bool, int, float, string, array, or object — got {}",
                other.type_name()
            ),
        )),
    }
}

fn from_expr(v: ExprValue) -> ValueRef {
    match v {
        ExprValue::Nil => Value::Nil.ref_cell(),
        ExprValue::Bool(b) => Value::Bool(b).ref_cell(),
        ExprValue::Int(n) => Value::Int(n).ref_cell(),
        ExprValue::Float(f) => Value::Float(f).ref_cell(),
        ExprValue::String(s) => Value::String(s.to_string()).ref_cell(),
        ExprValue::Array(items) => {
            let out: Vec<ValueRef> = items.into_iter().map(from_expr).collect();
            Value::Array(out).ref_cell()
        }
        ExprValue::Object(map) => {
            let mut out = HashMap::new();
            for (k, v) in map {
                out.insert(k.to_string(), from_expr(v));
            }
            Value::Object(out).ref_cell()
        }
    }
}

fn vars_from_object(obj: &HashMap<String, ValueRef>, span: Span) -> NiaoResult<HashMap<Arc<str>, ExprValue>> {
    let mut vars = HashMap::with_capacity(obj.len());
    for (k, vr) in obj {
        vars.insert(Arc::from(k.as_str()), to_expr(&*vr.borrow(), span)?);
    }
    Ok(vars)
}

fn optional_vars(args: &[ValueRef], idx: usize, span: Span) -> NiaoResult<HashMap<Arc<str>, ExprValue>> {
    if args.len() <= idx {
        return Ok(HashMap::new());
    }
    match &*args[idx].borrow() {
        Value::Object(map) => vars_from_object(map, span),
        Value::Nil => Ok(HashMap::new()),
        other => Err(type_err(
            span,
            format!(
                "expected object of variables, got {}",
                other.type_name()
            ),
        )),
    }
}

fn wrap_external_fn(callee: ValueRef, span: Span) -> ExternalFn {
    Arc::new(move |args: &[ExprValue]| {
        let niao_args: Vec<ValueRef> = args.iter().map(|v| from_expr(v.clone())).collect();
        match call_niao_function(Rc::clone(&callee), &niao_args, span) {
            Ok(vr) => to_expr(&*vr.borrow(), span)
                .map_err(|e| ExprError::Eval { message: e.to_string() }),
            Err(e) => Err(ExprError::Eval {
                message: e.to_string(),
            }),
        }
    })
}

fn op_from_name(name: &str) -> Option<BinOpTag> {
    match name {
        "+" => Some(BinOpTag::Add),
        "-" => Some(BinOpTag::Sub),
        "*" => Some(BinOpTag::Mul),
        "/" => Some(BinOpTag::Div),
        "//" => Some(BinOpTag::FloorDiv),
        "%" => Some(BinOpTag::Mod),
        "**" => Some(BinOpTag::Pow),
        "==" => Some(BinOpTag::Eq),
        "!=" => Some(BinOpTag::NotEq),
        "<" => Some(BinOpTag::Lt),
        ">" => Some(BinOpTag::Gt),
        "<=" => Some(BinOpTag::Le),
        ">=" => Some(BinOpTag::Ge),
        "and" => Some(BinOpTag::And),
        "or" => Some(BinOpTag::Or),
        "in" => Some(BinOpTag::In),
        _ => None,
    }
}

fn load_fns_into(ev: &mut Evaluator, fns: &HashMap<Arc<str>, ValueRef>, span: Span) {
    for (name, callee) in fns {
        let cb = wrap_external_fn(Rc::clone(callee), span);
        ev.set_fn(name, cb);
    }
}

fn run_eval(
    ev: &mut Evaluator,
    fns: &HashMap<Arc<str>, ValueRef>,
    source: &str,
    span: Span,
) -> NiaoResult<ValueRef> {
    load_fns_into(ev, fns, span);
    match ev.eval(source) {
        Ok(v) => Ok(from_expr(v)),
        Err(e) => Ok(nexpr_err(span, e.message())),
    }
}

fn run_compiled(
    ev: &mut Evaluator,
    fns: &HashMap<Arc<str>, ValueRef>,
    compiled: &Compiled,
    span: Span,
) -> NiaoResult<ValueRef> {
    load_fns_into(ev, fns, span);
    match ev.run(compiled) {
        Ok(v) => Ok(from_expr(v)),
        Err(e) => Ok(nexpr_err(span, e.message())),
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// nexpr.eval(expr, vars?) — evaluate an expression string in one shot.
///
// >>> import "nexpr"
// >>> nexpr.eval("2 + 3 * 4")
// => 14
// >>> nexpr.eval("x * y", { "x": 10, "y": 3 })
// => 30
fn nexpr_eval(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nexpr_eval", span)?;
    let source = string_arg(args, 0, "nexpr_eval", span)?;
    let vars = optional_vars(args, 1, span)?;
    let mut ev = Evaluator::new();
    ev.vars.extend(vars);
    run_eval(&mut ev, &HashMap::new(), &source, span)
}

/// nexpr.valid(expr) — true when the expression parses successfully.
///
// >>> import "nexpr"
// >>> nexpr.valid("1 + 2")
// => true
// >>> nexpr.valid("1 +")
// => false
fn nexpr_valid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nexpr_valid", span)?;
    let source = string_arg(args, 0, "nexpr_valid", span)?;
    Ok(Value::Bool(valid(&source)).ref_cell())
}

/// nexpr.compile(expr) — parse once; returns an opaque compiled handle.
///
// >>> import "nexpr"
// >>> let c = nexpr.compile("a + b")
// >>> type(c)
// => "int"
fn nexpr_compile(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nexpr_compile", span)?;
    let source = string_arg(args, 0, "nexpr_compile", span)?;
    match parse(&source) {
        Ok(c) => Ok(Value::Int(alloc_compiled(c)).ref_cell()),
        Err(e) => Ok(parse_err(span, e)),
    }
}

/// nexpr.run(compiled, vars?) — execute a compiled expression.
///
// >>> import "nexpr"
// >>> let c = nexpr.compile("x + 1")
// >>> nexpr.run(c, { "x": 41 })
// => 42
fn nexpr_run(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nexpr_run", span)?;
    let id = handle_arg(args, 0, "nexpr_run", span)?;
    let vars = optional_vars(args, 1, span)?;
    COMPILED.with(|m| {
        let compiled = m.borrow().get(&id).cloned().ok_or_else(|| {
            RuntimeError::at(
                span,
                codes::E4204_NEXPR_HANDLE,
                format!("invalid compiled handle {id}"),
            )
        })?;
        let mut ev = Evaluator::new();
        ev.vars.extend(vars);
        run_compiled(&mut ev, &HashMap::new(), &compiled, span)
    })
}

/// nexpr.evaluator(vars?, fns?) — create a reusable evaluator handle.
///
// >>> import "nexpr"
// >>> let ev = nexpr.evaluator({ "x": 5 })
// >>> nexpr.execute(ev, "x * 2")
// => 10
fn nexpr_evaluator(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 2, "nexpr_evaluator", span)?;
    let vars = optional_vars(args, 0, span)?;
    let mut fns_map = HashMap::new();
    if args.len() >= 2 {
        match &*args[1].borrow() {
            Value::Object(map) => {
                for (k, vr) in map {
                    fns_map.insert(Arc::from(k.as_str()), Rc::clone(vr));
                }
            }
            Value::Nil => {}
            other => {
                return Err(type_err(
                    span,
                    format!("fns must be an object of callables, got {}", other.type_name()),
                ));
            }
        }
    }
    let mut ev = Evaluator::new();
    ev.vars.extend(vars);
    Ok(Value::Int(alloc_evaluator(ev, fns_map)).ref_cell())
}

/// nexpr.set(ev, name, value) — bind a variable on an evaluator.
///
// >>> import "nexpr"
// >>> let ev = nexpr.evaluator()
// >>> nexpr.set(ev, "pi", 3.14)
// >>> nexpr.execute(ev, "pi * 2")
// => 6.28
fn nexpr_set(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nexpr_set", span)?;
    let id = handle_arg(args, 0, "nexpr_set", span)?;
    let name = string_arg(args, 1, "nexpr_set", span)?;
    let value = to_expr(&*args[2].borrow(), span)?;
    EVALUATORS.with(|m| {
        let mut m = m.borrow_mut();
        let entry = m.get_mut(&id).ok_or_else(|| {
            RuntimeError::at(
                span,
                codes::E4204_NEXPR_HANDLE,
                format!("invalid evaluator handle {id}"),
            )
        })?;
        entry.ev.set_var(&name, value);
        Ok(Value::Nil.ref_cell())
    })
}

/// nexpr.set_fn(ev, name, fn) — register a custom callable.
fn nexpr_set_fn(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nexpr_set_fn", span)?;
    let id = handle_arg(args, 0, "nexpr_set_fn", span)?;
    let name = string_arg(args, 1, "nexpr_set_fn", span)?;
    EVALUATORS.with(|m| {
        let mut m = m.borrow_mut();
        let entry = m.get_mut(&id).ok_or_else(|| {
            RuntimeError::at(
                span,
                codes::E4204_NEXPR_HANDLE,
                format!("invalid evaluator handle {id}"),
            )
        })?;
        entry.fns.insert(Arc::from(name.as_str()), Rc::clone(&args[2]));
        Ok(Value::Nil.ref_cell())
    })
}

/// nexpr.get(ev, name) — read a bound variable.
fn nexpr_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nexpr_get", span)?;
    let id = handle_arg(args, 0, "nexpr_get", span)?;
    let name = string_arg(args, 1, "nexpr_get", span)?;
    EVALUATORS.with(|m| {
        let m = m.borrow();
        let entry = m.get(&id).ok_or_else(|| {
            RuntimeError::at(
                span,
                codes::E4204_NEXPR_HANDLE,
                format!("invalid evaluator handle {id}"),
            )
        })?;
        match entry.ev.get_var(&name) {
            Some(v) => Ok(from_expr(v.clone())),
            None => Ok(nexpr_err(
                span,
                format!("undefined variable '{name}'"),
            )),
        }
    })
}

/// nexpr.clear(ev) — remove all variables from an evaluator.
fn nexpr_clear(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nexpr_clear", span)?;
    let id = handle_arg(args, 0, "nexpr_clear", span)?;
    EVALUATORS.with(|m| {
        let mut m = m.borrow_mut();
        let entry = m.get_mut(&id).ok_or_else(|| {
            RuntimeError::at(
                span,
                codes::E4204_NEXPR_HANDLE,
                format!("invalid evaluator handle {id}"),
            )
        })?;
        entry.ev.clear_vars();
        Ok(Value::Nil.ref_cell())
    })
}

/// nexpr.clear_fns(ev) — remove custom functions from an evaluator.
fn nexpr_clear_fns(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nexpr_clear_fns", span)?;
    let id = handle_arg(args, 0, "nexpr_clear_fns", span)?;
    EVALUATORS.with(|m| {
        let mut m = m.borrow_mut();
        let entry = m.get_mut(&id).ok_or_else(|| {
            RuntimeError::at(
                span,
                codes::E4204_NEXPR_HANDLE,
                format!("invalid evaluator handle {id}"),
            )
        })?;
        entry.ev.clear_fns();
        entry.fns.clear();
        Ok(Value::Nil.ref_cell())
    })
}

/// nexpr.execute(ev, expr) — parse and evaluate on a persistent evaluator.
fn nexpr_execute(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nexpr_execute", span)?;
    let id = handle_arg(args, 0, "nexpr_execute", span)?;
    let source = string_arg(args, 1, "nexpr_execute", span)?;
    EVALUATORS.with(|m| {
        let mut m = m.borrow_mut();
        let entry = m.get_mut(&id).ok_or_else(|| {
            RuntimeError::at(
                span,
                codes::E4204_NEXPR_HANDLE,
                format!("invalid evaluator handle {id}"),
            )
        })?;
        let fns = entry.fns.clone();
        run_eval(&mut entry.ev, &fns, &source, span)
    })
}

/// nexpr.execute_compiled(ev, compiled) — run a compiled handle on an evaluator.
fn nexpr_execute_compiled(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nexpr_execute_compiled", span)?;
    let ev_id = handle_arg(args, 0, "nexpr_execute_compiled", span)?;
    let c_id = handle_arg(args, 1, "nexpr_execute_compiled", span)?;
    let compiled = COMPILED.with(|m| {
        m.borrow().get(&c_id).cloned().ok_or_else(|| {
            RuntimeError::at(
                span,
                codes::E4204_NEXPR_HANDLE,
                format!("invalid compiled handle {c_id}"),
            )
        })
    })?;
    EVALUATORS.with(|m| {
        let mut m = m.borrow_mut();
        let entry = m.get_mut(&ev_id).ok_or_else(|| {
            RuntimeError::at(
                span,
                codes::E4204_NEXPR_HANDLE,
                format!("invalid evaluator handle {ev_id}"),
            )
        })?;
        let fns = entry.fns.clone();
        run_compiled(&mut entry.ev, &fns, &compiled, span)
    })
}

/// nexpr.batch(ev, compiled, rows, threads?) — parallel evaluate per-row variable maps.
fn nexpr_batch(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nexpr_batch", span)?;
    let ev_id = handle_arg(args, 0, "nexpr_batch", span)?;
    let c_id = handle_arg(args, 1, "nexpr_batch", span)?;
    let threads = if args.len() >= 4 {
        match &*args[3].borrow() {
            Value::Int(n) if *n > 0 => *n as usize,
            Value::Nil => 0,
            other => {
                return Err(type_err(
                    span,
                    format!("threads must be a positive int or nil, got {}", other.type_name()),
                ));
            }
        }
    } else {
        0
    };
    let rows_val = Rc::clone(&args[2]);
    let compiled = COMPILED.with(|m| {
        m.borrow().get(&c_id).cloned().ok_or_else(|| {
            RuntimeError::at(
                span,
                codes::E4204_NEXPR_HANDLE,
                format!("invalid compiled handle {c_id}"),
            )
        })
    })?;
    let (ev_template, fns) = EVALUATORS.with(|m| {
        let m = m.borrow();
        let entry = m.get(&ev_id).ok_or_else(|| {
            RuntimeError::at(
                span,
                codes::E4204_NEXPR_HANDLE,
                format!("invalid evaluator handle {ev_id}"),
            )
        })?;
        Ok((entry.ev.clone(), entry.fns.clone()))
    })?;
    let row_maps: Vec<HashMap<Arc<str>, ExprValue>> = match &*rows_val.borrow() {
        Value::Array(rows) => {
            let mut out = Vec::with_capacity(rows.len());
            for row in rows {
                match &*row.borrow() {
                    Value::Object(map) => out.push(vars_from_object(map, span)?),
                    other => {
                        return Err(type_err(
                            span,
                            format!("batch rows must be objects, got {}", other.type_name()),
                        ));
                    }
                }
            }
            out
        }
        other => {
            return Err(type_err(
                span,
                format!("rows must be an array of objects, got {}", other.type_name()),
            ));
        }
    };
    let mut ev = ev_template;
    load_fns_into(&mut ev, &fns, span);
    let results = ev.batch(&compiled, &row_maps, threads);
    let out: Vec<ValueRef> = results
        .into_iter()
        .map(|r| match r {
            Ok(v) => from_expr(v),
            Err(e) => nexpr_err(span, e.message()),
        })
        .collect();
    Ok(Value::Array(out).ref_cell())
}

/// nexpr.names(ev) — list bound variable names.
fn nexpr_names(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nexpr_names", span)?;
    let id = handle_arg(args, 0, "nexpr_names", span)?;
    EVALUATORS.with(|m| {
        let m = m.borrow();
        let entry = m.get(&id).ok_or_else(|| {
            RuntimeError::at(
                span,
                codes::E4204_NEXPR_HANDLE,
                format!("invalid evaluator handle {id}"),
            )
        })?;
        let names: Vec<ValueRef> = entry
            .ev
            .var_names()
            .into_iter()
            .map(|s| Value::String(s).ref_cell())
            .collect();
        Ok(Value::Array(names).ref_cell())
    })
}

/// nexpr.free_compiled(handle) — release a compiled expression handle.
fn nexpr_free_compiled(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nexpr_free_compiled", span)?;
    let id = handle_arg(args, 0, "nexpr_free_compiled", span)?;
    COMPILED.with(|m| {
        if m.borrow_mut().remove(&id).is_none() {
            return Err(RuntimeError::at(
                span,
                codes::E4204_NEXPR_HANDLE,
                format!("invalid compiled handle {id}"),
            ));
        }
        Ok(Value::Nil.ref_cell())
    })
}

/// nexpr.free(ev) — release an evaluator handle.
fn nexpr_free(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nexpr_free", span)?;
    let id = handle_arg(args, 0, "nexpr_free", span)?;
    EVALUATORS.with(|m| {
        if m.borrow_mut().remove(&id).is_none() {
            return Err(RuntimeError::at(
                span,
                codes::E4204_NEXPR_HANDLE,
                format!("invalid evaluator handle {id}"),
            ));
        }
        Ok(Value::Nil.ref_cell())
    })
}

/// nexpr.allow_op(ev, op, allowed?) — enable/disable an operator in the sandbox.
fn nexpr_allow_op(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nexpr_allow_op", span)?;
    let id = handle_arg(args, 0, "nexpr_allow_op", span)?;
    let op_name = string_arg(args, 1, "nexpr_allow_op", span)?;
    let tag = op_from_name(&op_name).ok_or_else(|| {
        type_err(span, format!("unknown operator '{op_name}'"))
    })?;
    let allowed = if args.len() >= 3 {
        match &*args[2].borrow() {
            Value::Bool(b) => *b,
            other => {
                return Err(type_err(
                    span,
                    format!("allowed must be bool, got {}", other.type_name()),
                ));
            }
        }
    } else {
        true
    };
    EVALUATORS.with(|m| {
        let mut m = m.borrow_mut();
        let entry = m.get_mut(&id).ok_or_else(|| {
            RuntimeError::at(
                span,
                codes::E4204_NEXPR_HANDLE,
                format!("invalid evaluator handle {id}"),
            )
        })?;
        entry.ev.allow_op(tag, allowed);
        Ok(Value::Nil.ref_cell())
    })
}

/// nexpr.allow_fn(ev, name, allowed?) — enable/disable a function name.
fn nexpr_allow_fn(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nexpr_allow_fn", span)?;
    let id = handle_arg(args, 0, "nexpr_allow_fn", span)?;
    let name = string_arg(args, 1, "nexpr_allow_fn", span)?;
    let allowed = if args.len() >= 3 {
        match &*args[2].borrow() {
            Value::Bool(b) => *b,
            other => {
                return Err(type_err(
                    span,
                    format!("allowed must be bool, got {}", other.type_name()),
                ));
            }
        }
    } else {
        true
    };
    EVALUATORS.with(|m| {
        let mut m = m.borrow_mut();
        let entry = m.get_mut(&id).ok_or_else(|| {
            RuntimeError::at(
                span,
                codes::E4204_NEXPR_HANDLE,
                format!("invalid evaluator handle {id}"),
            )
        })?;
        entry.ev.allow_fn(&name, allowed);
        Ok(Value::Nil.ref_cell())
    })
}

/// nexpr.functions() — list default builtin function names.
fn nexpr_functions(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    let names: Vec<ValueRef> = niao_expr::default_functions()
        .iter()
        .map(|s| Value::String((*s).to_string()).ref_cell())
        .collect();
    Ok(Value::Array(names).ref_cell())
}

/// nexpr.operators() — list supported operator tokens.
fn nexpr_operators(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    let names: Vec<ValueRef> = niao_expr::default_operators()
        .iter()
        .map(|s| Value::String((*s).to_string()).ref_cell())
        .collect();
    Ok(Value::Array(names).ref_cell())
}

/// nexpr.referenced(compiled) — variable names referenced by a compiled expression.
fn nexpr_referenced(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nexpr_referenced", span)?;
    let id = handle_arg(args, 0, "nexpr_referenced", span)?;
    COMPILED.with(|m| {
        let guard = m.borrow();
        let compiled = guard.get(&id).ok_or_else(|| {
            RuntimeError::at(
                span,
                codes::E4204_NEXPR_HANDLE,
                format!("invalid compiled handle {id}"),
            )
        })?;
        let names: Vec<ValueRef> = compiled
            .names
            .iter()
            .map(|n| Value::String(n.to_string()).ref_cell())
            .collect();
        Ok(Value::Array(names).ref_cell())
    })
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nexpr_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nexpr_fns![
    ("nexpr_eval", "eval", nexpr_eval),
    ("nexpr_valid", "valid", nexpr_valid),
    ("nexpr_compile", "compile", nexpr_compile),
    ("nexpr_run", "run", nexpr_run),
    ("nexpr_evaluator", "evaluator", nexpr_evaluator),
    ("nexpr_set", "set", nexpr_set),
    ("nexpr_set_fn", "set_fn", nexpr_set_fn),
    ("nexpr_get", "get", nexpr_get),
    ("nexpr_clear", "clear", nexpr_clear),
    ("nexpr_clear_fns", "clear_fns", nexpr_clear_fns),
    ("nexpr_execute", "execute", nexpr_execute),
    ("nexpr_execute_compiled", "execute_compiled", nexpr_execute_compiled),
    ("nexpr_batch", "batch", nexpr_batch),
    ("nexpr_names", "names", nexpr_names),
    ("nexpr_free_compiled", "free_compiled", nexpr_free_compiled),
    ("nexpr_free", "free", nexpr_free),
    ("nexpr_allow_op", "allow_op", nexpr_allow_op),
    ("nexpr_allow_fn", "allow_fn", nexpr_allow_fn),
    ("nexpr_functions", "functions", nexpr_functions),
    ("nexpr_operators", "operators", nexpr_operators),
    ("nexpr_referenced", "referenced", nexpr_referenced),
];

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nexpr";
pub const MODULE_PATHS: &[&str] = &["nexpr", "std/nexpr"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    #[test]
    fn eval_simple() {
        let out = nexpr_eval(&[Value::String("1+2".into()).ref_cell()], span()).unwrap();
        assert_eq!(*out.borrow(), Value::Int(3));
    }
}
