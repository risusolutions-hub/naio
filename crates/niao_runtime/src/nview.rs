//! Native nview standard library — Jinja-style templating: inheritance, blocks,
//! filters, autoescape, partials for HTML/text output (~jinja2 subset).
//! Distinct from ntemplate's LLM prompt templates.
//!
//! Import with `import "nview"` (or `import "std/nview"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_view::{
    batch_compiled, batch_render, escape, escape_attr, filters, render, render_file, unescape,
    valid, vars, CompiledTemplate, EscapeMode, ViewEnv, ViewError, ViewErrorKind, ViewOpts,
};
use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

const E4470: u32 = codes::E4470_NVIEW_ARITY;
const E4471: u32 = codes::E4471_NVIEW_ERROR;
const E4472: u32 = codes::E4472_NVIEW_TYPE;
const E4473: u32 = codes::E4473_NVIEW_HANDLE;
const E4474: u32 = codes::E4474_NVIEW_PARSE;

thread_local! {
    static TEMPLATES: RefCell<HashMap<i64, CompiledTemplate>> = RefCell::new(HashMap::new());
    static ENVS: RefCell<HashMap<i64, ViewEnv>> = RefCell::new(HashMap::new());
    static NEXT_TPL: RefCell<i64> = const { RefCell::new(1) };
    static NEXT_ENV: RefCell<i64> = const { RefCell::new(1) };
}

fn alloc_tpl(t: CompiledTemplate) -> i64 {
    let id = NEXT_TPL.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    TEMPLATES.with(|m| m.borrow_mut().insert(id, t));
    id
}

fn alloc_env(e: ViewEnv) -> i64 {
    let id = NEXT_ENV.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    ENVS.with(|m| m.borrow_mut().insert(id, e));
    id
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E4470,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E4470,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E4472, msg.into())
}

fn nview_err(span: Span, err: &ViewError) -> ValueRef {
    let code = match err.kind() {
        ViewErrorKind::Parse => E4474,
        ViewErrorKind::Invalid => E4473,
        _ => E4471,
    };
    error_value(code, "nview_error", err.message().to_string(), span)
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

fn value_to_json(v: &Value, span: Span) -> NiaoResult<JsonValue> {
    match v {
        Value::Nil => Ok(JsonValue::Null),
        Value::Bool(b) => Ok(JsonValue::Bool(*b)),
        Value::Int(n) => Ok(JsonValue::Number((*n).into())),
        Value::Float(f) => {
            if let Some(n) = JsonNumber::from_f64(*f) {
                Ok(JsonValue::Number(n))
            } else {
                Ok(JsonValue::Null)
            }
        }
        Value::String(s) => Ok(JsonValue::String(s.clone())),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(value_to_json(&*item.borrow(), span)?);
            }
            Ok(JsonValue::Array(out))
        }
        Value::Object(map) => {
            let mut out = JsonMap::new();
            for (k, vr) in map {
                out.insert(k.clone(), value_to_json(&*vr.borrow(), span)?);
            }
            Ok(JsonValue::Object(out))
        }
        Value::BigInt(n) => {
            let s = n.to_string();
            if let Ok(i) = s.parse::<i64>() {
                Ok(JsonValue::Number(i.into()))
            } else {
                Ok(JsonValue::String(s))
            }
        }
        other => Err(type_err(
            span,
            format!(
                "context values must be nil/bool/int/float/string/array/object, got {}",
                other.type_name()
            ),
        )),
    }
}

