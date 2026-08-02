//! Native ndoc standard library — extract and run doc-comment doctests
//! (`// >>>` code, `// =>` expected) from Niao source.
//!
//! Import with `import "ndoc"` (or `import "std/ndoc"`).

use crate::{
    apply_binop, builtin_environment, error_value, native_module_export_name, values_equal,
    NativeFn, NiaoResult, RuntimeError, Value, ValueRef,
};
use niao_ast::*;
use niao_parser::{parse, ParseError};
use std::collections::HashMap;
use std::rc::Rc;

const E3210_NDOC_ARITY: u32 = 3210;
const E3211_NDOC_ERROR: u32 = 3211;
const E3212_NDOC_TYPE: u32 = 3212;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3210_NDOC_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3212_NDOC_TYPE, msg.into())
}

fn doc_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3211_NDOC_ERROR, "ndoc_error", msg.into(), span)
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
// Doctest extraction
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct DocExample {
    line: usize,
    code: Vec<String>,
    expect: Option<String>,
}

fn strip_doctest_prefix(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_start();
    let (marker, rest) = if let Some(r) = trimmed.strip_prefix("// >>>") {
        (">>>", r)
    } else if let Some(r) = trimmed.strip_prefix("/// >>>") {
        (">>>", r)
    } else if let Some(r) = trimmed.strip_prefix("// =>") {
        ("=>", r)
    } else if let Some(r) = trimmed.strip_prefix("/// =>") {
        ("=>", r)
    } else {
        return None;
    };
    let line_no = line.len() - trimmed.len();
    let _ = line_no;
    let content = rest.trim_start();
    Some((if marker == ">>>" { 0 } else { 1 }, content))
}

fn extract_examples(source: &str) -> Vec<DocExample> {
    let mut examples = Vec::new();
    let mut current: Option<DocExample> = None;

    for (idx, line) in source.lines().enumerate() {
        let line_no = idx + 1;
        if let Some((kind, content)) = strip_doctest_prefix(line) {
            if kind == 0 {
                if let Some(prev) = current.take() {
                    examples.push(prev);
                }
                current = Some(DocExample {
                    line: line_no,
                    code: vec![content.to_string()],
                    expect: None,
                });
            } else if let Some(ref mut ex) = current {
                ex.expect = Some(content.to_string());
            }
        } else if let Some(ref mut ex) = current {
            // Continuation line inside a doctest block (indented code after >>>)
            if !content_is_separator(line) && !line.trim().is_empty() {
                let trimmed = line.trim_start();
                if let Some(cont) = trimmed.strip_prefix("//").map(|r| r.trim_start()) {
                    if !cont.starts_with(">>>") && !cont.starts_with("=>") {
                        ex.code.push(cont.to_string());
                    }
                }
            }
        }
    }
    if let Some(prev) = current {
        examples.push(prev);
    }
    examples
}

fn content_is_separator(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("// >>>")
        || t.starts_with("/// >>>")
        || t.starts_with("// =>")
        || t.starts_with("/// =>")
}

fn example_obj(ex: &DocExample) -> ValueRef {
    let mut m = HashMap::new();
    m.insert("line".to_string(), Value::Int(ex.line as i64).ref_cell());
    m.insert(
        "code".to_string(),
        Value::Array(
            ex.code
                .iter()
                .map(|c| Value::String(c.clone()).ref_cell())
                .collect(),
        )
        .ref_cell(),
    );
    if let Some(e) = &ex.expect {
        m.insert("expect".to_string(), Value::String(e.clone()).ref_cell());
    }
    Value::Object(m).ref_cell()
}

// ---------------------------------------------------------------------------
// Mini evaluator for doctest snippets
// ---------------------------------------------------------------------------

struct DocEnv {
    vars: HashMap<String, ValueRef>,
}

impl DocEnv {
    fn new() -> Self {
        Self {
            vars: HashMap::new(),
        }
    }

    fn resolve_name(&self, name: &str) -> Option<ValueRef> {
        self.vars
            .get(name)
            .cloned()
            .or_else(|| builtin_environment().get(name))
    }

