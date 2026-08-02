//! Native nsym standard library — symbolic math with expression trees,
//! simplification, symbolic differentiation, and equation solving.
//!
//! Import with `import "nsym"` (or `import "std/nsym"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
enum Expr {
    Num(f64),
    Var(String),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    Pow(Box<Expr>, Box<Expr>),
    Neg(Box<Expr>),
}

thread_local! {
    static EXPRS: RefCell<HashMap<i64, Expr>> = RefCell::new(HashMap::new());
    static NEXT_ID: RefCell<i64> = const { RefCell::new(1) };
}

fn alloc_expr(e: Expr) -> i64 {
    let id = NEXT_ID.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    EXPRS.with(|m| m.borrow_mut().insert(id, e));
    id
}

fn get_expr(id: i64, span: Span) -> NiaoResult<Expr> {
    EXPRS.with(|m| {
        m.borrow().get(&id).cloned().ok_or_else(|| {
            RuntimeError::at(
                span,
                codes::E4594_NSYM_HANDLE,
                format!("invalid symbolic expression handle {id}"),
            )
        })
    })
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E4592_NSYM_TYPE, msg.into())
}

fn nsym_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E4591_NSYM_ERROR, "nsym_error", msg.into(), span)
}

fn parse_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E4593_NSYM_PARSE, "nsym_error", msg.into(), span)
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E4590_NSYM_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
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
                "{name}() expects positive handle as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

struct Parser<'a> {
    src: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src: src.as_bytes(),
            i: 0,
        }
    }
    fn parse(src: &'a str) -> Result<Expr, String> {
        let mut p = Parser::new(src);
        let expr = p.parse_expr()?;
        p.skip_ws();
        if p.i != p.src.len() {
            return Err(format!("unexpected token at byte {}", p.i));
        }
        Ok(expr)
    }
    fn peek(&self) -> Option<u8> {
        self.src.get(self.i).copied()
    }
    fn next(&mut self) -> Option<u8> {
        let ch = self.peek()?;
        self.i += 1;
        Some(ch)
    }
    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if (c as char).is_whitespace() {
                self.i += 1;
            } else {
                break;
            }
        }
    }
    fn eat(&mut self, c: u8) -> bool {
        self.skip_ws();
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_add_sub()
    }
    fn parse_add_sub(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_mul_div()?;
        loop {
            self.skip_ws();
            if self.eat(b'+') {
                left = Expr::Add(Box::new(left), Box::new(self.parse_mul_div()?));
            } else if self.eat(b'-') {
                left = Expr::Sub(Box::new(left), Box::new(self.parse_mul_div()?));
            } else {
                break;
            }
        }
        Ok(left)
    }
    fn parse_mul_div(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_pow()?;
        loop {
            self.skip_ws();
            if self.eat(b'*') {
                left = Expr::Mul(Box::new(left), Box::new(self.parse_pow()?));
            } else if self.eat(b'/') {
                left = Expr::Div(Box::new(left), Box::new(self.parse_pow()?));
            } else {
                break;
            }
        }
        Ok(left)
    }
    fn parse_pow(&mut self) -> Result<Expr, String> {
        let left = self.parse_unary()?;
        self.skip_ws();
        if self.eat(b'^') {
            let right = self.parse_pow()?;
            Ok(Expr::Pow(Box::new(left), Box::new(right)))
        } else {
            Ok(left)
        }
    }
    fn parse_unary(&mut self) -> Result<Expr, String> {
        self.skip_ws();
        if self.eat(b'-') {
            Ok(Expr::Neg(Box::new(self.parse_unary()?)))
        } else {
            self.parse_atom()
        }
    }
    fn parse_atom(&mut self) -> Result<Expr, String> {
        self.skip_ws();
        if self.eat(b'(') {
            let e = self.parse_expr()?;
            if !self.eat(b')') {
                return Err("expected ')'".to_string());
            }
            return Ok(e);
        }
        if let Some(c) = self.peek() {
            if (c as char).is_ascii_digit() || c == b'.' {
                return self.parse_number();
            }
            if (c as char).is_ascii_alphabetic() || c == b'_' {
                return self.parse_ident();
            }
        }
        Err("expected number, variable, or parenthesized expression".to_string())
    }
    fn parse_number(&mut self) -> Result<Expr, String> {
        self.skip_ws();
        let start = self.i;
        let mut seen_dot = false;
        while let Some(c) = self.peek() {
            if c == b'.' && !seen_dot {
                seen_dot = true;
                self.i += 1;
            } else if (c as char).is_ascii_digit() {
                self.i += 1;
            } else {
                break;
            }
        }
        let s = std::str::from_utf8(&self.src[start..self.i]).map_err(|_| "invalid number")?;
        let n = s.parse::<f64>().map_err(|_| "invalid number literal")?;
        Ok(Expr::Num(n))
    }
    fn parse_ident(&mut self) -> Result<Expr, String> {
        self.skip_ws();
        let start = self.i;
        while let Some(c) = self.peek() {
            if (c as char).is_ascii_alphanumeric() || c == b'_' {
                self.i += 1;
            } else {
                break;
            }
        }
        let s = std::str::from_utf8(&self.src[start..self.i]).map_err(|_| "invalid identifier")?;
        Ok(Expr::Var(s.to_string()))
    }
}

