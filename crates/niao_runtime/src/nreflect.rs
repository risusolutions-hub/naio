//! Native nreflect standard library — runtime introspection: function arity/params,
//! doc strings, module listing, and source locations (~Python `inspect` subset).
//!
//! Import with `import "nreflect"` (or `import "std/nreflect"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::{FnDef, Span, TypeName};
use niao_errors::codes;
use niao_reflect::{
    self, doc_for_decl, doc_from_source, find_decl_by_name, format_signature, parse_module_info,
    scan_sources_parallel, SignatureInfo,
};
use std::collections::HashMap;
use std::rc::Rc;

const E3517: u32 = codes::E3517_NREFLECT_ARITY;
const E3518: u32 = codes::E3518_NREFLECT_ERROR;
const E3519: u32 = codes::E3519_NREFLECT_TYPE;
const E3520: u32 = codes::E3520_NREFLECT_NOT_FOUND;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3519, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3517,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3517,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn reflect_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3518, "nreflect_error", msg.into(), span)
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

fn optional_string(args: &[ValueRef], idx: usize) -> Option<String> {
    if args.len() <= idx {
        return None;
    }
    match &*args[idx].borrow() {
        Value::String(s) => Some(s.clone()),
        Value::Nil => None,
        _ => None,
    }
}

fn func_ptr(val: &ValueRef) -> usize {
    Rc::as_ptr(val) as usize
}

fn type_name_str(ty: &TypeName) -> String {
    niao_reflect::format_type_name(ty)
}

fn params_obj(def: &FnDef) -> ValueRef {
    let items: Vec<ValueRef> = def
        .params
        .iter()
        .map(|p| {
            let mut m = HashMap::new();
            m.insert("name".to_string(), Value::String(p.name.clone()).ref_cell());
            if let Some(ty) = &p.ty {
                m.insert("type".to_string(), Value::String(type_name_str(ty)).ref_cell());
            }
            m.insert("line".to_string(), Value::Int(p.span.line as i64).ref_cell());
            m.insert("col".to_string(), Value::Int(p.span.col as i64).ref_cell());
            Value::Object(m).ref_cell()
        })
        .collect();
    Value::Array(items).ref_cell()
}

fn signature_from_fndef(def: &FnDef) -> ValueRef {
    let mut m = HashMap::new();
    m.insert("name".to_string(), Value::String(def.name.clone()).ref_cell());
    m.insert("params".to_string(), params_obj(def));
    m.insert("arity".to_string(), Value::Int(def.params.len() as i64).ref_cell());
    if let Some(ret) = &def.return_type {
        m.insert(
            "return_type".to_string(),
            Value::String(type_name_str(ret)).ref_cell(),
        );
    }
    m.insert("line".to_string(), Value::Int(def.span.line as i64).ref_cell());
    m.insert("col".to_string(), Value::Int(def.span.col as i64).ref_cell());
    m.insert(
        "formatted".to_string(),
        Value::String(format_signature(&sig_info_from_fndef(def))).ref_cell(),
    );
    Value::Object(m).ref_cell()
}

fn sig_info_from_fndef(def: &FnDef) -> SignatureInfo {
    SignatureInfo {
        name: def.name.clone(),
        params: def
            .params
            .iter()
            .map(|p| niao_reflect::ParamInfo {
                name: p.name.clone(),
                type_name: p.ty.as_ref().map(type_name_str),
                line: p.span.line,
                col: p.span.col,
            })
            .collect(),
        return_type: def.return_type.as_ref().map(type_name_str),
        arity: def.params.len(),
        line: def.span.line,
        col: def.span.col,
        span_start: def.span.start,
        span_end: def.span.end,
    }
}

fn location_obj(file: Option<String>, line: usize, col: usize, end_line: Option<usize>, end_col: Option<usize>) -> ValueRef {
    let mut m = HashMap::new();
    if let Some(f) = file {
        m.insert("file".to_string(), Value::String(f).ref_cell());
    }
    m.insert("line".to_string(), Value::Int(line as i64).ref_cell());
    m.insert("col".to_string(), Value::Int(col as i64).ref_cell());
    if let Some(el) = end_line {
        m.insert("end_line".to_string(), Value::Int(el as i64).ref_cell());
    }
    if let Some(ec) = end_col {
        m.insert("end_col".to_string(), Value::Int(ec as i64).ref_cell());
    }
    Value::Object(m).ref_cell()
}