    fn import_module(&mut self, path: &str) {
        let path = path.trim_matches('"');
        if let Some(export) = native_module_export_name(path) {
            if let Some(val) = builtin_environment().get(export) {
                self.vars.insert(export.to_string(), Rc::clone(&val));
            }
        }
    }
}

fn eval_expr(expr: &Expr, env: &DocEnv, span: Span) -> NiaoResult<Value> {
    match expr {
        Expr::Int(v, _) => Ok(Value::Int(*v)),
        Expr::Float(v, _) => Ok(Value::Float(*v)),
        Expr::String(v, _) => Ok(Value::String(v.clone())),
        Expr::Bool(v, _) => Ok(Value::Bool(*v)),
        Expr::Nil(_) => Ok(Value::Nil),
        Expr::Ident(name, s) => env
            .vars
            .get(name)
            .map(|v| v.borrow().clone())
            .ok_or_else(|| {
                RuntimeError::at(*s, E3211_NDOC_ERROR, format!("undefined variable '{name}'"))
            }),
        Expr::Binary {
            left,
            op,
            right,
            span: s,
        } => {
            let l = eval_expr(left, env, span)?;
            let r = eval_expr(right, env, span)?;
            apply_binop(*op, &l, &r, *s)
        }
        Expr::Unary { op, expr, span: s } => {
            let v = eval_expr(expr, env, span)?;
            match op {
                UnaryOp::Not => Ok(Value::Bool(!v.is_truthy())),
                UnaryOp::Neg => match v {
                    Value::Int(n) => Ok(Value::Int(-n)),
                    Value::Float(f) => Ok(Value::Float(-f)),
                    other => Err(RuntimeError::at(
                        *s,
                        E3212_NDOC_TYPE,
                        format!("cannot negate {}", other.type_name()),
                    )),
                },
            }
        }
        Expr::Array { elements, .. } => {
            let mut out = Vec::new();
            for e in elements {
                out.push(eval_expr(e, env, span)?.ref_cell());
            }
            Ok(Value::Array(out))
        }
        Expr::Object { fields, .. } => {
            let mut out = HashMap::new();
            for (k, e) in fields {
                out.insert(k.clone(), eval_expr(e, env, span)?.ref_cell());
            }
            Ok(Value::Object(out))
        }
        Expr::Member {
            object,
            field,
            span: s,
        } => {
            let val = eval_expr(object, env, span)?;
            match val {
                Value::Object(map) => map.get(field).map(|v| v.borrow().clone()).ok_or_else(|| {
                    RuntimeError::at(*s, E3211_NDOC_ERROR, format!("undefined field '{field}'"))
                }),
                other => Err(RuntimeError::at(
                    *s,
                    E3212_NDOC_TYPE,
                    format!("cannot access field on {}", other.type_name()),
                )),
            }
        }
        Expr::Call {
            callee,
            args,
            span: s,
        } => {
            let callee_val = match &**callee {
                Expr::Ident(name, _) => env.resolve_name(name).ok_or_else(|| {
                    RuntimeError::at(*s, E3211_NDOC_ERROR, format!("undefined '{name}'"))
                })?,
                _ => eval_expr(callee, env, span)?.ref_cell(),
            };
            let arg_refs: Vec<ValueRef> = args
                .iter()
                .map(|a| eval_expr(a, env, span).map(|v| v.ref_cell()))
                .collect::<Result<_, _>>()?;
            let native = match &*callee_val.borrow() {
                Value::NativeFunction(f) => Rc::clone(f),
                other => {
                    return Err(RuntimeError::at(
                        *s,
                        E3212_NDOC_TYPE,
                        format!("cannot call {}", other.type_name()),
                    ))
                }
            };
            native(&arg_refs, *s).map(|v| v.borrow().clone())
        }
        other => Err(RuntimeError::at(
            other.span(),
            E3212_NDOC_TYPE,
            format!("doctest expression not supported: {:?}", other),
        )),
    }
}

