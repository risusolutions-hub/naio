use crate::ast::{BinOp, Compiled, Expr, UnaryOp};
use crate::error::ExprError;
use crate::parse::parse;
use crate::value::{str_key, Value};
use niao_parallel;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub type ExternalFn = Arc<dyn Fn(&[Value]) -> Result<Value, ExprError>>;

/// Sandbox evaluator with variables, custom functions, and optional restrictions.
#[derive(Clone)]
pub struct Evaluator {
    pub vars: HashMap<Arc<str>, Value>,
    pub fns: HashMap<Arc<str>, ExternalFn>,
    disabled_ops: HashSet<BinOpTag>,
    disabled_fns: HashSet<Arc<str>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOpTag {
    Add,
    Sub,
    Mul,
    Div,
    FloorDiv,
    Mod,
    Pow,
    Eq,
    NotEq,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
    In,
}

impl From<&BinOp> for BinOpTag {
    fn from(op: &BinOp) -> Self {
        match op {
            BinOp::Add => BinOpTag::Add,
            BinOp::Sub => BinOpTag::Sub,
            BinOp::Mul => BinOpTag::Mul,
            BinOp::Div => BinOpTag::Div,
            BinOp::FloorDiv => BinOpTag::FloorDiv,
            BinOp::Mod => BinOpTag::Mod,
            BinOp::Pow => BinOpTag::Pow,
            BinOp::Eq => BinOpTag::Eq,
            BinOp::NotEq => BinOpTag::NotEq,
            BinOp::Lt => BinOpTag::Lt,
            BinOp::Gt => BinOpTag::Gt,
            BinOp::Le => BinOpTag::Le,
            BinOp::Ge => BinOpTag::Ge,
            BinOp::And => BinOpTag::And,
            BinOp::Or => BinOpTag::Or,
            BinOp::In => BinOpTag::In,
        }
    }
}

impl Evaluator {
    pub fn new() -> Self {
        Self {
            vars: HashMap::new(),
            fns: HashMap::new(),
            disabled_ops: HashSet::new(),
            disabled_fns: HashSet::new(),
        }
    }

    pub fn set_var(&mut self, name: &str, value: Value) {
        self.vars.insert(str_key(name), value);
    }

    pub fn get_var(&self, name: &str) -> Option<&Value> {
        self.vars.get(name)
    }

    pub fn clear_vars(&mut self) {
        self.vars.clear();
    }

    pub fn set_fn(&mut self, name: &str, f: ExternalFn) {
        self.fns.insert(str_key(name), f);
    }

    pub fn clear_fns(&mut self) {
        self.fns.clear();
    }

    pub fn var_names(&self) -> Vec<String> {
        self.vars.keys().map(|k| k.to_string()).collect()
    }

    pub fn allow_op(&mut self, op: BinOpTag, allowed: bool) {
        if allowed {
            self.disabled_ops.remove(&op);
        } else {
            self.disabled_ops.insert(op);
        }
    }

    pub fn allow_fn(&mut self, name: &str, allowed: bool) {
        let key = str_key(name);
        if allowed {
            self.disabled_fns.remove(&key);
        } else {
            self.disabled_fns.insert(key);
        }
    }

    pub fn eval(&self, source: &str) -> Result<Value, ExprError> {
        let compiled = parse(source)?;
        self.run(&compiled)
    }

    pub fn run(&self, compiled: &Compiled) -> Result<Value, ExprError> {
        eval_expr(&compiled.expr, self)
    }

