//! Native nlint standard library — AST-as-data via `niao_parser`, data-driven
//! lint rules, and `nlint_check(source)` diagnostics.
//!
//! Import with `import "nlint"` (or `import "std/nlint"`).

use crate::{call_niao_function, error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::*;
use niao_parser::{parse, ParseError};
use std::collections::HashMap;
use std::rc::Rc;

const E3220_NLINT_ARITY: u32 = 3220;
const E3221_NLINT_ERROR: u32 = 3221;
const E3222_NLINT_TYPE: u32 = 3222;
const E3223_NLINT_PARSE: u32 = 3223;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3220_NLINT_ARITY,
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
            E3220_NLINT_ARITY,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3222_NLINT_TYPE, msg.into())
}

fn lint_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3221_NLINT_ERROR, "nlint_error", msg.into(), span)
}

fn parse_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3223_NLINT_PARSE, "nlint_error", msg.into(), span)
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

fn parse_source(source: &str, span: Span) -> Result<Program, ValueRef> {
    parse(source).map_err(|e| parse_err(span, format_parse_error(&e)))
}

fn format_parse_error(e: &ParseError) -> String {
    match e {
        ParseError::Eof => "unexpected end of file".into(),
        ParseError::Unexpected {
            found,
            expected,
            line,
            col,
        } => format!("line {line}, col {col}: expected {expected}, found {found}"),
        ParseError::Lex(le) => format!("lexer error: {le}"),
    }
}

// ---------------------------------------------------------------------------
// AST → Value (AST-as-data)
// ---------------------------------------------------------------------------

fn span_obj(s: Span) -> ValueRef {
    let mut m = HashMap::new();
    m.insert("line".to_string(), Value::Int(s.line as i64).ref_cell());
    m.insert("col".to_string(), Value::Int(s.col as i64).ref_cell());
    m.insert("start".to_string(), Value::Int(s.start as i64).ref_cell());
    m.insert("end".to_string(), Value::Int(s.end as i64).ref_cell());
    Value::Object(m).ref_cell()
}

fn str_val(s: &str) -> ValueRef {
    Value::String(s.to_string()).ref_cell()
}

fn binop_name(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "Add",
        BinOp::Sub => "Sub",
        BinOp::Mul => "Mul",
        BinOp::Div => "Div",
        BinOp::FloorDiv => "FloorDiv",
        BinOp::Mod => "Mod",
        BinOp::Eq => "Eq",
        BinOp::Ne => "Ne",
        BinOp::Lt => "Lt",
        BinOp::Gt => "Gt",
        BinOp::Le => "Le",
        BinOp::Ge => "Ge",
        BinOp::And => "And",
        BinOp::Or => "Or",
    }
}

fn unaryop_name(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Not => "Not",
        UnaryOp::Neg => "Neg",
    }
}

fn type_name_val(t: &TypeName) -> ValueRef {
    match t {
        TypeName::Int => str_val("int"),
        TypeName::Float => str_val("float"),
        TypeName::String => str_val("string"),
        TypeName::Bool => str_val("bool"),
        TypeName::Void => str_val("void"),
        TypeName::Array => str_val("array"),
        TypeName::Error => str_val("error"),
        TypeName::Named(n) => str_val(n),
    }
}