fn ctx_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<JsonValue> {
    match &*args[idx].borrow() {
        Value::Object(map) => {
            let mut out = JsonMap::new();
            for (k, vr) in map {
                out.insert(k.clone(), value_to_json(&*vr.borrow(), span)?);
            }
            Ok(JsonValue::Object(out))
        }
        Value::Nil => Ok(JsonValue::Object(JsonMap::new())),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a context object as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn truthy_val(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Int(n) => *n != 0,
        _ => false,
    }
}

fn parse_opts(args: &[ValueRef], idx: usize, span: Span) -> NiaoResult<ViewOpts> {
    if args.len() <= idx {
        return Ok(ViewOpts::default());
    }
    match &*args[idx].borrow() {
        Value::Nil => Ok(ViewOpts::default()),
        Value::Object(map) => {
            let mut opts = ViewOpts::default();
            if let Some(v) = map.get("autoescape") {
                opts.autoescape = match &*v.borrow() {
                    Value::Bool(false) => EscapeMode::None,
                    Value::Bool(true) => EscapeMode::Html,
                    Value::String(s) => match s.to_ascii_lowercase().as_str() {
                        "none" | "off" | "false" => EscapeMode::None,
                        "html" | "on" | "true" => EscapeMode::Html,
                        "auto" => EscapeMode::Auto,
                        other => {
                            return Err(type_err(
                                span,
                                format!("unknown autoescape mode '{other}'"),
                            ));
                        }
                    },
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "autoescape must be bool or string, got {}",
                                other.type_name()
                            ),
                        ));
                    }
                };
            }
            if let Some(v) = map.get("keep_trailing_newline") {
                opts.keep_trailing_newline = truthy_val(&*v.borrow());
            }
            if let Some(v) = map.get("trim_blocks") {
                opts.trim_blocks = truthy_val(&*v.borrow());
            }
            if let Some(v) = map.get("lstrip_blocks") {
                opts.lstrip_blocks = truthy_val(&*v.borrow());
            }
            Ok(opts)
        }
        other => Err(type_err(
            span,
            format!("expected options object, got {}", other.type_name()),
        )),
    }
}

fn str_val(s: impl Into<String>) -> NiaoResult<ValueRef> {
    Ok(Value::String(s.into()).ref_cell())
}

fn int_val(n: i64) -> NiaoResult<ValueRef> {
    Ok(Value::Int(n).ref_cell())
}

fn bool_val(b: bool) -> NiaoResult<ValueRef> {
    Ok(Value::Bool(b).ref_cell())
}

fn nil_val() -> NiaoResult<ValueRef> {
    Ok(Value::Nil.ref_cell())
}

fn string_array(items: Vec<String>) -> NiaoResult<ValueRef> {
    Ok(Value::Array(
        items
            .into_iter()
            .map(|s| Value::String(s).ref_cell())
            .collect(),
    )
    .ref_cell())
}

/// nview.render(source, ctx, opts?) — one-shot Jinja render.
// >>> import "nview"
// >>> nview.render("Hello {{ name }}!", {name: "Ada"}, {autoescape: false})
// => "Hello Ada!"
fn nview_render(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nview_render", span)?;
    let source = string_arg(args, 0, "nview_render", span)?;
    let ctx = ctx_arg(args, 1, "nview_render", span)?;
    let opts = parse_opts(args, 2, span)?;
    match render(&source, &ctx, &opts) {
        Ok(s) => str_val(s),
        Err(e) => Ok(nview_err(span, &e)),
    }
}

/// nview.compile(source, opts?) — compile → template handle.
// >>> import "nview"
// >>> let t = nview.compile("{{ x }}")
// >>> type(t)
fn nview_compile(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nview_compile", span)?;
    let source = string_arg(args, 0, "nview_compile", span)?;
    let opts = parse_opts(args, 1, span)?;
    match CompiledTemplate::compile(&source, &opts) {
        Ok(t) => int_val(alloc_tpl(t)),
        Err(e) => Ok(nview_err(span, &e)),
    }
}

/// nview.run(tpl, ctx) — render a compiled template handle.
// >>> import "nview"
// >>> let t = nview.compile("{{ n }}", {autoescape: false})
// >>> nview.run(t, {n: 7})
// => "7"
fn nview_run(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nview_run", span)?;
    let id = handle_arg(args, 0, "nview_run", span)?;
    let ctx = ctx_arg(args, 1, "nview_run", span)?;
    TEMPLATES.with(|m| {
        let m = m.borrow();
        let Some(t) = m.get(&id) else {
            return Ok(error_value(
                E4473,
                "nview_error",
                format!("invalid template handle {id}"),
                span,
            ));
        };
        match t.render(&ctx) {
            Ok(s) => str_val(s),
            Err(e) => Ok(nview_err(span, &e)),
        }
    })
}