fn eval_stmt(stmt: &Stmt, env: &mut DocEnv, span: Span) -> NiaoResult<ValueRef> {
    match stmt {
        Stmt::VarDecl { name, init, .. } => {
            let val = if let Some(e) = init {
                eval_expr(e, env, span)?.ref_cell()
            } else {
                Value::Nil.ref_cell()
            };
            env.vars.insert(name.clone(), val.clone());
            Ok(val)
        }
        Stmt::Assign {
            target: AssignTarget::Name(n),
            value,
            ..
        } => {
            let val = eval_expr(value, env, span)?.ref_cell();
            env.vars.insert(n.clone(), Rc::clone(&val));
            Ok(val)
        }
        Stmt::Return { value, .. } => {
            if let Some(e) = value {
                Ok(eval_expr(e, env, span)?.ref_cell())
            } else {
                Ok(Value::Nil.ref_cell())
            }
        }
        Stmt::Expr(e) => Ok(eval_expr(e, env, span)?.ref_cell()),
        other => Err(RuntimeError::at(
            span,
            E3212_NDOC_TYPE,
            format!("doctest statement not supported: {other:?}"),
        )),
    }
}

fn parse_snippet_program(snippet: &str, _span: Span) -> Result<Program, String> {
    let wrapped = format!("{snippet}");
    parse(&wrapped).map_err(|e| format_parse_error(&e))
}

fn run_snippets(snippets: &[String], span: Span) -> Result<ValueRef, String> {
    let mut env = DocEnv::new();
    let mut last = Value::Nil.ref_cell();
    for snippet in snippets {
        let program = parse_snippet_program(snippet, span)?;
        for item in &program.items {
            match item {
                TopLevel::Import(imp) => {
                    env.import_module(&imp.path);
                }
                TopLevel::Stmt(stmt) => {
                    last = eval_stmt(stmt, &mut env, span).map_err(|e| e.to_string())?;
                }
                TopLevel::Fn(f) => {
                    for stmt in &f.body.stmts {
                        last = eval_stmt(stmt, &mut env, span).map_err(|e| e.to_string())?;
                    }
                }
                _ => {}
            }
        }
    }
    Ok(last)
}

fn parse_expect_expr(expect: &str, span: Span) -> Result<Value, String> {
    let program = parse(expect).map_err(|e| format_parse_error(&e))?;
    let expr = match program.items.first() {
        Some(TopLevel::Stmt(Stmt::Expr(e))) => e,
        Some(TopLevel::Fn(f)) => f
            .body
            .stmts
            .first()
            .and_then(|s| match s {
                Stmt::Return { value: Some(e), .. } => Some(e),
                Stmt::Expr(e) => Some(e),
                _ => None,
            })
            .ok_or_else(|| "expected expression in => value".to_string())?,
        _ => return Err("expected expression in => value".into()),
    };
    eval_expr(expr, &DocEnv::new(), span).map_err(|e| e.to_string())
}

