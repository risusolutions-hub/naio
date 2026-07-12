//! Native nlazy standard library — fused lazy pipelines over packed arrays.
//! Built-in map/filter/take stages compose without materializing until
//! `collect` or `sum`.
//!
//! Import with `import "nlazy"` (or `import "std/nlazy"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const E3410_NLAZY_ARITY: u32 = 3410;
const E3411_NLAZY_ERROR: u32 = 3411;
const E3412_NLAZY_TYPE: u32 = 3412;
const E3413_NLAZY_INVALID_HANDLE: u32 = 3413;

// ---------------------------------------------------------------------------
// Pipeline model
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum ArrayKind {
    Int(Vec<i64>),
    Float(Vec<f64>),
}

#[derive(Clone, Debug)]
enum Step {
    Map(String),
    Filter(String),
    Take(usize),
}

#[derive(Clone, Debug)]
struct LazyPipe {
    source: Option<ArrayKind>,
    steps: Vec<Step>,
}

impl LazyPipe {
    fn new() -> Self {
        LazyPipe {
            source: None,
            steps: Vec::new(),
        }
    }
}

thread_local! {
    static PIPES: RefCell<HashMap<i64, LazyPipe>> = RefCell::new(HashMap::new());
    static NEXT_HANDLE: RefCell<i64> = const { RefCell::new(1) };
}

fn alloc_handle(pipe: LazyPipe) -> i64 {
    let id = NEXT_HANDLE.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    PIPES.with(|pipes| {
        pipes.borrow_mut().insert(id, pipe);
    });
    id
}

fn with_pipe<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut LazyPipe) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    PIPES.with(|pipes| {
        let mut pipes = pipes.borrow_mut();
        match pipes.get_mut(&id) {
            Some(p) => Ok(Ok(f(p))),
            None => Ok(Err(error_value(
                E3413_NLAZY_INVALID_HANDLE,
                "nlazy_error",
                format!("invalid or closed lazy pipeline handle {id}"),
                span,
            ))),
        }
    })
}

fn with_pipe_result<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut LazyPipe) -> Result<T, ValueRef>,
) -> NiaoResult<Result<T, ValueRef>> {
    PIPES.with(|pipes| {
        let mut pipes = pipes.borrow_mut();
        match pipes.get_mut(&id) {
            Some(p) => Ok(f(p)),
            None => Ok(Err(error_value(
                E3413_NLAZY_INVALID_HANDLE,
                "nlazy_error",
                format!("invalid or closed lazy pipeline handle {id}"),
                span,
            ))),
        }
    })
}

fn require_source(p: &LazyPipe, span: Span) -> Result<(), ValueRef> {
    if p.source.is_none() {
        Err(lazy_err(span, "nlazy pipeline has no source; call from() first"))
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3410_NLAZY_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3412_NLAZY_TYPE, msg.into())
}

fn lazy_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3411_NLAZY_ERROR, "nlazy_error", msg.into(), span)
}

fn handle_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a pipeline handle (int) as argument {}, got {}",
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

fn source_from_value(v: &Value, span: Span) -> Result<ArrayKind, ValueRef> {
    match v {
        Value::IntArray(items) => Ok(ArrayKind::Int(items.clone())),
        Value::FloatArray(items) => Ok(ArrayKind::Float(items.clone())),
        other => Err(lazy_err(
            span,
            format!(
                "nlazy.from() expects IntArray or FloatArray, got {}",
                other.type_name()
            ),
        )),
    }
}

fn is_map_op(kind: &ArrayKind, op: &str) -> bool {
    match kind {
        ArrayKind::Int(_) => matches!(op, "id" | "neg" | "abs" | "double"),
        ArrayKind::Float(_) => matches!(op, "id" | "neg" | "abs" | "square" | "sqrt"),
    }
}

fn is_filter_pred(kind: &ArrayKind, pred: &str) -> bool {
    match kind {
        ArrayKind::Int(_) => {
            matches!(pred, "positive" | "negative" | "nonzero" | "even" | "odd")
        }
        ArrayKind::Float(_) => matches!(pred, "positive" | "negative" | "nonzero"),
    }
}