fn expr_val(e: &Expr) -> ValueRef {
    let mut m = HashMap::new();
    m.insert("span".to_string(), span_obj(e.span()));
    match e {
        Expr::Int(v, _) => {
            m.insert("kind".to_string(), str_val("Int"));
            m.insert("value".to_string(), Value::Int(*v).ref_cell());
        }
        Expr::Float(v, _) => {
            m.insert("kind".to_string(), str_val("Float"));
            m.insert("value".to_string(), Value::Float(*v).ref_cell());
        }
        Expr::String(v, _) => {
            m.insert("kind".to_string(), str_val("String"));
            m.insert("value".to_string(), str_val(v));
        }
        Expr::Bool(v, _) => {
            m.insert("kind".to_string(), str_val("Bool"));
            m.insert("value".to_string(), Value::Bool(*v).ref_cell());
        }
        Expr::Nil(_) => {
            m.insert("kind".to_string(), str_val("Nil"));
        }
        Expr::Ident(name, _) => {
            m.insert("kind".to_string(), str_val("Ident"));
            m.insert("name".to_string(), str_val(name));
        }
        Expr::Binary {
            left, op, right, ..
        } => {
            m.insert("kind".to_string(), str_val("Binary"));
            m.insert("op".to_string(), str_val(binop_name(*op)));
            m.insert("left".to_string(), expr_val(left));
            m.insert("right".to_string(), expr_val(right));
        }
        Expr::Unary { op, expr, .. } => {
            m.insert("kind".to_string(), str_val("Unary"));
            m.insert("op".to_string(), str_val(unaryop_name(*op)));
            m.insert("expr".to_string(), expr_val(expr));
        }
        Expr::Call { callee, args, .. } => {
            m.insert("kind".to_string(), str_val("Call"));
            m.insert("callee".to_string(), expr_val(callee));
            m.insert(
                "args".to_string(),
                Value::Array(args.iter().map(expr_val).collect()).ref_cell(),
            );
        }
        Expr::Member { object, field, .. } => {
            m.insert("kind".to_string(), str_val("Member"));
            m.insert("object".to_string(), expr_val(object));
            m.insert("field".to_string(), str_val(field));
        }
        Expr::Index { object, index, .. } => {
            m.insert("kind".to_string(), str_val("Index"));
            m.insert("object".to_string(), expr_val(object));
            m.insert("index".to_string(), expr_val(index));
        }
        Expr::Array { elements, .. } => {
            m.insert("kind".to_string(), str_val("Array"));
            m.insert(
                "elements".to_string(),
                Value::Array(elements.iter().map(expr_val).collect()).ref_cell(),
            );
        }
        Expr::Object { fields, .. } => {
            m.insert("kind".to_string(), str_val("Object"));
            let mut obj = HashMap::new();
            for (k, v) in fields {
                obj.insert(k.clone(), expr_val(v));
            }
            m.insert("fields".to_string(), Value::Object(obj).ref_cell());
        }
        Expr::StructInit { name, fields, .. } => {
            m.insert("kind".to_string(), str_val("StructInit"));
            m.insert("name".to_string(), str_val(name));
            let mut obj = HashMap::new();
            for (k, v) in fields {
                obj.insert(k.clone(), expr_val(v));
            }
            m.insert("fields".to_string(), Value::Object(obj).ref_cell());
        }
        Expr::ClassInit { name, fields, .. } => {
            m.insert("kind".to_string(), str_val("ClassInit"));
            m.insert("name".to_string(), str_val(name));
            let mut obj = HashMap::new();
            for (k, v) in fields {
                obj.insert(k.clone(), expr_val(v));
            }
            m.insert("fields".to_string(), Value::Object(obj).ref_cell());
        }
        Expr::SuperCall { method, args, .. } => {
            m.insert("kind".to_string(), str_val("SuperCall"));
            m.insert("method".to_string(), str_val(method));
            m.insert(
                "args".to_string(),
                Value::Array(args.iter().map(expr_val).collect()).ref_cell(),
            );
        }
    }
    Value::Object(m).ref_cell()
}