    pub fn batch(
        &self,
        compiled: &Compiled,
        rows: &[HashMap<Arc<str>, Value>],
        threads: usize,
    ) -> Vec<Result<Value, ExprError>> {
        if !self.fns.is_empty() || threads == 1 {
            return rows
                .iter()
                .map(|row| {
                    let mut ev = self.clone();
                    for (k, v) in row {
                        ev.vars.insert(Arc::clone(k), v.clone());
                    }
                    ev.run(compiled)
                })
                .collect();
        }
        let base_vars = self.vars.clone();
        let disabled_ops = self.disabled_ops.clone();
        let disabled_fns = self.disabled_fns.clone();
        let expr = compiled.expr.clone();
        niao_parallel::map(rows, threads, |row| {
            let mut vars = base_vars.clone();
            for (k, v) in row {
                vars.insert(Arc::clone(k), v.clone());
            }
            let ev = Evaluator {
                vars,
                fns: HashMap::new(),
                disabled_ops: disabled_ops.clone(),
                disabled_fns: disabled_fns.clone(),
            };
            eval_expr(&expr, &ev)
        })
    }
}

impl Default for Evaluator {
    fn default() -> Self {
        Self::new()
    }
}

pub fn eval_once(source: &str, vars: &HashMap<Arc<str>, Value>) -> Result<Value, ExprError> {
    let mut ev = Evaluator::new();
    ev.vars
        .extend(vars.iter().map(|(k, v)| (Arc::clone(k), v.clone())));
    ev.eval(source)
}

pub fn default_functions() -> &'static [&'static str] {
    &[
        "abs", "round", "min", "max", "len", "sum", "all", "any", "int", "float", "str", "bool",
        "pow", "ord", "chr", "hex", "oct", "bin",
    ]
}

pub fn default_operators() -> &'static [&'static str] {
    &[
        "+", "-", "*", "/", "//", "%", "**", "==", "!=", "<", ">", "<=", ">=", "and", "or", "not",
        "in",
    ]
}

fn eval_expr(expr: &Expr, ev: &Evaluator) -> Result<Value, ExprError> {
    match expr {
        Expr::Nil => Ok(Value::Nil),
        Expr::Bool(b) => Ok(Value::Bool(*b)),
        Expr::Int(n) => Ok(Value::Int(*n)),
        Expr::Float(f) => Ok(Value::Float(*f)),
        Expr::String(s) => Ok(Value::String(Arc::clone(s))),
        Expr::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for e in items {
                out.push(eval_expr(e, ev)?);
            }
            Ok(Value::Array(out))
        }
        Expr::Object(pairs) => {
            let mut map = HashMap::with_capacity(pairs.len());
            for (k, e) in pairs {
                map.insert(Arc::clone(k), eval_expr(e, ev)?);
            }
            Ok(Value::Object(map))
        }
        Expr::Name(n) => ev
            .vars
            .get(n.as_ref())
            .cloned()
            .ok_or_else(|| ExprError::UndefinedVar(n.to_string())),
        Expr::Unary(op, inner) => {
            let v = eval_expr(inner, ev)?;
            match op {
                UnaryOp::Neg => neg_value(v),
                UnaryOp::Not => Ok(Value::Bool(!v.is_truthy())),
            }
        }
        Expr::Binary(op, a, b) => {
            let tag = BinOpTag::from(op);
            if ev.disabled_ops.contains(&tag) {
                return Err(ExprError::Disabled {
                    what: format!("operator {:?}", op),
                });
            }
            let lhs = eval_expr(a, ev)?;
            match op {
                BinOp::And => {
                    if !lhs.is_truthy() {
                        return Ok(Value::Bool(false));
                    }
                    return Ok(Value::Bool(eval_expr(b, ev)?.is_truthy()));
                }
                BinOp::Or => {
                    if lhs.is_truthy() {
                        return Ok(Value::Bool(true));
                    }
                    return Ok(Value::Bool(eval_expr(b, ev)?.is_truthy()));
                }
                _ => {}
            }
            let rhs = eval_expr(b, ev)?;
            eval_binop(op, lhs, rhs)
        }
        Expr::Ternary(then_e, cond, else_e) => {
            if eval_expr(cond, ev)?.is_truthy() {
                eval_expr(then_e, ev)
            } else {
                eval_expr(else_e, ev)
            }
        }
        Expr::Call(callee, args) => {
            let mut evaluated = Vec::with_capacity(args.len());
            for a in args {
                evaluated.push(eval_expr(a, ev)?);
            }
            let name = resolve_call_name(callee, ev)?;
            if ev.disabled_fns.contains(name.as_ref()) {
                return Err(ExprError::Disabled {
                    what: format!("function {name}"),
                });
            }
            if let Some(f) = ev.fns.get(name.as_ref()) {
                return f(&evaluated);
            }
            call_builtin(name.as_ref(), &evaluated)
        }
        Expr::Attr(obj, field) => {
            let v = eval_expr(obj, ev)?;
            getattr(&v, field)
        }
        Expr::Index(obj, idx) => {
            let v = eval_expr(obj, ev)?;
            let i = eval_expr(idx, ev)?;
            getindex(&v, &i)
        }
    }
}