fn fmt_expr(e: &Expr) -> String {
    fn rec(e: &Expr, out: &mut String, prec: u8) {
        match e {
            Expr::Num(n) => {
                let _ = write!(out, "{n}");
            }
            Expr::Var(v) => out.push_str(v),
            Expr::Neg(x) => {
                out.push('-');
                rec(x, out, 9);
            }
            Expr::Add(a, b) => {
                if prec > 1 {
                    out.push('(');
                }
                rec(a, out, 1);
                out.push_str(" + ");
                rec(b, out, 1);
                if prec > 1 {
                    out.push(')');
                }
            }
            Expr::Sub(a, b) => {
                if prec > 1 {
                    out.push('(');
                }
                rec(a, out, 1);
                out.push_str(" - ");
                rec(b, out, 2);
                if prec > 1 {
                    out.push(')');
                }
            }
            Expr::Mul(a, b) => {
                if prec > 2 {
                    out.push('(');
                }
                rec(a, out, 2);
                out.push_str(" * ");
                rec(b, out, 2);
                if prec > 2 {
                    out.push(')');
                }
            }
            Expr::Div(a, b) => {
                if prec > 2 {
                    out.push('(');
                }
                rec(a, out, 2);
                out.push_str(" / ");
                rec(b, out, 3);
                if prec > 2 {
                    out.push(')');
                }
            }
            Expr::Pow(a, b) => {
                if prec > 3 {
                    out.push('(');
                }
                rec(a, out, 3);
                out.push('^');
                rec(b, out, 4);
                if prec > 3 {
                    out.push(')');
                }
            }
        }
    }
    let mut s = String::new();
    rec(e, &mut s, 0);
    s
}

fn as_num(e: &Expr) -> Option<f64> {
    if let Expr::Num(n) = e {
        Some(*n)
    } else {
        None
    }
}