fn run_examples(examples: &[DocExample], span: Span) -> ValueRef {
    let mut results = Vec::new();
    let mut passed = 0i64;
    let mut failed = 0i64;

    for ex in examples {
        let mut block = HashMap::new();
        block.insert("line".to_string(), Value::Int(ex.line as i64).ref_cell());
        block.insert(
            "code".to_string(),
            Value::String(ex.code.join("\n")).ref_cell(),
        );

        match run_snippets(&ex.code, span) {
            Ok(got) => {
                block.insert("got".to_string(), Rc::clone(&got));
                if let Some(expect_src) = &ex.expect {
                    match parse_expect_expr(expect_src, span) {
                        Ok(expected) => {
                            let ok = values_equal(&got.borrow(), &expected);
                            block.insert("ok".to_string(), Value::Bool(ok).ref_cell());
                            block.insert(
                                "expect".to_string(),
                                Value::String(expect_src.clone()).ref_cell(),
                            );
                            if ok {
                                passed += 1;
                            } else {
                                failed += 1;
                                block.insert(
                                    "message".to_string(),
                                    Value::String(format!(
                                        "expected {}, got {}",
                                        expected.to_string(),
                                        got.borrow().to_string()
                                    ))
                                    .ref_cell(),
                                );
                            }
                        }
                        Err(msg) => {
                            failed += 1;
                            block.insert("ok".to_string(), Value::Bool(false).ref_cell());
                            block.insert("message".to_string(), Value::String(msg).ref_cell());
                        }
                    }
                } else {
                    block.insert("ok".to_string(), Value::Bool(true).ref_cell());
                    passed += 1;
                }
            }
            Err(msg) => {
                failed += 1;
                block.insert("ok".to_string(), Value::Bool(false).ref_cell());
                block.insert("message".to_string(), Value::String(msg).ref_cell());
            }
        }
        results.push(Value::Object(block).ref_cell());
    }

    let mut summary = HashMap::new();
    summary.insert(
        "total".to_string(),
        Value::Int(examples.len() as i64).ref_cell(),
    );
    summary.insert("passed".to_string(), Value::Int(passed).ref_cell());
    summary.insert("failed".to_string(), Value::Int(failed).ref_cell());
    summary.insert("ok".to_string(), Value::Bool(failed == 0).ref_cell());
    summary.insert("results".to_string(), Value::Array(results).ref_cell());
    Value::Object(summary).ref_cell()
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn ndoc_extract(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndoc_extract", span)?;
    let source = string_arg(args, 0, "ndoc_extract", span)?;
    let examples = extract_examples(&source);
    Ok(Value::Array(examples.iter().map(example_obj).collect()).ref_cell())
}

fn ndoc_run(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndoc_run", span)?;
    let source = string_arg(args, 0, "ndoc_run", span)?;
    let examples = extract_examples(&source);
    Ok(run_examples(&examples, span))
}

fn ndoc_check(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndoc_check", span)?;
    let source = string_arg(args, 0, "ndoc_check", span)?;
    let examples = extract_examples(&source);
    if examples.is_empty() {
        return Ok(doc_err(
            span,
            "no doctests found (use // >>> and optional // =>)",
        ));
    }
    let result = run_examples(&examples, span);
    let ok = match &*result.borrow() {
        Value::Object(m) => matches!(&*m.get("ok").unwrap().borrow(), Value::Bool(true)),
        _ => true,
    };
    if ok {
        Ok(result)
    } else {
        Ok(doc_err(span, "doctest(s) failed — see results"))
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ndoc_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ndoc_fns![
    ("ndoc_extract", "extract", ndoc_extract),
    ("ndoc_run", "run", ndoc_run),
    ("ndoc_check", "check", ndoc_check),
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

pub const MODULE_NAME: &str = "ndoc";
pub const MODULE_PATHS: &[&str] = &["ndoc", "std/ndoc"];

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
    fn extract_finds_examples() {
        let src = r#"
// >>> 1 + 2
// => 3
// >>> let x = 5
// >>> x * 2
// => 10
"#;
        let ex = ndoc_extract(&[s(src)], span()).unwrap();
        let ex_ref = ex.borrow();
        match &*ex_ref {
            Value::Array(items) => assert_eq!(items.len(), 2),
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[test]
    fn run_passes_arithmetic() {
        let src = "// >>> 2 + 3\n// => 5\n";
        let r = ndoc_run(&[s(src)], span()).unwrap();
        let r_ref = r.borrow();
        match &*r_ref {
            Value::Object(m) => {
                let ok_ref = m.get("ok").unwrap().borrow();
                assert!(matches!(&*ok_ref, Value::Bool(true)));
                let passed_ref = m.get("passed").unwrap().borrow();
                assert!(matches!(&*passed_ref, Value::Int(1)));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn run_fails_wrong_expect() {
        let src = "// >>> 2 + 2\n// => 5\n";
        let r = ndoc_run(&[s(src)], span()).unwrap();
        let r_ref = r.borrow();
        match &*r_ref {
            Value::Object(m) => {
                let ok_ref = m.get("ok").unwrap().borrow();
                assert!(matches!(&*ok_ref, Value::Bool(false)));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn let_binding_across_lines() {
        let src = "// >>> let n = 4\n// >>> n + 1\n// => 5\n";
        let r = ndoc_run(&[s(src)], span()).unwrap();
        let r_ref = r.borrow();
        match &*r_ref {
            Value::Object(m) => {
                let ok_ref = m.get("ok").unwrap().borrow();
                assert!(matches!(&*ok_ref, Value::Bool(true)));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }
}