fn resolve_call_name(callee: &Expr, ev: &Evaluator) -> Result<Arc<str>, ExprError> {
    match callee {
        Expr::Name(n) => Ok(Arc::clone(n)),
        Expr::Attr(obj, field) => {
            // For obj.method(), use flat name if obj is a simple variable
            if let Expr::Name(base) = obj.as_ref() {
                Ok(str_key(&format!("{}.{}", base, field)))
            } else {
                // dynamic call target — evaluate and require registered external fn by field name
                let _ = eval_expr(obj, ev)?;
                Ok(Arc::clone(field))
            }
        }
        _ => Err(ExprError::Eval {
            message: "invalid call target".into(),
        }),
    }
}

fn getattr(v: &Value, field: &str) -> Result<Value, ExprError> {
    match v {
        Value::Object(map) => map.get(field).cloned().ok_or_else(|| ExprError::Eval {
            message: format!("object has no attribute '{field}'"),
        }),
        _ => Err(ExprError::Type {
            message: format!("cannot access attribute '{field}' on {}", v.type_name()),
        }),
    }
}

fn getindex(v: &Value, idx: &Value) -> Result<Value, ExprError> {
    match (v, idx) {
        (Value::Array(a), Value::Int(i)) => {
            let i = normalize_index(*i, a.len())?;
            Ok(a[i].clone())
        }
        (Value::String(s), Value::Int(i)) => {
            let chars: Vec<char> = s.chars().collect();
            let i = normalize_index(*i, chars.len())?;
            Ok(Value::String(Arc::from(
                chars.get(i).map(|c| c.to_string()).unwrap_or_default(),
            )))
        }
        (Value::Object(map), Value::String(k)) => {
            map.get(k.as_ref()).cloned().ok_or_else(|| ExprError::Eval {
                message: format!("key '{k}' not found"),
            })
        }
        (Value::Object(map), Value::Int(i)) => {
            let key = i.to_string();
            map.get(key.as_str())
                .cloned()
                .ok_or_else(|| ExprError::Eval {
                    message: format!("key '{key}' not found"),
                })
        }
        _ => Err(ExprError::Type {
            message: format!("cannot index {} with {}", v.type_name(), idx.type_name()),
        }),
    }
}

fn normalize_index(i: i64, len: usize) -> Result<usize, ExprError> {
    let len_i = len as i64;
    let idx = if i < 0 { len_i + i } else { i };
    if idx < 0 || idx >= len_i {
        return Err(ExprError::Eval {
            message: format!("index {i} out of range for length {len}"),
        });
    }
    Ok(idx as usize)
}

fn neg_value(v: Value) -> Result<Value, ExprError> {
    match v {
        Value::Int(n) => Ok(Value::Int(-n)),
        Value::Float(f) => Ok(Value::Float(-f)),
        _ => Err(ExprError::Type {
            message: format!("unary - not supported for {}", v.type_name()),
        }),
    }
}

fn eval_binop(op: &BinOp, lhs: Value, rhs: Value) -> Result<Value, ExprError> {
    match op {
        BinOp::Add => add_values(lhs, rhs),
        BinOp::Sub => num_binop(lhs, rhs, |a, b| a - b, |a, b| a - b),
        BinOp::Mul => num_binop(lhs, rhs, |a, b| a * b, |a, b| a * b),
        BinOp::Div => {
            if is_zero(&rhs) {
                return Err(ExprError::DivByZero);
            }
            num_binop(lhs, rhs, |a, b| a / b, |a, b| a / b)
        }
        BinOp::FloorDiv => {
            if is_zero(&rhs) {
                return Err(ExprError::DivByZero);
            }
            num_binop(lhs, rhs, |a, b| a.div_euclid(b), |a, b| (a / b).floor())
        }
        BinOp::Mod => {
            if is_zero(&rhs) {
                return Err(ExprError::DivByZero);
            }
            num_binop(lhs, rhs, |a, b| a.rem_euclid(b), |a, b| a % b)
        }
        BinOp::Pow => pow_values(lhs, rhs),
        BinOp::Eq => Ok(Value::Bool(values_equal(&lhs, &rhs))),
        BinOp::NotEq => Ok(Value::Bool(!values_equal(&lhs, &rhs))),
        BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => compare_values(op, lhs, rhs),
        BinOp::In => membership(rhs, lhs),
        BinOp::And | BinOp::Or => unreachable!(),
    }
}