fn user_fndef(args: &[ValueRef], span: Span, name: &str) -> NiaoResult<FnDef> {
    match &*args[0].borrow() {
        Value::Function(f) => Ok(f.def.clone()),
        other => Err(type_err(
            span,
            format!("{name}() expects a user function, got {}", other.type_name()),
        )),
    }
}

fn value_kind(v: &Value) -> &'static str {
    match v {
        Value::Function(_) => "function",
        Value::NativeFunction(_) => "native_function",
        Value::Instance(_) => "instance",
        Value::Object(_) => "object",
        Value::Array(_) | Value::IntArray(_) | Value::FloatArray(_) | Value::BoolArray(_)
        | Value::ByteArray(_) | Value::StringArray(_) => "array",
        Value::String(_) => "string",
        Value::Int(_) => "int",
        Value::BigInt(_) => "bigint",
        Value::Float(_) => "float",
        Value::Bool(_) => "bool",
        Value::Nil => "nil",
        Value::Native(_) => "native_ds",
        Value::Error(_) => "error",
        Value::NclHandle(_) => "ncl_handle",
        Value::NmlHandle(_) => "nml_handle",
        #[cfg(feature = "nmongo")]
        Value::BsonDoc(_) => "bson_doc",
    }
}

// ---------------------------------------------------------------------------
// Predicates
// ---------------------------------------------------------------------------

/// >>> nreflect.is_function(fn(x) { return x })
/// => true
fn nreflect_is_function(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nreflect_is_function", span)?;
    Ok(Value::Bool(matches!(&*args[0].borrow(), Value::Function(_))).ref_cell())
}

/// >>> nreflect.is_native(nreflect.arity)
/// => true
fn nreflect_is_native(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nreflect_is_native", span)?;
    Ok(Value::Bool(matches!(&*args[0].borrow(), Value::NativeFunction(_))).ref_cell())
}

/// >>> nreflect.is_callable(fn() { })
/// => true
fn nreflect_is_callable(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nreflect_is_callable", span)?;
    let callable = matches!(
        &*args[0].borrow(),
        Value::Function(_) | Value::NativeFunction(_)
    );
    Ok(Value::Bool(callable).ref_cell())
}

/// >>> nreflect.is_instance(obj)
/// => false
fn nreflect_is_instance(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nreflect_is_instance", span)?;
    Ok(Value::Bool(matches!(&*args[0].borrow(), Value::Instance(_))).ref_cell())
}

/// >>> nreflect.kind(42)
/// => "int"
fn nreflect_kind(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nreflect_kind", span)?;
    Ok(Value::String(value_kind(&args[0].borrow()).into()).ref_cell())
}

// ---------------------------------------------------------------------------
// Function metadata
// ---------------------------------------------------------------------------

/// >>> let f = fn(a, b) { return a + b }; nreflect.arity(f)
/// => 2
fn nreflect_arity(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nreflect_arity", span)?;
    match &*args[0].borrow() {
        Value::Function(f) => Ok(Value::Int(f.def.params.len() as i64).ref_cell()),
        Value::NativeFunction(_) => Ok(Value::Nil.ref_cell()),
        other => Err(type_err(
            span,
            format!(
                "nreflect_arity() expects a function, got {}",
                other.type_name()
            ),
        )),
    }
}

/// >>> let f = fn(x) { return x }; nreflect.name(f)
/// => "<anonymous>" or declared name
fn nreflect_name(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nreflect_name", span)?;
    match &*args[0].borrow() {
        Value::Function(f) => Ok(Value::String(f.def.name.clone()).ref_cell()),
        Value::NativeFunction(_) => Ok(Value::String("<native>".into()).ref_cell()),
        other => Err(type_err(
            span,
            format!("nreflect_name() expects a function, got {}", other.type_name()),
        )),
    }
}

/// >>> let f = fn(a: int) -> int { return a }; len(nreflect.params(f))
/// => 1
fn nreflect_params(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nreflect_params", span)?;
    let def = user_fndef(args, span, "nreflect_params")?;
    Ok(params_obj(&def))
}