fn stmt_val(s: &Stmt) -> ValueRef {
    let mut m = HashMap::new();
    let span = match s {
        Stmt::VarDecl { span, .. }
        | Stmt::Assign { span, .. }
        | Stmt::If { span, .. }
        | Stmt::While { span, .. }
        | Stmt::For { span, .. }
        | Stmt::Return { span, .. }
        | Stmt::Try { span, .. }
        | Stmt::Throw { span, .. } => *span,
        Stmt::Break(s) | Stmt::Continue(s) => *s,
        Stmt::Expr(e) => e.span(),
    };
    m.insert("span".to_string(), span_obj(span));
    match s {
        Stmt::VarDecl { name, ty, init, .. } => {
            m.insert("kind".to_string(), str_val("VarDecl"));
            m.insert("name".to_string(), str_val(name));
            if let Some(t) = ty {
                m.insert("type".to_string(), type_name_val(t));
            }
            if let Some(e) = init {
                m.insert("init".to_string(), expr_val(e));
            }
        }
        Stmt::Assign {
            target, op, value, ..
        } => {
            m.insert("kind".to_string(), str_val("Assign"));
            m.insert(
                "op".to_string(),
                str_val(match op {
                    AssignOp::Assign => "Assign",
                    AssignOp::AddAssign => "AddAssign",
                    AssignOp::SubAssign => "SubAssign",
                }),
            );
            m.insert("value".to_string(), expr_val(value));
            match target {
                AssignTarget::Name(n) => {
                    m.insert("target".to_string(), str_val(n));
                }
                AssignTarget::Member { object, field } => {
                    let mut t = HashMap::new();
                    t.insert("kind".to_string(), str_val("Member"));
                    t.insert("object".to_string(), expr_val(object));
                    t.insert("field".to_string(), str_val(field));
                    m.insert("target".to_string(), Value::Object(t).ref_cell());
                }
                AssignTarget::Index { object, index } => {
                    let mut t = HashMap::new();
                    t.insert("kind".to_string(), str_val("Index"));
                    t.insert("object".to_string(), expr_val(object));
                    t.insert("index".to_string(), expr_val(index));
                    m.insert("target".to_string(), Value::Object(t).ref_cell());
                }
            }
        }
        Stmt::If {
            cond,
            then_block,
            else_block,
            ..
        } => {
            m.insert("kind".to_string(), str_val("If"));
            m.insert("cond".to_string(), expr_val(cond));
            m.insert("then".to_string(), block_val(then_block));
            if let Some(eb) = else_block {
                m.insert("else".to_string(), block_val(eb));
            }
        }
        Stmt::While { cond, body, .. } => {
            m.insert("kind".to_string(), str_val("While"));
            m.insert("cond".to_string(), expr_val(cond));
            m.insert("body".to_string(), block_val(body));
        }
        Stmt::For {
            var, iter, body, ..
        } => {
            m.insert("kind".to_string(), str_val("For"));
            m.insert("var".to_string(), str_val(var));
            m.insert("iter".to_string(), expr_val(iter));
            m.insert("body".to_string(), block_val(body));
        }
        Stmt::Return { value, .. } => {
            m.insert("kind".to_string(), str_val("Return"));
            if let Some(v) = value {
                m.insert("value".to_string(), expr_val(v));
            }
        }
        Stmt::Try {
            try_block,
            catch_var,
            catch_block,
            ..
        } => {
            m.insert("kind".to_string(), str_val("Try"));
            m.insert("try".to_string(), block_val(try_block));
            m.insert("catch_var".to_string(), str_val(catch_var));
            m.insert("catch".to_string(), block_val(catch_block));
        }
        Stmt::Throw { value, .. } => {
            m.insert("kind".to_string(), str_val("Throw"));
            m.insert("value".to_string(), expr_val(value));
        }
        Stmt::Break(_) => {
            m.insert("kind".to_string(), str_val("Break"));
        }
        Stmt::Continue(_) => {
            m.insert("kind".to_string(), str_val("Continue"));
        }
        Stmt::Expr(e) => {
            m.insert("kind".to_string(), str_val("Expr"));
            m.insert("expr".to_string(), expr_val(e));
        }
    }
    Value::Object(m).ref_cell()
}

fn block_val(b: &Block) -> ValueRef {
    let mut m = HashMap::new();
    m.insert("kind".to_string(), str_val("Block"));
    m.insert("span".to_string(), span_obj(b.span));
    m.insert(
        "stmts".to_string(),
        Value::Array(b.stmts.iter().map(stmt_val).collect()).ref_cell(),
    );
    Value::Object(m).ref_cell()
}