fn is_zero(v: &Value) -> bool {
    matches!(v, Value::Int(0)) || matches!(v, Value::Float(f) if *f == 0.0)
}

fn add_values(lhs: Value, rhs: Value) -> Result<Value, ExprError> {
    match (&lhs, &rhs) {
        (Value::String(a), Value::String(b)) => {
            let mut s = String::with_capacity(a.len() + b.len());
            s.push_str(a);
            s.push_str(b);
            Ok(Value::String(Arc::from(s)))
        }
        (Value::String(a), _) => {
            let mut s = String::with_capacity(a.len() + 16);
            s.push_str(a);
            s.push_str(&rhs.to_string_repr());
            Ok(Value::String(Arc::from(s)))
        }
        (_, Value::String(b)) => {
            let mut s = lhs.to_string_repr();
            s.push_str(b);
            Ok(Value::String(Arc::from(s)))
        }
        _ => num_binop(lhs, rhs, |a, b| a + b, |a, b| a + b),
    }
}

fn num_binop<Fi, Ff>(lhs: Value, rhs: Value, fi: Fi, ff: Ff) -> Result<Value, ExprError>
where
    Fi: FnOnce(i64, i64) -> i64,
    Ff: FnOnce(f64, f64) -> f64,
{
    match (lhs, rhs) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(fi(a, b))),
        (a, b) => {
            let af = a.as_float().ok_or_else(|| type_err(&a, &b))?;
            let bf = b.as_float().ok_or_else(|| type_err(&a, &b))?;
            Ok(Value::Float(ff(af, bf)))
        }
    }
}

fn type_err(a: &Value, b: &Value) -> ExprError {
    ExprError::Type {
        message: format!(
            "unsupported operand types: {} and {}",
            a.type_name(),
            b.type_name()
        ),
    }
}

fn pow_values(lhs: Value, rhs: Value) -> Result<Value, ExprError> {
    match (lhs, rhs) {
        (Value::Int(a), Value::Int(b)) if b >= 0 => {
            let r = a.pow(b as u32);
            Ok(Value::Int(r))
        }
        (a, b) => {
            let af = a.as_float().ok_or_else(|| type_err(&a, &b))?;
            let bf = b.as_float().ok_or_else(|| type_err(&a, &b))?;
            Ok(Value::Float(af.powf(bf)))
        }
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Nil, Value::Nil) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x == y,
        (Value::Int(x), Value::Float(y)) | (Value::Float(y), Value::Int(x)) => {
            (*x as f64 - y).abs() < f64::EPSILON
        }
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(a, b)| values_equal(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|ov| values_equal(v, ov)))
        }
        _ => false,
    }
}

fn compare_values(op: &BinOp, lhs: Value, rhs: Value) -> Result<Value, ExprError> {
    let ord = compare_ord(&lhs, &rhs)?;
    let r = match op {
        BinOp::Lt => ord == std::cmp::Ordering::Less,
        BinOp::Gt => ord == std::cmp::Ordering::Greater,
        BinOp::Le => ord != std::cmp::Ordering::Greater,
        BinOp::Ge => ord != std::cmp::Ordering::Less,
        _ => unreachable!(),
    };
    Ok(Value::Bool(r))
}

fn compare_ord(lhs: &Value, rhs: &Value) -> Result<std::cmp::Ordering, ExprError> {
    match (lhs, rhs) {
        (Value::Int(a), Value::Int(b)) => Ok(a.cmp(b)),
        (Value::String(a), Value::String(b)) => Ok(a.cmp(b)),
        (Value::Bool(a), Value::Bool(b)) => Ok(a.cmp(b)),
        _ => {
            let af = lhs.as_float().ok_or_else(|| type_err(lhs, rhs))?;
            let bf = rhs.as_float().ok_or_else(|| type_err(lhs, rhs))?;
            af.partial_cmp(&bf).ok_or_else(|| ExprError::Type {
                message: "cannot compare NaN".into(),
            })
        }
    }
}