/// >>> nreflect.signature(fn(a, b) { return a + b }).arity
/// => 2
fn nreflect_signature(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nreflect_signature", span)?;
    let def = user_fndef(args, span, "nreflect_signature")?;
    Ok(signature_from_fndef(&def))
}

/// >>> nreflect.return_type(fn() -> int { return 1 })
/// => "int"
fn nreflect_return_type(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nreflect_return_type", span)?;
    let def = user_fndef(args, span, "nreflect_return_type")?;
    Ok(match &def.return_type {
        Some(ty) => Value::String(type_name_str(ty)).ref_cell(),
        None => Value::Nil.ref_cell(),
    })
}

/// >>> nreflect.format_signature(fn(a: int, b: int) -> int { return a + b })
/// => "add(a: int, b: int) -> int"
fn nreflect_format_signature(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nreflect_format_signature", span)?;
    let def = user_fndef(args, span, "nreflect_format_signature")?;
    Ok(Value::String(format_signature(&sig_info_from_fndef(&def))).ref_cell())
}

// ---------------------------------------------------------------------------
// Doc strings & source
// ---------------------------------------------------------------------------

/// >>> nreflect.doc_from_source("// hello\nfn f() { }", "f")
/// => "hello"
fn nreflect_doc_from_source(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nreflect_doc_from_source", span)?;
    let source = string_arg(args, 0, "nreflect_doc_from_source", span)?;
    let name = string_arg(args, 1, "nreflect_doc_from_source", span)?;
    Ok(match doc_from_source(&source, &name) {
        Some(d) => Value::String(d).ref_cell(),
        None => Value::Nil.ref_cell(),
    })
}

/// >>> nreflect.doc(fn_with_doc_comment)
/// => doc string or nil
fn nreflect_doc(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nreflect_doc", span)?;
    if let Value::Function(_f) = &*args[0].borrow() {
        if let Some(meta) = niao_reflect::function_meta(func_ptr(&args[0])) {
            if let Some(d) = doc_for_decl(&meta.source, &meta.name, Some("fn")) {
                return Ok(Value::String(d).ref_cell());
            }
        }
        return Ok(Value::Nil.ref_cell());
    }
    Err(type_err(
        span,
        format!(
            "nreflect_doc() expects a user function, got {}",
            args[0].borrow().type_name()
        ),
    ))
}

/// >>> nreflect.source(fn_declared_in_loaded_module)
/// => source text or nil
fn nreflect_source(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nreflect_source", span)?;
    if let Value::Function(f) = &*args[0].borrow() {
        if let Some(meta) = niao_reflect::function_meta(func_ptr(&args[0])) {
            if meta.span_end > meta.span_start && meta.span_end <= meta.source.len() {
                let slice = &meta.source[meta.span_start..meta.span_end];
                return Ok(Value::String(slice.to_string()).ref_cell());
            }
        }
        if let Some(meta) = niao_reflect::function_meta(func_ptr(&args[0])) {
            if let Some(sig) = find_decl_by_name(&meta.source, &f.def.name) {
                if sig.span_end > sig.span_start && sig.span_end <= meta.source.len() {
                    let slice = &meta.source[sig.span_start..sig.span_end];
                    return Ok(Value::String(slice.to_string()).ref_cell());
                }
            }
        }
        return Ok(Value::Nil.ref_cell());
    }
    Err(type_err(span, "nreflect_source() expects a user function"))
}

/// >>> nreflect.source_lines(fn).start
/// => 1
fn nreflect_source_lines(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nreflect_source_lines", span)?;
    if let Value::Function(f) = &*args[0].borrow() {
        let mut m = HashMap::new();
        m.insert("start".to_string(), Value::Int(f.def.span.line as i64).ref_cell());
        if let Some(meta) = niao_reflect::function_meta(func_ptr(&args[0])) {
            if meta.span_end > meta.span_start && meta.span_end <= meta.source.len() {
                let slice = &meta.source[meta.span_start..meta.span_end];
                let lines: Vec<ValueRef> = slice
                    .lines()
                    .map(|l| Value::String(l.to_string()).ref_cell())
                    .collect();
                m.insert("lines".to_string(), Value::Array(lines).ref_cell());
            }
        }
        return Ok(Value::Object(m).ref_cell());
    }
    Err(type_err(span, "nreflect_source_lines() expects a user function"))
}