/// nview.close(tpl) — free a compiled template handle.
// >>> import "nview"
// >>> let t = nview.compile("x")
// >>> nview.close(t)
// => true
fn nview_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nview_close", span)?;
    let id = handle_arg(args, 0, "nview_close", span)?;
    let removed = TEMPLATES.with(|m| m.borrow_mut().remove(&id).is_some());
    bool_val(removed)
}

/// nview.env(opts?) — create a multi-template environment handle.
// >>> import "nview"
// >>> let e = nview.env({autoescape: false})
// >>> type(e)
fn nview_env(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nview_env", span)?;
    let opts = parse_opts(args, 0, span)?;
    int_val(alloc_env(ViewEnv::new(opts)))
}

/// nview.add(env, name, source) — register a named template for extends/include.
// >>> import "nview"
// >>> let e = nview.env({autoescape: false})
// >>> nview.add(e, "p.html", "hi")
fn nview_add(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nview_add", span)?;
    let id = handle_arg(args, 0, "nview_add", span)?;
    let name = string_arg(args, 1, "nview_add", span)?;
    let source = string_arg(args, 2, "nview_add", span)?;
    ENVS.with(|m| {
        let mut m = m.borrow_mut();
        let Some(env) = m.get_mut(&id) else {
            return Ok(error_value(
                E4473,
                "nview_error",
                format!("invalid env handle {id}"),
                span,
            ));
        };
        match env.add(&name, &source) {
            Ok(()) => nil_val(),
            Err(e) => Ok(nview_err(span, &e)),
        }
    })
}

/// nview.has(env, name) — true when a named template is registered.
// >>> import "nview"
// >>> let e = nview.env()
// >>> nview.add(e, "a.html", "x")
// >>> nview.has(e, "a.html")
// => true
fn nview_has(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nview_has", span)?;
    let id = handle_arg(args, 0, "nview_has", span)?;
    let name = string_arg(args, 1, "nview_has", span)?;
    ENVS.with(|m| {
        let m = m.borrow();
        let Some(env) = m.get(&id) else {
            return Ok(error_value(
                E4473,
                "nview_error",
                format!("invalid env handle {id}"),
                span,
            ));
        };
        bool_val(env.has(&name))
    })
}

/// nview.names(env) — sorted list of registered template names.
// >>> import "nview"
// >>> let e = nview.env()
// >>> nview.add(e, "b.html", "b")
// >>> nview.add(e, "a.html", "a")
// >>> nview.names(e)
fn nview_names(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nview_names", span)?;
    let id = handle_arg(args, 0, "nview_names", span)?;
    ENVS.with(|m| {
        let m = m.borrow();
        let Some(env) = m.get(&id) else {
            return Ok(error_value(
                E4473,
                "nview_error",
                format!("invalid env handle {id}"),
                span,
            ));
        };
        string_array(env.names())
    })
}

/// nview.remove(env, name) — remove a named template; returns whether it existed.
// >>> import "nview"
// >>> let e = nview.env()
// >>> nview.add(e, "x.html", "x")
// >>> nview.remove(e, "x.html")
// => true
fn nview_remove(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nview_remove", span)?;
    let id = handle_arg(args, 0, "nview_remove", span)?;
    let name = string_arg(args, 1, "nview_remove", span)?;
    ENVS.with(|m| {
        let mut m = m.borrow_mut();
        let Some(env) = m.get_mut(&id) else {
            return Ok(error_value(
                E4473,
                "nview_error",
                format!("invalid env handle {id}"),
                span,
            ));
        };
        bool_val(env.remove(&name))
    })
}