fn membership(container: Value, item: Value) -> Result<Value, ExprError> {
    let found = match container {
        Value::String(s) => item.as_str().is_some_and(|needle| s.contains(needle)),
        Value::Array(a) => a.iter().any(|v| values_equal(v, &item)),
        Value::Object(o) => item.as_str().is_some_and(|k| o.contains_key(k)),
        _ => {
            return Err(ExprError::Type {
                message: format!(
                    "'in' expects string, array, or object, got {}",
                    container.type_name()
                ),
            })
        }
    };
    Ok(Value::Bool(found))
}

fn call_builtin(name: &str, args: &[Value]) -> Result<Value, ExprError> {
    match name {
        "abs" => unary_num(args, "abs", |n| n.abs(), |n| n.abs()),
        "round" => {
            arity_range(args, "round", 1, 2)?;
            let n = num_arg(args, 0)?;
            let digits = args.get(1).and_then(|v| v.as_int()).unwrap_or(0);
            let scale = 10f64.powi(digits as i32);
            Ok(Value::Float((n * scale).round() / scale))
        }
        "min" => variadic_minmax(args, true),
        "max" => variadic_minmax(args, false),
        "len" => {
            arity(args, "len", 1)?;
            Ok(Value::Int(match &args[0] {
                Value::String(s) => s.chars().count() as i64,
                Value::Array(a) => a.len() as i64,
                Value::Object(o) => o.len() as i64,
                other => {
                    return Err(ExprError::Type {
                        message: format!("len() not supported for {}", other.type_name()),
                    })
                }
            }))
        }
        "sum" => {
            arity_range(args, "sum", 1, 1)?;
            let Value::Array(items) = &args[0] else {
                return Err(ExprError::Type {
                    message: "sum() expects an array".into(),
                });
            };
            let mut total = 0f64;
            for v in items {
                total += v.as_float().ok_or_else(|| ExprError::Type {
                    message: "sum() elements must be numeric".into(),
                })?;
            }
            Ok(Value::Float(total))
        }
        "all" => {
            arity(args, "all", 1)?;
            let Value::Array(items) = &args[0] else {
                return Err(ExprError::Type {
                    message: "all() expects an array".into(),
                });
            };
            Ok(Value::Bool(items.iter().all(|v| v.is_truthy())))
        }
        "any" => {
            arity(args, "any", 1)?;
            let Value::Array(items) = &args[0] else {
                return Err(ExprError::Type {
                    message: "any() expects an array".into(),
                });
            };
            Ok(Value::Bool(items.iter().any(|v| v.is_truthy())))
        }
        "int" => {
            arity_range(args, "int", 1, 2)?;
            let base = args.get(1).and_then(|v| v.as_int()).unwrap_or(10);
            if let Value::String(s) = &args[0] {
                let n =
                    i64::from_str_radix(s.trim(), base as u32).map_err(|_| ExprError::Type {
                        message: format!("invalid int literal '{s}'"),
                    })?;
                return Ok(Value::Int(n));
            }
            let n = num_arg(args, 0)?;
            Ok(Value::Int(n.trunc() as i64))
        }
        "float" => {
            arity(args, "float", 1)?;
            if let Value::String(s) = &args[0] {
                let n: f64 = s.parse().map_err(|_| ExprError::Type {
                    message: format!("invalid float '{s}'"),
                })?;
                return Ok(Value::Float(n));
            }
            Ok(Value::Float(num_arg(args, 0)?))
        }
        "str" => {
            arity(args, "str", 1)?;
            Ok(Value::String(Arc::from(args[0].to_string_repr())))
        }
        "bool" => {
            arity(args, "bool", 1)?;
            Ok(Value::Bool(args[0].is_truthy()))
        }
        "pow" => {
            arity(args, "pow", 2)?;
            pow_values(args[0].clone(), args[1].clone())
        }
        "ord" => {
            arity(args, "ord", 1)?;
            let Value::String(s) = &args[0] else {
                return Err(ExprError::Type {
                    message: "ord() expects a single-character string".into(),
                });
            };
            let mut it = s.chars();
            let ch = it.next().ok_or_else(|| ExprError::Type {
                message: "ord() expects a non-empty string".into(),
            })?;
            if it.next().is_some() {
                return Err(ExprError::Type {
                    message: "ord() expects a single-character string".into(),
                });
            }
            Ok(Value::Int(ch as u32 as i64))
        }
        "chr" => {
            arity(args, "chr", 1)?;
            let n = args[0].as_int().ok_or_else(|| ExprError::Type {
                message: "chr() expects int".into(),
            })?;
            let ch = char::from_u32(n as u32).ok_or_else(|| ExprError::Eval {
                message: format!("chr() out of range: {n}"),
            })?;
            Ok(Value::String(Arc::from(ch.to_string())))
        }
        "hex" => radix_format(args, "hex", 16),
        "oct" => radix_format(args, "oct", 8),
        "bin" => radix_format(args, "bin", 2),
        other => Err(ExprError::UndefinedFn(other.to_string())),
    }
}