fn simplify(e: Expr) -> Expr {
    match e {
        Expr::Neg(x) => {
            let sx = simplify(*x);
            match sx {
                Expr::Num(n) => Expr::Num(-n),
                Expr::Neg(inner) => *inner,
                _ => Expr::Neg(Box::new(sx)),
            }
        }
        Expr::Add(a, b) => {
            let sa = simplify(*a);
            let sb = simplify(*b);
            match (as_num(&sa), as_num(&sb)) {
                (Some(x), Some(y)) => Expr::Num(x + y),
                (Some(0.0), _) => sb,
                (_, Some(0.0)) => sa,
                _ => Expr::Add(Box::new(sa), Box::new(sb)),
            }
        }
        Expr::Sub(a, b) => {
            let sa = simplify(*a);
            let sb = simplify(*b);
            match (as_num(&sa), as_num(&sb)) {
                (Some(x), Some(y)) => Expr::Num(x - y),
                (_, Some(0.0)) => sa,
                _ if sa == sb => Expr::Num(0.0),
                _ => Expr::Sub(Box::new(sa), Box::new(sb)),
            }
        }
        Expr::Mul(a, b) => {
            let sa = simplify(*a);
            let sb = simplify(*b);
            match (as_num(&sa), as_num(&sb)) {
                (Some(x), Some(y)) => Expr::Num(x * y),
                (Some(0.0), _) | (_, Some(0.0)) => Expr::Num(0.0),
                (Some(1.0), _) => sb,
                (_, Some(1.0)) => sa,
                _ => Expr::Mul(Box::new(sa), Box::new(sb)),
            }
        }
        Expr::Div(a, b) => {
            let sa = simplify(*a);
            let sb = simplify(*b);
            match (as_num(&sa), as_num(&sb)) {
                (_, Some(0.0)) => Expr::Div(Box::new(sa), Box::new(sb)),
                (Some(x), Some(y)) => Expr::Num(x / y),
                (_, Some(1.0)) => sa,
                _ if sa == sb => Expr::Num(1.0),
                _ => Expr::Div(Box::new(sa), Box::new(sb)),
            }
        }
        Expr::Pow(a, b) => {
            let sa = simplify(*a);
            let sb = simplify(*b);
            match (as_num(&sa), as_num(&sb)) {
                (_, Some(0.0)) => Expr::Num(1.0),
                (_, Some(1.0)) => sa,
                (Some(x), Some(y)) => Expr::Num(x.powf(y)),
                _ => Expr::Pow(Box::new(sa), Box::new(sb)),
            }
        }
        other => other,
    }
}

fn vars(e: &Expr, out: &mut BTreeSet<String>) {
    match e {
        Expr::Var(v) => {
            out.insert(v.clone());
        }
        Expr::Num(_) => {}
        Expr::Neg(x) => vars(x, out),
        Expr::Add(a, b) | Expr::Sub(a, b) | Expr::Mul(a, b) | Expr::Div(a, b) | Expr::Pow(a, b) => {
            vars(a, out);
            vars(b, out);
        }
    }
}

fn derivative(e: &Expr, var: &str) -> Expr {
    match e {
        Expr::Num(_) => Expr::Num(0.0),
        Expr::Var(v) => {
            if v == var {
                Expr::Num(1.0)
            } else {
                Expr::Num(0.0)
            }
        }
        Expr::Neg(x) => Expr::Neg(Box::new(derivative(x, var))),
        Expr::Add(a, b) => Expr::Add(Box::new(derivative(a, var)), Box::new(derivative(b, var))),
        Expr::Sub(a, b) => Expr::Sub(Box::new(derivative(a, var)), Box::new(derivative(b, var))),
        Expr::Mul(a, b) => Expr::Add(
            Box::new(Expr::Mul(Box::new(derivative(a, var)), Box::new((**b).clone()))),
            Box::new(Expr::Mul(Box::new((**a).clone()), Box::new(derivative(b, var)))),
        ),
        Expr::Div(a, b) => Expr::Div(
            Box::new(Expr::Sub(
                Box::new(Expr::Mul(Box::new(derivative(a, var)), Box::new((**b).clone()))),
                Box::new(Expr::Mul(Box::new((**a).clone()), Box::new(derivative(b, var)))),
            )),
            Box::new(Expr::Pow(Box::new((**b).clone()), Box::new(Expr::Num(2.0)))),
        ),
        Expr::Pow(base, exp) => {
            if let Some(n) = as_num(exp) {
                let n1 = n - 1.0;
                Expr::Mul(
                    Box::new(Expr::Mul(
                        Box::new(Expr::Num(n)),
                        Box::new(Expr::Pow(Box::new((**base).clone()), Box::new(Expr::Num(n1)))),
                    )),
                    Box::new(derivative(base, var)),
                )
            } else {
                Expr::Num(0.0)
            }
        }
    }
}