fn fn_def_val(f: &FnDef) -> ValueRef {
    let mut m = HashMap::new();
    m.insert("kind".to_string(), str_val("Fn"));
    m.insert("span".to_string(), span_obj(f.span));
    m.insert("name".to_string(), str_val(&f.name));
    let params: Vec<ValueRef> = f
        .params
        .iter()
        .map(|p| {
            let mut pm = HashMap::new();
            pm.insert("name".to_string(), str_val(&p.name));
            if let Some(t) = &p.ty {
                pm.insert("type".to_string(), type_name_val(t));
            }
            pm.insert("span".to_string(), span_obj(p.span));
            Value::Object(pm).ref_cell()
        })
        .collect();
    m.insert("params".to_string(), Value::Array(params).ref_cell());
    if let Some(rt) = &f.return_type {
        m.insert("return_type".to_string(), type_name_val(rt));
    }
    m.insert("body".to_string(), block_val(&f.body));
    Value::Object(m).ref_cell()
}

fn top_level_val(item: &TopLevel) -> ValueRef {
    match item {
        TopLevel::Import(i) => {
            let mut m = HashMap::new();
            m.insert("kind".to_string(), str_val("Import"));
            m.insert("span".to_string(), span_obj(i.span));
            m.insert("path".to_string(), str_val(&i.path));
            if let Some(a) = &i.alias {
                m.insert("alias".to_string(), str_val(a));
            }
            Value::Object(m).ref_cell()
        }
        TopLevel::Fn(f) => fn_def_val(f),
        TopLevel::Struct(s) => {
            let mut m = HashMap::new();
            m.insert("kind".to_string(), str_val("Struct"));
            m.insert("span".to_string(), span_obj(s.span));
            m.insert("name".to_string(), str_val(&s.name));
            let fields: Vec<ValueRef> = s
                .fields
                .iter()
                .map(|f| {
                    let mut fm = HashMap::new();
                    fm.insert("name".to_string(), str_val(&f.name));
                    fm.insert("type".to_string(), type_name_val(&f.ty));
                    fm.insert("span".to_string(), span_obj(f.span));
                    Value::Object(fm).ref_cell()
                })
                .collect();
            m.insert("fields".to_string(), Value::Array(fields).ref_cell());
            Value::Object(m).ref_cell()
        }
        TopLevel::Class(c) => {
            let mut m = HashMap::new();
            m.insert("kind".to_string(), str_val("Class"));
            m.insert("span".to_string(), span_obj(c.span));
            m.insert("name".to_string(), str_val(&c.name));
            Value::Object(m).ref_cell()
        }
        TopLevel::Trait(t) => {
            let mut m = HashMap::new();
            m.insert("kind".to_string(), str_val("Trait"));
            m.insert("span".to_string(), span_obj(t.span));
            m.insert("name".to_string(), str_val(&t.name));
            Value::Object(m).ref_cell()
        }
        TopLevel::Server(_) => {
            let mut m = HashMap::new();
            m.insert("kind".to_string(), str_val("Server"));
            Value::Object(m).ref_cell()
        }
        TopLevel::Route(r) => {
            let mut m = HashMap::new();
            m.insert("kind".to_string(), str_val("Route"));
            m.insert("span".to_string(), span_obj(r.span));
            m.insert("path".to_string(), str_val(&r.path));
            Value::Object(m).ref_cell()
        }
        TopLevel::Stmt(s) => stmt_val(s),
    }
}

fn program_val(p: &Program) -> ValueRef {
    let mut m = HashMap::new();
    m.insert("kind".to_string(), str_val("Program"));
    m.insert("span".to_string(), span_obj(p.span));
    m.insert(
        "items".to_string(),
        Value::Array(p.items.iter().map(top_level_val).collect()).ref_cell(),
    );
    Value::Object(m).ref_cell()
}