/// >>> nreflect.source_file(fn)
/// => module path or nil
fn nreflect_source_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nreflect_source_file", span)?;
    if let Value::Function(_) = &*args[0].borrow() {
        if let Some(meta) = niao_reflect::function_meta(func_ptr(&args[0])) {
            return Ok(Value::String(meta.module_path).ref_cell());
        }
        return Ok(Value::Nil.ref_cell());
    }
    Err(type_err(span, "nreflect_source_file() expects a user function"))
}

/// >>> nreflect.location(fn).line
/// => 1
fn nreflect_location(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nreflect_location", span)?;
    match &*args[0].borrow() {
        Value::Function(f) => {
            let file = niao_reflect::function_meta(func_ptr(&args[0]))
                .map(|m| m.module_path);
            Ok(location_obj(file, f.def.span.line, f.def.span.col, None, None))
        }
        other => Err(type_err(
            span,
            format!(
                "nreflect_location() expects a user function, got {}",
                other.type_name()
            ),
        )),
    }
}

// ---------------------------------------------------------------------------
// Modules
// ---------------------------------------------------------------------------

/// >>> len(nreflect.modules()) >= 0
/// => true
fn nreflect_modules(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nreflect_modules", span)?;
    let items: Vec<ValueRef> = niao_reflect::list_modules()
        .into_iter()
        .map(|rec| {
            let mut m = HashMap::new();
            m.insert("path".to_string(), Value::String(rec.path).ref_cell());
            m.insert(
                "exports".to_string(),
                Value::Int(rec.exports.len() as i64).ref_cell(),
            );
            m.insert("kind".to_string(), Value::String("file".into()).ref_cell());
            Value::Object(m).ref_cell()
        })
        .collect();
    Ok(Value::Array(items).ref_cell())
}

/// >>> len(nreflect.native_modules()) > 0
/// => true
fn nreflect_native_modules(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nreflect_native_modules", span)?;
    let paths = crate::native_module_paths();
    let items: Vec<ValueRef> = paths
        .iter()
        .filter(|p| !p.starts_with("std/"))
        .map(|p| Value::String((*p).to_string()).ref_cell())
        .collect();
    Ok(Value::Array(items).ref_cell())
}

/// >>> nreflect.module_info("path") or nil
fn nreflect_module_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nreflect_module_info", span)?;
    let path = string_arg(args, 0, "nreflect_module_info", span)?;
    let Some(rec) = niao_reflect::module_record(&path) else {
        return Ok(Value::Nil.ref_cell());
    };
    Ok(module_info_obj(&rec.path, &rec.info))
}

fn module_info_obj(path: &str, info: &niao_reflect::ModuleInfo) -> ValueRef {
    let mut m = HashMap::new();
    m.insert("path".to_string(), Value::String(path.to_string()).ref_cell());
    m.insert(
        "functions".to_string(),
        Value::Array(
            info.functions
                .iter()
                .map(|f| {
                    let mut fm = HashMap::new();
                    fm.insert("name".to_string(), Value::String(f.name.clone()).ref_cell());
                    fm.insert("arity".to_string(), Value::Int(f.arity as i64).ref_cell());
                    fm.insert("line".to_string(), Value::Int(f.line as i64).ref_cell());
                    Value::Object(fm).ref_cell()
                })
                .collect(),
        )
        .ref_cell(),
    );
    m.insert(
        "imports".to_string(),
        Value::Array(
            info.imports
                .iter()
                .map(|i| Value::String(i.clone()).ref_cell())
                .collect(),
        )
        .ref_cell(),
    );
    if !info.errors.is_empty() {
        m.insert(
            "errors".to_string(),
            Value::Array(
                info.errors
                    .iter()
                    .map(|e| Value::String(e.clone()).ref_cell())
                    .collect(),
            )
            .ref_cell(),
        );
    }
    Value::Object(m).ref_cell()
}