/// nview.render_named(env, name, ctx) — render a registered template by name.
// >>> import "nview"
// >>> let e = nview.env({autoescape: false})
// >>> nview.add(e, "hi.html", "Hi {{ who }}")
// >>> nview.render_named(e, "hi.html", {who: "Ada"})
// => "Hi Ada"
fn nview_render_named(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nview_render_named", span)?;
    let id = handle_arg(args, 0, "nview_render_named", span)?;
    let name = string_arg(args, 1, "nview_render_named", span)?;
    let ctx = ctx_arg(args, 2, "nview_render_named", span)?;
    ENVS.with(|m| {
        let m = m.borrow();
        let Some(env) = m.get(&id) else {
            return Ok(error_value(
                E4473,
                "nview_error",
                format!("invalid env handle {id}"),
                span,
            ));
        };
        match env.render_named(&name, &ctx) {
            Ok(s) => str_val(s),
            Err(e) => Ok(nview_err(span, &e)),
        }
    })
}

/// nview.render_in(env, source, ctx) — render source with env templates available.
// >>> import "nview"
// >>> let e = nview.env({autoescape: false})
// >>> nview.add(e, "p.html", "P")
// >>> nview.render_in(e, "{% include \"p.html\" %}", {})
// => "P"
fn nview_render_in(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nview_render_in", span)?;
    let id = handle_arg(args, 0, "nview_render_in", span)?;
    let source = string_arg(args, 1, "nview_render_in", span)?;
    let ctx = ctx_arg(args, 2, "nview_render_in", span)?;
    ENVS.with(|m| {
        let m = m.borrow();
        let Some(env) = m.get(&id) else {
            return Ok(error_value(
                E4473,
                "nview_error",
                format!("invalid env handle {id}"),
                span,
            ));
        };
        match env.render_in(&source, &ctx) {
            Ok(s) => str_val(s),
            Err(e) => Ok(nview_err(span, &e)),
        }
    })
}

/// nview.env_close(env) — free an environment handle.
// >>> import "nview"
// >>> let e = nview.env()
// >>> nview.env_close(e)
// => true
fn nview_env_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nview_env_close", span)?;
    let id = handle_arg(args, 0, "nview_env_close", span)?;
    let removed = ENVS.with(|m| m.borrow_mut().remove(&id).is_some());
    bool_val(removed)
}

/// nview.render_file(path, ctx, opts?) — load a file and render it.
// >>> import "nview"
// >>> // see tests for filesystem cases
fn nview_render_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nview_render_file", span)?;
    let path = string_arg(args, 0, "nview_render_file", span)?;
    let ctx = ctx_arg(args, 1, "nview_render_file", span)?;
    let opts = parse_opts(args, 2, span)?;
    match render_file(PathBuf::from(path).as_path(), &ctx, &opts) {
        Ok(s) => str_val(s),
        Err(e) => Ok(nview_err(span, &e)),
    }
}

/// nview.add_file(env, name, path) — load a file into the environment.
fn nview_add_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nview_add_file", span)?;
    let id = handle_arg(args, 0, "nview_add_file", span)?;
    let name = string_arg(args, 1, "nview_add_file", span)?;
    let path = string_arg(args, 2, "nview_add_file", span)?;
    ENVS.with(|m| {
        let mut m = m.borrow_mut();
        let Some(env) = m.get_mut(&id) else {
            return Ok(error_value(
                E4473,
                "nview_error",
                format!("invalid env handle {id}"),
                span,
            ));
        };
        match env.add_file(&name, PathBuf::from(path).as_path()) {
            Ok(()) => nil_val(),
            Err(e) => Ok(nview_err(span, &e)),
        }
    })
}

/// nview.load_dir(env, dir) — load *.html/*.j2/*.jinja/*.txt from a directory.
fn nview_load_dir(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nview_load_dir", span)?;
    let id = handle_arg(args, 0, "nview_load_dir", span)?;
    let dir = string_arg(args, 1, "nview_load_dir", span)?;
    ENVS.with(|m| {
        let mut m = m.borrow_mut();
        let Some(env) = m.get_mut(&id) else {
            return Ok(error_value(
                E4473,
                "nview_error",
                format!("invalid env handle {id}"),
                span,
            ));
        };
        match env.load_dir(PathBuf::from(dir).as_path()) {
            Ok(n) => int_val(n as i64),
            Err(e) => Ok(nview_err(span, &e)),
        }
    })
}