// ---------------------------------------------------------------------------
// Lint engine
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Issue {
    rule: String,
    message: String,
    line: usize,
    col: usize,
    severity: String,
}

fn issue_obj(i: &Issue) -> ValueRef {
    let mut m = HashMap::new();
    m.insert("rule".to_string(), str_val(&i.rule));
    m.insert("message".to_string(), str_val(&i.message));
    m.insert("line".to_string(), Value::Int(i.line as i64).ref_cell());
    m.insert("col".to_string(), Value::Int(i.col as i64).ref_cell());
    m.insert("severity".to_string(), str_val(&i.severity));
    Value::Object(m).ref_cell()
}

fn callee_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Ident(name, _) => Some(name.as_str()),
        Expr::Member { field, .. } => Some(field.as_str()),
        _ => None,
    }
}

fn walk_exprs(expr: &Expr, f: &mut impl FnMut(&Expr)) {
    f(expr);
    match expr {
        Expr::Binary { left, right, .. } => {
            walk_exprs(left, f);
            walk_exprs(right, f);
        }
        Expr::Unary { expr, .. } => walk_exprs(expr, f),
        Expr::Call { callee, args, .. } => {
            walk_exprs(callee, f);
            for a in args {
                walk_exprs(a, f);
            }
        }
        Expr::Member { object, .. } => walk_exprs(object, f),
        Expr::Index { object, index, .. } => {
            walk_exprs(object, f);
            walk_exprs(index, f);
        }
        Expr::Array { elements, .. } => {
            for e in elements {
                walk_exprs(e, f);
            }
        }
        Expr::Object { fields, .. }
        | Expr::StructInit { fields, .. }
        | Expr::ClassInit { fields, .. } => {
            for (_, e) in fields {
                walk_exprs(e, f);
            }
        }
        Expr::SuperCall { args, .. } => {
            for a in args {
                walk_exprs(a, f);
            }
        }
        _ => {}
    }
}

fn walk_stmts(stmts: &[Stmt], f: &mut impl FnMut(&Stmt)) {
    for s in stmts {
        f(s);
        match s {
            Stmt::If {
                then_block,
                else_block,
                ..
            } => {
                walk_stmts(&then_block.stmts, f);
                if let Some(eb) = else_block {
                    walk_stmts(&eb.stmts, f);
                }
            }
            Stmt::While { body, .. } | Stmt::For { body, .. } => walk_stmts(&body.stmts, f),
            Stmt::Try {
                try_block,
                catch_block,
                ..
            } => {
                walk_stmts(&try_block.stmts, f);
                walk_stmts(&catch_block.stmts, f);
            }
            _ => {}
        }
    }
}

fn apply_builtin_rules(program: &Program, issues: &mut Vec<Issue>) {
    let mut has_main = false;
    for item in &program.items {
        if let TopLevel::Fn(f) = item {
            if f.name == "main" {
                has_main = true;
            }
            if f.body.stmts.is_empty() {
                issues.push(Issue {
                    rule: "no-empty-fn".into(),
                    message: format!("function '{}' has an empty body", f.name),
                    line: f.span.line,
                    col: f.span.col,
                    severity: "warn".into(),
                });
            }
            walk_stmts(&f.body.stmts, &mut |stmt| {
                if let Stmt::Expr(Expr::Call { callee, span, .. }) = stmt {
                    if callee_name(callee) == Some("print") {
                        issues.push(Issue {
                            rule: "no-print".into(),
                            message: format!("print() in function '{}'", f.name),
                            line: span.line,
                            col: span.col,
                            severity: "warn".into(),
                        });
                    }
                }
            });
        }
        if let TopLevel::Stmt(Stmt::Expr(Expr::Call { callee, span, .. })) = item {
            if callee_name(callee) == Some("print") {
                issues.push(Issue {
                    rule: "no-top-level-print".into(),
                    message: "top-level print() call".into(),
                    line: span.line,
                    col: span.col,
                    severity: "warn".into(),
                });
            }
        }
    }
    if !has_main {
        issues.push(Issue {
            rule: "require-main".into(),
            message: "no fn main() entry point".into(),
            line: program.span.line,
            col: program.span.col,
            severity: "warn".into(),
        });
    }
}