fn eval(e: &Expr, env: &HashMap<String, f64>) -> Result<f64, String> {
    match e {
        Expr::Num(n) => Ok(*n),
        Expr::Var(v) => env
            .get(v)
            .copied()
            .ok_or_else(|| format!("missing variable '{v}'")),
        Expr::Neg(x) => Ok(-eval(x, env)?),
        Expr::Add(a, b) => Ok(eval(a, env)? + eval(b, env)?),
        Expr::Sub(a, b) => Ok(eval(a, env)? - eval(b, env)?),
        Expr::Mul(a, b) => Ok(eval(a, env)? * eval(b, env)?),
        Expr::Div(a, b) => {
            let den = eval(b, env)?;
            if den == 0.0 {
                return Err("division by zero".to_string());
            }
            Ok(eval(a, env)? / den)
        }
        Expr::Pow(a, b) => Ok(eval(a, env)?.powf(eval(b, env)?)),
    }
}

fn subst(e: &Expr, var: &str, val: &Expr) -> Expr {
    match e {
        Expr::Num(n) => Expr::Num(*n),
        Expr::Var(v) => {
            if v == var {
                val.clone()
            } else {
                Expr::Var(v.clone())
            }
        }
        Expr::Neg(x) => Expr::Neg(Box::new(subst(x, var, val))),
        Expr::Add(a, b) => Expr::Add(Box::new(subst(a, var, val)), Box::new(subst(b, var, val))),
        Expr::Sub(a, b) => Expr::Sub(Box::new(subst(a, var, val)), Box::new(subst(b, var, val))),
        Expr::Mul(a, b) => Expr::Mul(Box::new(subst(a, var, val)), Box::new(subst(b, var, val))),
        Expr::Div(a, b) => Expr::Div(Box::new(subst(a, var, val)), Box::new(subst(b, var, val))),
        Expr::Pow(a, b) => Expr::Pow(Box::new(subst(a, var, val)), Box::new(subst(b, var, val))),
    }
}

fn poly_coeffs(e: &Expr, var: &str) -> Option<(f64, f64, f64)> {
    match e {
        Expr::Num(n) => Some((0.0, 0.0, *n)),
        Expr::Var(v) => {
            if v == var {
                Some((0.0, 1.0, 0.0))
            } else {
                None
            }
        }
        Expr::Neg(x) => {
            let (a, b, c) = poly_coeffs(x, var)?;
            Some((-a, -b, -c))
        }
        Expr::Add(l, r) => {
            let (a1, b1, c1) = poly_coeffs(l, var)?;
            let (a2, b2, c2) = poly_coeffs(r, var)?;
            Some((a1 + a2, b1 + b2, c1 + c2))
        }
        Expr::Sub(l, r) => {
            let (a1, b1, c1) = poly_coeffs(l, var)?;
            let (a2, b2, c2) = poly_coeffs(r, var)?;
            Some((a1 - a2, b1 - b2, c1 - c2))
        }
        Expr::Mul(l, r) => {
            let (a1, b1, c1) = poly_coeffs(l, var)?;
            let (a2, b2, c2) = poly_coeffs(r, var)?;
            let a = a1 * c2 + b1 * b2 + c1 * a2;
            let b = b1 * c2 + c1 * b2;
            let c = c1 * c2;
            let cubic = a1 * b2 + b1 * a2;
            let quartic = a1 * a2;
            if cubic != 0.0 || quartic != 0.0 {
                return None;
            }
            Some((a, b, c))
        }
        Expr::Div(l, r) => {
            let (a2, b2, c2) = poly_coeffs(r, var)?;
            if a2 != 0.0 || b2 != 0.0 || c2 == 0.0 {
                return None;
            }
            let (a1, b1, c1) = poly_coeffs(l, var)?;
            Some((a1 / c2, b1 / c2, c1 / c2))
        }
        Expr::Pow(base, exp) => {
            if let (Expr::Var(v), Expr::Num(n)) = (&**base, &**exp) {
                if v == var && *n == 2.0 {
                    return Some((1.0, 0.0, 0.0));
                }
                if v == var && *n == 1.0 {
                    return Some((0.0, 1.0, 0.0));
                }
                if *n == 0.0 {
                    return Some((0.0, 0.0, 1.0));
                }
            }
            None
        }
    }
}