/// nview.valid(source) — true when the template parses.
// >>> import "nview"
// >>> nview.valid("{{ x }}")
// => true
// >>> nview.valid("{% if %}")
// => false
fn nview_valid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nview_valid", span)?;
    let source = string_arg(args, 0, "nview_valid", span)?;
    bool_val(valid(&source))
}

/// nview.vars(source) — undeclared top-level variable names.
// >>> import "nview"
// >>> nview.vars("{{ a }} {{ b }}")
fn nview_vars(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nview_vars", span)?;
    let source = string_arg(args, 0, "nview_vars", span)?;
    match vars(&source) {
        Ok(v) => string_array(v),
        Err(e) => Ok(nview_err(span, &e)),
    }
}

/// nview.filters() — built-in filter names.
// >>> import "nview"
// >>> len(nview.filters()) > 10
// => true
fn nview_filters(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nview_filters", span)?;
    string_array(filters())
}

/// nview.escape(s) — HTML-escape a string.
// >>> import "nview"
// >>> nview.escape("a < b")
// => "a &lt; b"
fn nview_escape(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nview_escape", span)?;
    let s = string_arg(args, 0, "nview_escape", span)?;
    str_val(escape(&s))
}

/// nview.escape_attr(s) — HTML-escape for attribute values.
// >>> import "nview"
// >>> nview.escape_attr("x\"y")
fn nview_escape_attr(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nview_escape_attr", span)?;
    let s = string_arg(args, 0, "nview_escape_attr", span)?;
    str_val(escape_attr(&s))
}

/// nview.unescape(s) — decode HTML entities.
// >>> import "nview"
// >>> nview.unescape("a &lt; b")
// => "a < b"
fn nview_unescape(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nview_unescape", span)?;
    let s = string_arg(args, 0, "nview_unescape", span)?;
    str_val(unescape(&s))
}

/// nview.batch(source, ctxs, opts?) — render many contexts (parallel).
// >>> import "nview"
// >>> nview.batch("{{ n }}", [{n: 1}, {n: 2}], {autoescape: false})
fn nview_batch(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nview_batch", span)?;
    let source_or_handle = Rc::clone(&args[0]);
    let mut threads = 0usize;
    let mut opts = ViewOpts::default();
    if args.len() >= 3 {
        match &*args[2].borrow() {
            Value::Object(map) => {
                opts = parse_opts(args, 2, span)?;
                if let Some(v) = map.get("threads") {
                    match &*v.borrow() {
                        Value::Int(n) if *n > 0 => threads = *n as usize,
                        Value::Nil => {}
                        other => {
                            return Err(type_err(
                                span,
                                format!("threads must be positive int, got {}", other.type_name()),
                            ));
                        }
                    }
                }
            }
            Value::Nil => {}
            other => {
                return Err(type_err(
                    span,
                    format!("expected options object, got {}", other.type_name()),
                ));
            }
        }
    }
    let ctxs: Vec<JsonValue> = match &*args[1].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::Object(map) => {
                        let mut obj = JsonMap::new();
                        for (k, vr) in map {
                            obj.insert(k.clone(), value_to_json(&*vr.borrow(), span)?);
                        }
                        out.push(JsonValue::Object(obj));
                    }
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "batch contexts[{i}] must be object, got {}",
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
                    "nview_batch() expects an array of context objects, got {}",
                    other.type_name()
                ),
            ));
        }
    };

    let result = match &*source_or_handle.borrow() {
        Value::String(source) => batch_render(source, &ctxs, &opts, threads),
        Value::Int(id) if *id > 0 => TEMPLATES.with(|m| {
            let m = m.borrow();
            let t = m.get(id).ok_or_else(|| {
                ViewError::invalid(format!("invalid template handle {id}"))
            })?;
            batch_compiled(t, &ctxs, threads)
        }),
        other => {
            return Err(type_err(
                span,
                format!(
                    "nview_batch() expects template string or handle, got {}",
                    other.type_name()
                ),
            ));
        }
    };

    match result {
        Ok(strs) => string_array(strs),
        Err(e) => Ok(nview_err(span, &e)),
    }
}