/// >>> nreflect.module_exports(path)
/// => ["fn1", "fn2"] or nil
fn nreflect_module_exports(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nreflect_module_exports", span)?;
    let path = string_arg(args, 0, "nreflect_module_exports", span)?;
    let Some(rec) = niao_reflect::module_record(&path) else {
        return Ok(Value::Nil.ref_cell());
    };
    let items: Vec<ValueRef> = rec
        .exports
        .iter()
        .map(|n| Value::String(n.clone()).ref_cell())
        .collect();
    Ok(Value::Array(items).ref_cell())
}

/// >>> nreflect.parse_module("fn f() { }").functions[0].name
/// => "f"
fn nreflect_parse_module(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nreflect_parse_module", span)?;
    let source = string_arg(args, 0, "nreflect_parse_module", span)?;
    let info = parse_module_info(&source);
    Ok(module_info_obj("<string>", &info))
}

/// >>> nreflect.register_module("tmp.niao", source)
/// => true
fn nreflect_register_module(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nreflect_register_module", span)?;
    let path = string_arg(args, 0, "nreflect_register_module", span)?;
    let source = string_arg(args, 1, "nreflect_register_module", span)?;
    niao_reflect::register_module(path, source, None);
    Ok(Value::Bool(true).ref_cell())
}

/// >>> nreflect.find_function(source, "add").name
/// => "add"
fn nreflect_find_function(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nreflect_find_function", span)?;
    let source = string_arg(args, 0, "nreflect_find_function", span)?;
    let name = string_arg(args, 1, "nreflect_find_function", span)?;
    match find_decl_by_name(&source, &name) {
        Some(sig) => {
            let formatted = format_signature(&sig);
            let mut m = HashMap::new();
            m.insert("name".to_string(), Value::String(sig.name).ref_cell());
            m.insert("arity".to_string(), Value::Int(sig.arity as i64).ref_cell());
            m.insert("line".to_string(), Value::Int(sig.line as i64).ref_cell());
            m.insert(
                "formatted".to_string(),
                Value::String(formatted).ref_cell(),
            );
            Ok(Value::Object(m).ref_cell())
        }
        None => Ok(reflect_err(
            span,
            format!("function '{name}' not found in source"),
        )),
    }
}

/// >>> len(nreflect.scan(sources)) == 2
/// => true
fn nreflect_scan(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nreflect_scan", span)?;
    match &*args[0].borrow() {
        Value::Array(items) => {
            let mut pairs = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::Object(map) => {
                        let path = map
                            .get("path")
                            .and_then(|v| match &*v.borrow() {
                                Value::String(s) => Some(s.clone()),
                                _ => None,
                            })
                            .unwrap_or_else(|| format!("item_{i}"));
                        let source = map
                            .get("source")
                            .and_then(|v| match &*v.borrow() {
                                Value::String(s) => Some(s.clone()),
                                _ => None,
                            })
                            .ok_or_else(|| {
                                type_err(
                                    span,
                                    format!("nreflect_scan() item {} missing source string", i + 1),
                                )
                            })?;
                        pairs.push((path, source));
                    }
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "nreflect_scan() expects {{path, source}} objects, got {}",
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            let results = scan_sources_parallel(&pairs);
            let out: Vec<ValueRef> = results
                .into_iter()
                .map(|(path, info)| module_info_obj(&path, &info))
                .collect();
            Ok(Value::Array(out).ref_cell())
        }
        other => Err(type_err(
            span,
            format!(
                "nreflect_scan() expects an array of {{path, source}}, got {}",
                other.type_name()
            ),
        )),
    }
}

// ---------------------------------------------------------------------------
// Members & module lookup
// ---------------------------------------------------------------------------