fn rule_str(rule: &HashMap<String, ValueRef>, key: &str) -> Option<String> {
    rule.get(key).and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.clone()),
        _ => None,
    })
}

fn apply_data_rule(program: &Program, rule: &HashMap<String, ValueRef>, issues: &mut Vec<Issue>) {
    let id = rule_str(rule, "id").unwrap_or_else(|| "rule".into());
    let on = rule_str(rule, "on");
    let check = rule_str(rule, "check");
    let callee = rule_str(rule, "callee");
    let severity = rule_str(rule, "severity").unwrap_or_else(|| "warn".into());

    if on.as_deref() == Some("Fn") && check.as_deref() == Some("empty_body") {
        for item in &program.items {
            if let TopLevel::Fn(f) = item {
                if f.body.stmts.is_empty() {
                    issues.push(Issue {
                        rule: id.clone(),
                        message: format!("function '{}' has an empty body", f.name),
                        line: f.span.line,
                        col: f.span.col,
                        severity: severity.clone(),
                    });
                }
            }
        }
        return;
    }

    if on.as_deref() == Some("Call") {
        let target = callee.as_deref().unwrap_or("print");
        let mut visit = |expr: &Expr| {
            if let Expr::Call {
                callee: c, span, ..
            } = expr
            {
                if callee_name(c) == Some(target) {
                    issues.push(Issue {
                        rule: id.clone(),
                        message: format!("call to {target}()"),
                        line: span.line,
                        col: span.col,
                        severity: severity.clone(),
                    });
                }
            }
        };
        for item in &program.items {
            match item {
                TopLevel::Fn(f) => walk_stmts(&f.body.stmts, &mut |stmt| {
                    if let Stmt::Expr(e) = stmt {
                        walk_exprs(e, &mut visit);
                    }
                }),
                TopLevel::Stmt(Stmt::Expr(e)) => walk_exprs(e, &mut visit),
                _ => {}
            }
        }
    }
}

fn apply_custom_rule(
    program: &Program,
    rule: &HashMap<String, ValueRef>,
    span: Span,
    issues: &mut Vec<Issue>,
) -> NiaoResult<()> {
    let id = rule_str(rule, "id").unwrap_or_else(|| "custom".into());
    let on = rule_str(rule, "on").unwrap_or_else(|| "Program".into());
    let severity = rule_str(rule, "severity").unwrap_or_else(|| "warn".into());
    let func = match rule.get("fn") {
        Some(f) => Rc::clone(f),
        None => return Ok(()),
    };
    if !matches!(
        &*func.borrow(),
        Value::Function(_) | Value::NativeFunction(_)
    ) {
        return Err(type_err(span, "custom rule 'fn' must be a function"));
    }

    let nodes: Vec<ValueRef> = match on.as_str() {
        "Program" => vec![program_val(program)],
        "Fn" => program
            .items
            .iter()
            .filter_map(|i| {
                if let TopLevel::Fn(f) = i {
                    Some(fn_def_val(f))
                } else {
                    None
                }
            })
            .collect(),
        "Item" => program.items.iter().map(top_level_val).collect(),
        other => {
            return Err(type_err(
                span,
                format!("custom rule 'on' must be Program, Fn, or Item, got '{other}'"),
            ));
        }
    };

    for node in nodes {
        let result = call_niao_function(func.clone(), &[Rc::clone(&node)], span)?;
        let flagged = match &*result.borrow() {
            Value::Bool(b) => *b,
            Value::String(msg) if !msg.is_empty() => {
                let line = rule_str(rule, "line")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1);
                let col = rule_str(rule, "col")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1);
                issues.push(Issue {
                    rule: id.clone(),
                    message: msg.clone(),
                    line,
                    col,
                    severity: severity.clone(),
                });
                false
            }
            Value::Error(e) => {
                issues.push(Issue {
                    rule: id.clone(),
                    message: e.message.clone(),
                    line: e.line,
                    col: e.col,
                    severity: severity.clone(),
                });
                false
            }
            _ => false,
        };
        if flagged {
            let (line, col) = match &*node.borrow() {
                Value::Object(m) => {
                    let line = m
                        .get("span")
                        .and_then(|s| match &*s.borrow() {
                            Value::Object(sm) => sm.get("line").map(|l| match &*l.borrow() {
                                Value::Int(n) => *n as usize,
                                _ => 1,
                            }),
                            _ => None,
                        })
                        .unwrap_or(1);
                    let col = m
                        .get("span")
                        .and_then(|s| match &*s.borrow() {
                            Value::Object(sm) => sm.get("col").map(|c| match &*c.borrow() {
                                Value::Int(n) => *n as usize,
                                _ => 1,
                            }),
                            _ => None,
                        })
                        .unwrap_or(1);
                    (line, col)
                }
                _ => (1, 1),
            };
            issues.push(Issue {
                rule: id.clone(),
                message: format!("custom rule '{id}' flagged a node"),
                line,
                col,
                severity: severity.clone(),
            });
        }
    }
    Ok(())
}