macro_rules! nview_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nview_fns![
    ("nview_render", "render", nview_render),
    ("nview_compile", "compile", nview_compile),
    ("nview_run", "run", nview_run),
    ("nview_close", "close", nview_close),
    ("nview_env", "env", nview_env),
    ("nview_add", "add", nview_add),
    ("nview_has", "has", nview_has),
    ("nview_names", "names", nview_names),
    ("nview_remove", "remove", nview_remove),
    ("nview_render_named", "render_named", nview_render_named),
    ("nview_render_in", "render_in", nview_render_in),
    ("nview_env_close", "env_close", nview_env_close),
    ("nview_render_file", "render_file", nview_render_file),
    ("nview_add_file", "add_file", nview_add_file),
    ("nview_load_dir", "load_dir", nview_load_dir),
    ("nview_valid", "valid", nview_valid),
    ("nview_vars", "vars", nview_vars),
    ("nview_filters", "filters", nview_filters),
    ("nview_escape", "escape", nview_escape),
    ("nview_escape_attr", "escape_attr", nview_escape_attr),
    ("nview_unescape", "unescape", nview_unescape),
    ("nview_batch", "batch", nview_batch),
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

pub const MODULE_NAME: &str = "nview";
pub const MODULE_PATHS: &[&str] = &["nview", "std/nview"];

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

    #[test]
    fn render_doctest() {
        let out = nview_render(
            &[
                Value::String("Hello {{ name }}!".into()).ref_cell(),
                Value::Object({
                    let mut m = HashMap::new();
                    m.insert("name".into(), Value::String("Ada".into()).ref_cell());
                    m
                })
                .ref_cell(),
                Value::Object({
                    let mut m = HashMap::new();
                    m.insert("autoescape".into(), Value::Bool(false).ref_cell());
                    m
                })
                .ref_cell(),
            ],
            span(),
        )
        .unwrap();
        let s = match &*out.borrow() {
            Value::String(s) => s.clone(),
            other => panic!("expected string, got {other:?}"),
        };
        assert_eq!(s, "Hello Ada!");
    }

    #[test]
    fn escape_doctest() {
        let out = nview_escape(&[Value::String("a < b".into()).ref_cell()], span()).unwrap();
        let s = match &*out.borrow() {
            Value::String(s) => s.clone(),
            other => panic!("expected string, got {other:?}"),
        };
        assert_eq!(s, "a &lt; b");
    }

    #[test]
    fn compile_run_close() {
        let t = nview_compile(
            &[
                Value::String("{{ n }}".into()).ref_cell(),
                Value::Object({
                    let mut m = HashMap::new();
                    m.insert("autoescape".into(), Value::Bool(false).ref_cell());
                    m
                })
                .ref_cell(),
            ],
            span(),
        )
        .unwrap();
        let id = match &*t.borrow() {
            Value::Int(n) => *n,
            other => panic!("expected handle, got {other:?}"),
        };
        let out = nview_run(
            &[
                Value::Int(id).ref_cell(),
                Value::Object({
                    let mut m = HashMap::new();
                    m.insert("n".into(), Value::Int(7).ref_cell());
                    m
                })
                .ref_cell(),
            ],
            span(),
        )
        .unwrap();
        let s = match &*out.borrow() {
            Value::String(s) => s.clone(),
            other => panic!("expected string, got {other:?}"),
        };
        assert_eq!(s, "7");
        let closed = nview_close(&[Value::Int(id).ref_cell()], span()).unwrap();
        let ok = matches!(&*closed.borrow(), Value::Bool(true));
        assert!(ok);
    }
}