fn map_to_f64(value: &ValueRef) -> Result<HashMap<String, f64>, String> {
    match &*value.borrow() {
        Value::Object(map) => {
            let mut out = HashMap::with_capacity(map.len());
            for (k, v) in map {
                let n = match &*v.borrow() {
                    Value::Int(i) => *i as f64,
                    Value::Float(f) => *f,
                    other => {
                        return Err(format!(
                            "variable '{k}' must be int or float, got {}",
                            other.type_name()
                        ))
                    }
                };
                out.insert(k.clone(), n);
            }
            Ok(out)
        }
        other => Err(format!("expected variables object, got {}", other.type_name())),
    }
}

// >>> import "nsym"
// >>> let e = nsym.parse("x^2 + 2*x + 1")
// >>> type(e)
// => "int"
fn nsym_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsym_parse", span)?;
    let src = string_arg(args, 0, "nsym_parse", span)?;
    match Parser::parse(&src) {
        Ok(e) => Ok(Value::Int(alloc_expr(simplify(e))).ref_cell()),
        Err(msg) => Ok(parse_err(span, msg)),
    }
}

// >>> let e = nsym.parse("x + x + 0")
// >>> nsym.repr(nsym.simplify(e))
// => "x + x"
fn nsym_simplify(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsym_simplify", span)?;
    let id = handle_arg(args, 0, "nsym_simplify", span)?;
    let e = get_expr(id, span)?;
    Ok(Value::Int(alloc_expr(simplify(e))).ref_cell())
}

// >>> let e = nsym.parse("x^2 + 2*x + 1")
// >>> let d = nsym.diff(e, "x")
// >>> nsym.repr(nsym.simplify(d))
// => "2 * x + 2"
fn nsym_diff(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsym_diff", span)?;
    let id = handle_arg(args, 0, "nsym_diff", span)?;
    let var = string_arg(args, 1, "nsym_diff", span)?;
    let e = get_expr(id, span)?;
    let d = simplify(derivative(&e, &var));
    Ok(Value::Int(alloc_expr(d)).ref_cell())
}

// >>> let e = nsym.parse("x^2 + 2*x + 1")
// >>> nsym.repr(e)
// => "x^2 + 2 * x + 1"
fn nsym_repr(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsym_repr", span)?;
    let id = handle_arg(args, 0, "nsym_repr", span)?;
    let e = get_expr(id, span)?;
    Ok(Value::String(fmt_expr(&e)).ref_cell())
}

// >>> let e = nsym.parse("a*x + b")
// >>> nsym.vars(e)
// => ["a", "b", "x"]
fn nsym_vars(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsym_vars", span)?;
    let id = handle_arg(args, 0, "nsym_vars", span)?;
    let e = get_expr(id, span)?;
    let mut set = BTreeSet::new();
    vars(&e, &mut set);
    let out: Vec<ValueRef> = set.into_iter().map(|s| Value::String(s).ref_cell()).collect();
    Ok(Value::Array(out).ref_cell())
}

// >>> let e = nsym.parse("x^2 + 2*x + 1")
// >>> nsym.eval(e, { "x": 3 })
// => 16
fn nsym_eval(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsym_eval", span)?;
    let id = handle_arg(args, 0, "nsym_eval", span)?;
    let env = map_to_f64(&args[1]).map_err(|m| type_err(span, m))?;
    let e = get_expr(id, span)?;
    match eval(&e, &env) {
        Ok(v) => Ok(Value::Float(v).ref_cell()),
        Err(msg) => Ok(nsym_err(span, msg)),
    }
}

// >>> let e = nsym.parse("x^2 + y")
// >>> let e2 = nsym.subst(e, "y", nsym.parse("2*x"))
// >>> nsym.repr(nsym.simplify(e2))
// => "x^2 + 2 * x"
fn nsym_subst(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nsym_subst", span)?;
    let e_id = handle_arg(args, 0, "nsym_subst", span)?;
    let var = string_arg(args, 1, "nsym_subst", span)?;
    let r_id = handle_arg(args, 2, "nsym_subst", span)?;
    let e = get_expr(e_id, span)?;
    let repl = get_expr(r_id, span)?;
    Ok(Value::Int(alloc_expr(simplify(subst(&e, &var, &repl)))).ref_cell())
}