/// >>> len(nreflect.members({a: 1, b: 2}))
/// => 2
fn nreflect_members(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nreflect_members", span)?;
    let kind_filter = optional_string(args, 1);
    match &*args[0].borrow() {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let items: Vec<ValueRef> = keys
                .into_iter()
                .filter_map(|k| {
                    let val = map.get(k).unwrap();
                    let kind = value_kind(&val.borrow());
                    if let Some(ref f) = kind_filter {
                        if kind != f.as_str() {
                            return None;
                        }
                    }
                    let mut m = HashMap::new();
                    m.insert("name".to_string(), Value::String(k.clone()).ref_cell());
                    m.insert("kind".to_string(), Value::String(kind.into()).ref_cell());
                    Some(Value::Object(m).ref_cell())
                })
                .collect();
            Ok(Value::Array(items).ref_cell())
        }
        Value::Instance(inst) => {
            let mut keys: Vec<&String> = inst.fields.keys().collect();
            keys.sort();
            let items: Vec<ValueRef> = keys
                .into_iter()
                .map(|k| {
                    let mut m = HashMap::new();
                    m.insert("name".to_string(), Value::String(k.clone()).ref_cell());
                    m.insert("kind".to_string(), Value::String("field".into()).ref_cell());
                    Value::Object(m).ref_cell()
                })
                .collect();
            Ok(Value::Array(items).ref_cell())
        }
        other => Err(type_err(
            span,
            format!(
                "nreflect_members() expects object or instance, got {}",
                other.type_name()
            ),
        )),
    }
}

/// >>> nreflect.getmodule(fn)
/// => path or nil
fn nreflect_getmodule(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nreflect_getmodule", span)?;
    if let Value::Function(_) = &*args[0].borrow() {
        if let Some(meta) = niao_reflect::function_meta(func_ptr(&args[0])) {
            return Ok(Value::String(meta.module_path).ref_cell());
        }
        return Ok(Value::Nil.ref_cell());
    }
    Err(type_err(span, "nreflect_getmodule() expects a user function"))
}

// ---------------------------------------------------------------------------
// Stack
// ---------------------------------------------------------------------------

/// >>> type(nreflect.stack())
/// => "array"
fn nreflect_stack(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nreflect_stack", span)?;
    let frames: Vec<ValueRef> = niao_reflect::stack_frames()
        .into_iter()
        .map(|f| {
            let mut m = HashMap::new();
            m.insert("name".to_string(), Value::String(f.name).ref_cell());
            if let Some(file) = f.file {
                m.insert("file".to_string(), Value::String(file).ref_cell());
            }
            m.insert("line".to_string(), Value::Int(f.line as i64).ref_cell());
            m.insert("col".to_string(), Value::Int(f.col as i64).ref_cell());
            Value::Object(m).ref_cell()
        })
        .collect();
    Ok(Value::Array(frames).ref_cell())
}

/// >>> nreflect.current_frame()
/// => frame object or nil
fn nreflect_current_frame(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nreflect_current_frame", span)?;
    Ok(match niao_reflect::current_frame() {
        Some(f) => {
            let mut m = HashMap::new();
            m.insert("name".to_string(), Value::String(f.name).ref_cell());
            if let Some(file) = f.file {
                m.insert("file".to_string(), Value::String(file).ref_cell());
            }
            m.insert("line".to_string(), Value::Int(f.line as i64).ref_cell());
            m.insert("col".to_string(), Value::Int(f.col as i64).ref_cell());
            Value::Object(m).ref_cell()
        }
        None => Value::Nil.ref_cell(),
    })
}

/// >>> nreflect.clear()
/// => true
fn nreflect_clear(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nreflect_clear", span)?;
    niao_reflect::clear_registry();
    Ok(Value::Bool(true).ref_cell())
}

// ---------------------------------------------------------------------------
// Module exports
// ---------------------------------------------------------------------------