fn map_int(op: &str, x: i64) -> i64 {
    match op {
        "id" => x,
        "neg" => x.wrapping_neg(),
        "abs" => x.saturating_abs(),
        "double" => x.wrapping_mul(2),
        _ => x,
    }
}

fn map_float(op: &str, x: f64) -> f64 {
    match op {
        "id" => x,
        "neg" => -x,
        "abs" => x.abs(),
        "square" => x * x,
        "sqrt" => x.sqrt(),
        _ => x,
    }
}

fn filter_int(pred: &str, x: i64) -> bool {
    match pred {
        "positive" => x > 0,
        "negative" => x < 0,
        "nonzero" => x != 0,
        "even" => x % 2 == 0,
        "odd" => x % 2 != 0,
        _ => true,
    }
}

fn filter_float(pred: &str, x: f64) -> bool {
    match pred {
        "positive" => x > 0.0,
        "negative" => x < 0.0,
        "nonzero" => x != 0.0,
        _ => true,
    }
}

fn run_fused(source: &ArrayKind, steps: &[Step]) -> Result<ArrayKind, ValueRef> {
    match source {
        ArrayKind::Int(data) => {
            let mut iter: Box<dyn Iterator<Item = i64>> = Box::new(data.iter().copied());
            for step in steps {
                match step {
                    Step::Map(op) => {
                        let op = op.clone();
                        iter = Box::new(iter.map(move |x| map_int(&op, x)));
                    }
                    Step::Filter(pred) => {
                        let pred = pred.clone();
                        iter = Box::new(iter.filter(move |x| filter_int(&pred, *x)));
                    }
                    Step::Take(n) => {
                        let n = *n;
                        iter = Box::new(iter.take(n));
                    }
                }
            }
            Ok(ArrayKind::Int(iter.collect()))
        }
        ArrayKind::Float(data) => {
            let mut iter: Box<dyn Iterator<Item = f64>> = Box::new(data.iter().copied());
            for step in steps {
                match step {
                    Step::Map(op) => {
                        let op = op.clone();
                        iter = Box::new(iter.map(move |x| map_float(&op, x)));
                    }
                    Step::Filter(pred) => {
                        let pred = pred.clone();
                        iter = Box::new(iter.filter(move |x| filter_float(&pred, *x)));
                    }
                    Step::Take(n) => {
                        let n = *n;
                        iter = Box::new(iter.take(n));
                    }
                }
            }
            Ok(ArrayKind::Float(iter.collect()))
        }
    }
}

fn run_fused_sum(source: &ArrayKind, steps: &[Step]) -> Result<ValueRef, ValueRef> {
    match source {
        ArrayKind::Int(data) => {
            let mut iter: Box<dyn Iterator<Item = i64>> = Box::new(data.iter().copied());
            for step in steps {
                match step {
                    Step::Map(op) => {
                        let op = op.clone();
                        iter = Box::new(iter.map(move |x| map_int(&op, x)));
                    }
                    Step::Filter(pred) => {
                        let pred = pred.clone();
                        iter = Box::new(iter.filter(move |x| filter_int(&pred, *x)));
                    }
                    Step::Take(n) => {
                        let n = *n;
                        iter = Box::new(iter.take(n));
                    }
                }
            }
            let acc: i128 = iter.map(|x| x as i128).sum();
            Ok(Value::Int(acc as i64).ref_cell())
        }
        ArrayKind::Float(data) => {
            let mut iter: Box<dyn Iterator<Item = f64>> = Box::new(data.iter().copied());
            for step in steps {
                match step {
                    Step::Map(op) => {
                        let op = op.clone();
                        iter = Box::new(iter.map(move |x| map_float(&op, x)));
                    }
                    Step::Filter(pred) => {
                        let pred = pred.clone();
                        iter = Box::new(iter.filter(move |x| filter_float(&pred, *x)));
                    }
                    Step::Take(n) => {
                        let n = *n;
                        iter = Box::new(iter.take(n));
                    }
                }
            }
            let acc: f64 = iter.sum();
            Ok(Value::Float(acc).ref_cell())
        }
    }
}

fn run_fused_len(source: &ArrayKind, steps: &[Step]) -> Result<i64, ValueRef> {
    match run_fused(source, steps)? {
        ArrayKind::Int(v) => Ok(v.len() as i64),
        ArrayKind::Float(v) => Ok(v.len() as i64),
    }
}