fn parse_rules(val: &ValueRef, span: Span) -> NiaoResult<Vec<HashMap<String, ValueRef>>> {
    match &*val.borrow() {
        Value::Array(items) => {
            let mut out = Vec::new();
            for item in items {
                match &*item.borrow() {
                    Value::Object(m) => out.push(m.clone()),
                    other => {
                        return Err(type_err(
                            span,
                            format!("each rule must be an object, got {}", other.type_name()),
                        ));
                    }
                }
            }
            Ok(out)
        }
        other => Err(type_err(
            span,
            format!("rules must be an array, got {}", other.type_name()),
        )),
    }
}

fn default_rules() -> Vec<HashMap<String, ValueRef>> {
    vec![
        rule_obj("no-empty-fn", "Fn", Some("empty_body"), None),
        rule_obj("no-print", "Call", None, Some("print")),
        rule_obj("require-main", "Program", Some("missing_main"), None),
    ]
}

fn rule_obj(
    id: &str,
    on: &str,
    check: Option<&str>,
    callee: Option<&str>,
) -> HashMap<String, ValueRef> {
    let mut m = HashMap::new();
    m.insert("id".to_string(), str_val(id));
    m.insert("on".to_string(), str_val(on));
    if let Some(c) = check {
        m.insert("check".to_string(), str_val(c));
    }
    if let Some(c) = callee {
        m.insert("callee".to_string(), str_val(c));
    }
    m.insert("severity".to_string(), str_val("warn"));
    m
}

fn run_lint(
    program: &Program,
    rules: &[HashMap<String, ValueRef>],
    span: Span,
) -> NiaoResult<ValueRef> {
    let mut issues = Vec::new();
    apply_builtin_rules(program, &mut issues);

    for rule in rules {
        if rule.contains_key("fn") {
            apply_custom_rule(program, rule, span, &mut issues)?;
        } else if rule_str(rule, "check").as_deref() == Some("missing_main") {
            let has_main = program
                .items
                .iter()
                .any(|i| matches!(i, TopLevel::Fn(f) if f.name == "main"));
            if !has_main {
                let id = rule_str(rule, "id").unwrap_or_else(|| "require-main".into());
                issues.push(Issue {
                    rule: id,
                    message: "no fn main() entry point".into(),
                    line: program.span.line,
                    col: program.span.col,
                    severity: rule_str(rule, "severity").unwrap_or_else(|| "warn".into()),
                });
            }
        } else {
            apply_data_rule(program, rule, &mut issues);
        }
    }

    // Deduplicate by (rule, line, col, message)
    issues.sort_by(|a, b| (a.line, a.col, &a.rule).cmp(&(b.line, b.col, &b.rule)));
    issues.dedup_by(|a, b| {
        a.rule == b.rule && a.line == b.line && a.col == b.col && a.message == b.message
    });

    let mut out = HashMap::new();
    out.insert("ok".to_string(), Value::Bool(issues.is_empty()).ref_cell());
    out.insert(
        "issues".to_string(),
        Value::Array(issues.iter().map(issue_obj).collect()).ref_cell(),
    );
    Ok(Value::Object(out).ref_cell())
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn nlint_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nlint_parse", span)?;
    let source = string_arg(args, 0, "nlint_parse", span)?;
    match parse_source(&source, span) {
        Ok(program) => Ok(program_val(&program)),
        Err(e) => Ok(e),
    }
}