// >>> let lhs = nsym.parse("2*x + 4")
// >>> let rhs = nsym.parse("10")
// >>> nsym.solve_linear(lhs, rhs, "x")
// => 3
fn nsym_solve_linear(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nsym_solve_linear", span)?;
    let l = get_expr(handle_arg(args, 0, "nsym_solve_linear", span)?, span)?;
    let r = get_expr(handle_arg(args, 1, "nsym_solve_linear", span)?, span)?;
    let v = string_arg(args, 2, "nsym_solve_linear", span)?;
    let e = simplify(Expr::Sub(Box::new(l), Box::new(r)));
    match poly_coeffs(&e, &v) {
        Some((a, b, c)) if a == 0.0 && b != 0.0 => Ok(Value::Float(-c / b).ref_cell()),
        Some((a, b, _)) if a == 0.0 && b == 0.0 => Ok(nsym_err(span, "equation is degenerate for linear solve")),
        Some(_) => Ok(nsym_err(span, "equation is not linear in target variable")),
        None => Ok(nsym_err(span, "equation is not polynomial up to degree 2")),
    }
}

// >>> let lhs = nsym.parse("x^2 - 5*x + 6")
// >>> let rhs = nsym.parse("0")
// >>> nsym.solve_quadratic(lhs, rhs, "x")
// => [2, 3]
fn nsym_solve_quadratic(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nsym_solve_quadratic", span)?;
    let l = get_expr(handle_arg(args, 0, "nsym_solve_quadratic", span)?, span)?;
    let r = get_expr(handle_arg(args, 1, "nsym_solve_quadratic", span)?, span)?;
    let v = string_arg(args, 2, "nsym_solve_quadratic", span)?;
    let e = simplify(Expr::Sub(Box::new(l), Box::new(r)));
    match poly_coeffs(&e, &v) {
        Some((a, b, c)) if a != 0.0 => {
            let d = b * b - 4.0 * a * c;
            if d < 0.0 {
                return Ok(nsym_err(span, "no real roots"));
            }
            let sd = d.sqrt();
            let x1 = (-b - sd) / (2.0 * a);
            let x2 = (-b + sd) / (2.0 * a);
            let mut roots = vec![x1, x2];
            roots.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
            let out: Vec<ValueRef> = roots.into_iter().map(|x| Value::Float(x).ref_cell()).collect();
            Ok(Value::Array(out).ref_cell())
        }
        Some((_, b, c)) if b != 0.0 => Ok(Value::Array(vec![Value::Float(-c / b).ref_cell()]).ref_cell()),
        Some(_) => Ok(nsym_err(span, "equation is degenerate")),
        None => Ok(nsym_err(span, "equation is not polynomial up to degree 2")),
    }
}

// >>> let e = nsym.parse("x + 1")
// >>> nsym.free(e)
// => nil
fn nsym_free(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsym_free", span)?;
    let id = handle_arg(args, 0, "nsym_free", span)?;
    EXPRS.with(|m| {
        if m.borrow_mut().remove(&id).is_none() {
            return Err(RuntimeError::at(
                span,
                codes::E4594_NSYM_HANDLE,
                format!("invalid symbolic expression handle {id}"),
            ));
        }
        Ok(Value::Nil.ref_cell())
    })
}

macro_rules! nsym_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nsym_fns![
    ("nsym_parse", "parse", nsym_parse),
    ("nsym_simplify", "simplify", nsym_simplify),
    ("nsym_diff", "diff", nsym_diff),
    ("nsym_repr", "repr", nsym_repr),
    ("nsym_vars", "vars", nsym_vars),
    ("nsym_eval", "eval", nsym_eval),
    ("nsym_subst", "subst", nsym_subst),
    ("nsym_solve_linear", "solve_linear", nsym_solve_linear),
    ("nsym_solve_quadratic", "solve_quadratic", nsym_solve_quadratic),
    ("nsym_free", "free", nsym_free),
];

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nsym";
pub const MODULE_PATHS: &[&str] = &["nsym", "std/nsym"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simplify_fold_constants() {
        let e = Parser::parse("2 + 3*4").expect("parse ok");
        assert_eq!(simplify(e), Expr::Num(14.0));
    }
}