macro_rules! nreflect_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nreflect_fns![
    ("nreflect_is_function", "is_function", nreflect_is_function),
    ("nreflect_is_native", "is_native", nreflect_is_native),
    ("nreflect_is_callable", "is_callable", nreflect_is_callable),
    ("nreflect_is_instance", "is_instance", nreflect_is_instance),
    ("nreflect_kind", "kind", nreflect_kind),
    ("nreflect_arity", "arity", nreflect_arity),
    ("nreflect_name", "name", nreflect_name),
    ("nreflect_params", "params", nreflect_params),
    ("nreflect_signature", "signature", nreflect_signature),
    ("nreflect_return_type", "return_type", nreflect_return_type),
    ("nreflect_format_signature", "format_signature", nreflect_format_signature),
    ("nreflect_doc", "doc", nreflect_doc),
    ("nreflect_doc_from_source", "doc_from_source", nreflect_doc_from_source),
    ("nreflect_source", "source", nreflect_source),
    ("nreflect_source_lines", "source_lines", nreflect_source_lines),
    ("nreflect_source_file", "source_file", nreflect_source_file),
    ("nreflect_location", "location", nreflect_location),
    ("nreflect_modules", "modules", nreflect_modules),
    ("nreflect_native_modules", "native_modules", nreflect_native_modules),
    ("nreflect_module_info", "module_info", nreflect_module_info),
    ("nreflect_module_exports", "module_exports", nreflect_module_exports),
    ("nreflect_parse_module", "parse_module", nreflect_parse_module),
    ("nreflect_register_module", "register_module", nreflect_register_module),
    ("nreflect_find_function", "find_function", nreflect_find_function),
    ("nreflect_scan", "scan", nreflect_scan),
    ("nreflect_members", "members", nreflect_members),
    ("nreflect_getmodule", "getmodule", nreflect_getmodule),
    ("nreflect_stack", "stack", nreflect_stack),
    ("nreflect_current_frame", "current_frame", nreflect_current_frame),
    ("nreflect_clear", "clear", nreflect_clear),
];

pub const MODULE_NAME: &str = "nreflect";
pub const MODULE_PATHS: &[&str] = &["nreflect", "std/nreflect"];

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

/// Register a loaded module (called from the interpreter).
pub fn register_loaded_module(path: &str, source: &str, program: Option<&niao_ast::Program>) {
    niao_reflect::register_module(path, source, program);
}

/// Bind function value to module metadata (called from the interpreter).
pub fn bind_user_function(val: &ValueRef, module_path: &str, def: &FnDef) {
    niao_reflect::bind_function(
        func_ptr(val),
        module_path,
        &def.name,
        def.span.start,
        def.span.end,
        def.span.line,
        def.span.col,
    );
}

/// Push a stack frame for user function calls (returns guard).
pub fn push_call_frame(name: &str, file: Option<String>, line: usize, col: usize) -> niao_reflect::FrameGuard {
    niao_reflect::push_frame(name, file, line, col)
}

/// Module path for a registered user function value, if any.
pub fn function_module_path(val: &ValueRef) -> Option<String> {
    niao_reflect::function_meta(func_ptr(val)).map(|m| m.module_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    fn user_fn(name: &str, params: usize) -> ValueRef {
        let mut ps = Vec::new();
        for i in 0..params {
            ps.push(niao_ast::Param {
                name: format!("a{i}"),
                ty: None,
                span: Span::dummy(),
            });
        }
        Value::Function(crate::FunctionValue {
            def: FnDef {
                name: name.into(),
                params: ps,
                return_type: None,
                body: niao_ast::Block {
                    stmts: vec![],
                    span: Span::dummy(),
                },
                span: Span::dummy(),
            },
            closure: crate::Environment::new(),
        })
        .ref_cell()
    }

    #[test]
    fn arity_and_signature() {
        let f = user_fn("add", 2);
        let a = nreflect_arity(&[f.clone()], span()).unwrap();
        assert!(matches!(&*a.borrow(), Value::Int(2)));
        let sig = nreflect_signature(&[f], span()).unwrap();
        match &*sig.borrow() {
            Value::Object(m) => assert!(matches!(&*m["arity"].borrow(), Value::Int(2))),
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn doc_from_source_works() {
        let src = "// hello\nfn f() { }\n";
        let out = nreflect_doc_from_source(
            &[Value::String(src.into()).ref_cell(), Value::String("f".into()).ref_cell()],
            span(),
        )
        .unwrap();
        assert!(matches!(&*out.borrow(), Value::String(s) if s == "hello"));
    }

    #[test]
    fn bench_signature_hot_path() {
        use std::time::Instant;
        let f = user_fn("hot", 4);
        let start = Instant::now();
        for _ in 0..500_000 {
            let _ = nreflect_arity(&[f.clone()], span()).unwrap();
        }
        let elapsed = start.elapsed();
        eprintln!(
            "nreflect_arity: 500k calls in {:.2} ms ({:.0} ns/call)",
            elapsed.as_secs_f64() * 1000.0,
            elapsed.as_nanos() as f64 / 500_000.0
        );
    }
}