fn nlint_check(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nlint_check", span)?;
    let source = string_arg(args, 0, "nlint_check", span)?;
    let program = match parse_source(&source, span) {
        Ok(p) => p,
        Err(e) => return Ok(e),
    };
    let rules = if args.len() == 2 {
        parse_rules(&args[1], span)?
    } else {
        default_rules()
    };
    run_lint(&program, &rules, span)
}

fn nlint_rules(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nlint_rules", span)?;
    let rules = default_rules();
    Ok(Value::Array(
        rules
            .into_iter()
            .map(Value::Object)
            .map(|o| o.ref_cell())
            .collect(),
    )
    .ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nlint_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nlint_fns![
    ("nlint_parse", "parse", nlint_parse),
    ("nlint_check", "check", nlint_check),
    ("nlint_rules", "rules", nlint_rules),
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

pub const MODULE_NAME: &str = "nlint";
pub const MODULE_PATHS: &[&str] = &["nlint", "std/nlint"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::dummy()
    }

    fn s(v: &str) -> ValueRef {
        Value::String(v.to_string()).ref_cell()
    }

    #[test]
    fn parse_returns_ast_data() {
        let src = r#"fn add(a: int, b: int) -> int { return a + b }"#;
        let v = nlint_parse(&[s(src)], span()).unwrap();
        let v_ref = v.borrow();
        match &*v_ref {
            Value::Object(m) => {
                let kind_ref = m.get("kind").unwrap().borrow();
                assert!(matches!(&*kind_ref, Value::String(k) if k == "Program"));
                let items_ref = m.get("items").unwrap().borrow();
                match &*items_ref {
                    Value::Array(arr) => assert_eq!(arr.len(), 1),
                    other => panic!("expected items array, got {other:?}"),
                }
            }
            Value::Error(_) => panic!("unexpected parse error"),
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn check_flags_empty_fn_and_missing_main() {
        let src = "fn empty() { }";
        let r = nlint_check(&[s(src)], span()).unwrap();
        let r_ref = r.borrow();
        match &*r_ref {
            Value::Object(m) => {
                let ok_ref = m.get("ok").unwrap().borrow();
                assert!(matches!(&*ok_ref, Value::Bool(false)));
                let issues_ref = m.get("issues").unwrap().borrow();
                match &*issues_ref {
                    Value::Array(arr) => assert!(!arr.is_empty()),
                    other => panic!("expected issues, got {other:?}"),
                }
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn check_clean_source_passes() {
        let src = r#"fn main() { let x = 1 + 2 }"#;
        let r = nlint_check(&[s(src)], span()).unwrap();
        let r_ref = r.borrow();
        match &*r_ref {
            Value::Object(m) => {
                let ok_ref = m.get("ok").unwrap().borrow();
                assert!(matches!(&*ok_ref, Value::Bool(true)));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn parse_error_is_error_value() {
        let v = nlint_parse(&[s("fn {")], span()).unwrap();
        let v_ref = v.borrow();
        assert!(matches!(&*v_ref, Value::Error(_)));
    }
}