fn radix_format(args: &[Value], name: &str, radix: u32) -> Result<Value, ExprError> {
    arity(args, name, 1)?;
    let n = args[0].as_int().ok_or_else(|| ExprError::Type {
        message: format!("{name}() expects int"),
    })?;
    let s = match radix {
        16 => format!("{n:x}"),
        8 => format!("{n:o}"),
        2 => format!("{n:b}"),
        _ => n.to_string(),
    };
    Ok(Value::String(Arc::from(s)))
}

fn variadic_minmax(args: &[Value], min: bool) -> Result<Value, ExprError> {
    if args.is_empty() {
        return Err(ExprError::Arity {
            name: if min { "min" } else { "max" }.into(),
            expected: "at least 1".into(),
            got: 0,
        });
    }
    let mut best = num_arg(args, 0)?;
    for a in &args[1..] {
        let n = a.as_float().ok_or_else(|| ExprError::Type {
            message: "min/max args must be numeric".into(),
        })?;
        if (min && n < best) || (!min && n > best) {
            best = n;
        }
    }
    Ok(Value::Float(best))
}

fn unary_num<Fi, Ff>(args: &[Value], name: &str, fi: Fi, ff: Ff) -> Result<Value, ExprError>
where
    Fi: FnOnce(i64) -> i64,
    Ff: FnOnce(f64) -> f64,
{
    arity(args, name, 1)?;
    match &args[0] {
        Value::Int(n) => Ok(Value::Int(fi(*n))),
        Value::Float(f) => Ok(Value::Float(ff(*f))),
        other => Err(ExprError::Type {
            message: format!("{name}() expects number, got {}", other.type_name()),
        }),
    }
}

fn arity(args: &[Value], name: &str, n: usize) -> Result<(), ExprError> {
    if args.len() != n {
        return Err(ExprError::Arity {
            name: name.into(),
            expected: n.to_string(),
            got: args.len(),
        });
    }
    Ok(())
}

fn arity_range(args: &[Value], name: &str, min: usize, max: usize) -> Result<(), ExprError> {
    if args.len() < min || args.len() > max {
        return Err(ExprError::Arity {
            name: name.into(),
            expected: format!("{min}..={max}"),
            got: args.len(),
        });
    }
    Ok(())
}

fn num_arg(args: &[Value], idx: usize) -> Result<f64, ExprError> {
    args[idx].as_float().ok_or_else(|| ExprError::Type {
        message: format!("expected number at argument {}", idx + 1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::str_key;
    use std::collections::HashMap;

    #[test]
    fn eval_arithmetic() {
        let v = eval_once("2 + 3 * 4", &HashMap::new()).unwrap();
        assert_eq!(v, Value::Int(14));
    }

    #[test]
    fn eval_ternary() {
        let mut vars = HashMap::new();
        vars.insert(str_key("x"), Value::Int(10));
        let v = eval_once("100 if x > 5 else 0", &vars).unwrap();
        assert_eq!(v, Value::Int(100));
    }

    #[test]
    fn eval_in_operator() {
        let v = eval_once("'ell' in 'hello'", &HashMap::new()).unwrap();
        assert_eq!(v, Value::Bool(true));
    }

    #[test]
    fn batch_parallel() {
        let c = parse("a + b").unwrap();
        let ev = Evaluator::new();
        let rows: Vec<_> = (0..100)
            .map(|i| {
                let mut m = HashMap::new();
                m.insert(str_key("a"), Value::Int(i));
                m.insert(str_key("b"), Value::Int(1));
                m
            })
            .collect();
        let out = ev.batch(&c, &rows, 4);
        assert_eq!(out.len(), 100);
        assert_eq!(out[0].as_ref().unwrap(), &Value::Int(1));
        assert_eq!(out[99].as_ref().unwrap(), &Value::Int(100));
    }
}