fn describe_steps(steps: &[Step]) -> String {
    if steps.is_empty() {
        "nlazy[]".to_string()
    } else {
        let parts: Vec<String> = steps
            .iter()
            .map(|s| match s {
                Step::Map(op) => format!("map({op})"),
                Step::Filter(pred) => format!("filter({pred})"),
                Step::Take(n) => format!("take({n})"),
            })
            .collect();
        format!("nlazy[{}]", parts.join(" → "))
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn nlazy_from(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nlazy_from", span)?;
    let mut pipe = LazyPipe::new();
    match source_from_value(&args[0].borrow(), span) {
        Ok(kind) => pipe.source = Some(kind),
        Err(e) => return Ok(e),
    }
    let id = alloc_handle(pipe);
    Ok(Value::Int(id).ref_cell())
}

fn nlazy_map(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nlazy_map", span)?;
    let id = handle_arg(args, 0, "nlazy_map", span)?;
    let op = string_arg(args, 1, "nlazy_map", span)?;
    match with_pipe_result(id, span, |p| {
        require_source(p, span)?;
        let kind = p.source.as_ref().unwrap();
        if !is_map_op(kind, &op) {
            return Err(lazy_err(
                span,
                format!("unknown or incompatible map op '{op}' for this pipeline kind"),
            ));
        }
        p.steps.push(Step::Map(op));
        Ok(())
    })? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nlazy_filter(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nlazy_filter", span)?;
    let id = handle_arg(args, 0, "nlazy_filter", span)?;
    let pred = string_arg(args, 1, "nlazy_filter", span)?;
    match with_pipe_result(id, span, |p| {
        require_source(p, span)?;
        let kind = p.source.as_ref().unwrap();
        if !is_filter_pred(kind, &pred) {
            return Err(lazy_err(
                span,
                format!("unknown or incompatible filter '{pred}' for this pipeline kind"),
            ));
        }
        p.steps.push(Step::Filter(pred));
        Ok(())
    })? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nlazy_take(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nlazy_take", span)?;
    let id = handle_arg(args, 0, "nlazy_take", span)?;
    let n = int_arg(args, 1, "nlazy_take", span)?;
    if n < 0 {
        return Ok(lazy_err(
            span,
            "nlazy_take() expects a non-negative count",
        ));
    }
    match with_pipe_result(id, span, |p| {
        require_source(p, span)?;
        p.steps.push(Step::Take(n as usize));
        Ok(())
    })? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nlazy_collect(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nlazy_collect", span)?;
    let id = handle_arg(args, 0, "nlazy_collect", span)?;
    match with_pipe_result(id, span, |p| {
        require_source(p, span)?;
        Ok((p.source.clone().unwrap(), p.steps.clone()))
    })? {
        Ok((source, steps)) => match run_fused(&source, &steps) {
            Ok(ArrayKind::Int(v)) => Ok(Value::IntArray(v).ref_cell()),
            Ok(ArrayKind::Float(v)) => Ok(Value::FloatArray(v).ref_cell()),
            Err(e) => Ok(e),
        },
        Err(e) => Ok(e),
    }
}

fn nlazy_sum(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nlazy_sum", span)?;
    let id = handle_arg(args, 0, "nlazy_sum", span)?;
    match with_pipe_result(id, span, |p| {
        require_source(p, span)?;
        Ok((p.source.clone().unwrap(), p.steps.clone()))
    })? {
        Ok((source, steps)) => match run_fused_sum(&source, &steps) {
            Ok(v) => Ok(v),
            Err(e) => Ok(e),
        },
        Err(e) => Ok(e),
    }
}

fn nlazy_len(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nlazy_len", span)?;
    let id = handle_arg(args, 0, "nlazy_len", span)?;
    match with_pipe_result(id, span, |p| {
        require_source(p, span)?;
        Ok((p.source.clone().unwrap(), p.steps.clone()))
    })? {
        Ok((source, steps)) => match run_fused_len(&source, &steps) {
            Ok(n) => Ok(Value::Int(n).ref_cell()),
            Err(e) => Ok(e),
        },
        Err(e) => Ok(e),
    }
}

fn nlazy_describe(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nlazy_describe", span)?;
    let id = handle_arg(args, 0, "nlazy_describe", span)?;
    match with_pipe(id, span, |p| describe_steps(&p.steps))? {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nlazy_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nlazy_close", span)?;
    let id = handle_arg(args, 0, "nlazy_close", span)?;
    let removed = PIPES.with(|pipes| pipes.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nlazy_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nlazy_fns![
    ("nlazy_from", "from", nlazy_from),
    ("nlazy_map", "map", nlazy_map),
    ("nlazy_filter", "filter", nlazy_filter),
    ("nlazy_take", "take", nlazy_take),
    ("nlazy_collect", "collect", nlazy_collect),
    ("nlazy_sum", "sum", nlazy_sum),
    ("nlazy_len", "len", nlazy_len),
    ("nlazy_describe", "describe", nlazy_describe),
    ("nlazy_close", "close", nlazy_close),
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

pub const MODULE_NAME: &str = "nlazy";
pub const MODULE_PATHS: &[&str] = &["nlazy", "std/nlazy"];

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

    fn ia(v: Vec<i64>) -> ValueRef {
        Value::IntArray(v).ref_cell()
    }

    fn s(v: &str) -> ValueRef {
        Value::String(v.to_string()).ref_cell()
    }

    fn handle(r: NiaoResult<ValueRef>) -> i64 {
        match &*r.unwrap().borrow() {
            Value::Int(n) => *n,
            other => panic!("expected handle, got {other:?}"),
        }
    }

    fn expect_int(r: NiaoResult<ValueRef>) -> i64 {
        match &*r.unwrap().borrow() {
            Value::Int(n) => *n,
            other => panic!("expected int, got {other:?}"),
        }
    }

    #[test]
    fn fused_map_filter_take_sum() {
        let h = handle(nlazy_from(&[ia((1..=20).collect())], span()));
        nlazy_filter(&[Value::Int(h).ref_cell(), s("even")], span()).unwrap();
        nlazy_map(&[Value::Int(h).ref_cell(), s("double")], span()).unwrap();
        nlazy_take(&[Value::Int(h).ref_cell(), Value::Int(3).ref_cell()], span()).unwrap();
        // evens 2..20 → double → take 3: 4, 8, 12 → sum 24
        assert_eq!(expect_int(nlazy_sum(&[Value::Int(h).ref_cell()], span())), 24);
        let collected = nlazy_collect(&[Value::Int(h).ref_cell()], span()).unwrap();
        match &*collected.borrow() {
            Value::IntArray(v) => assert_eq!(v, &vec![4, 8, 12]),
            other => panic!("{other:?}"),
        }
        assert_eq!(
            expect_int(nlazy_len(&[Value::Int(h).ref_cell()], span())),
            3
        );
        nlazy_close(&[Value::Int(h).ref_cell()], span()).unwrap();
    }

    #[test]
    fn describe_pipeline() {
        let h = handle(nlazy_from(&[ia(vec![1, 2, 3])], span()));
        nlazy_map(&[Value::Int(h).ref_cell(), s("abs")], span()).unwrap();
        nlazy_take(&[Value::Int(h).ref_cell(), Value::Int(2).ref_cell()], span()).unwrap();
        match &*nlazy_describe(&[Value::Int(h).ref_cell()], span())
            .unwrap()
            .borrow()
        {
            Value::String(desc) => {
                assert!(desc.contains("map(abs)"));
                assert!(desc.contains("take(2)"));
            }
            other => panic!("{other:?}"),
        }
        nlazy_close(&[Value::Int(h).ref_cell()], span()).unwrap();
    }

    #[test]
    fn invalid_handle() {
        let v = nlazy_collect(&[Value::Int(999_999).ref_cell()], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn float_pipeline() {
        let data = Value::FloatArray(vec![1.0, 4.0, 9.0]).ref_cell();
        let h = handle(nlazy_from(&[data], span()));
        nlazy_map(&[Value::Int(h).ref_cell(), s("sqrt")], span()).unwrap();
        match &*nlazy_sum(&[Value::Int(h).ref_cell()], span()).unwrap().borrow() {
            Value::Float(x) => assert!((*x - 6.0).abs() < 1e-12),
            other => panic!("{other:?}"),
        }
        nlazy_close(&[Value::Int(h).ref_cell()], span()).unwrap();
    }
}
