//! Native nhtml standard library — forgiving HTML5 parser, CSS selectors,
//! tree walking, text extraction, escape/unescape (~BeautifulSoup4 subset).
//!
//! Import with `import "nhtml"` (or `import "std/nhtml"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_html::{
    alloc_document, ancestors, attr, attrs, child_elements, children, classes, compile_selector,
    descendants, escape, escape_attr, extract_text, find, find_all, has_attr, has_class, id_attr,
    inner_html, is_comment, is_element, is_tag, is_text, matches, next_sibling, node_direct_text,
    node_text, node_type, outer_html, parallel_extract_text, parallel_select, parent, prettify,
    prev_sibling, root_node, select_nodes, select_one, select_with_handle, siblings, strip_tags,
    tag, unescape, valid_selector, DocumentStore, SelectorStore, TextOpts,
};
use niao_parallel::available_threads;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const E3542: u32 = codes::E3542_NHTML_ARITY;
const E3543: u32 = codes::E3543_NHTML_ERROR;
const E3544: u32 = codes::E3544_NHTML_TYPE;
const E3545: u32 = codes::E3545_NHTML_INVALID_HANDLE;

thread_local! {
    static DOCS: RefCell<DocumentStore> = RefCell::new(DocumentStore::new());
    static SELECTORS: RefCell<SelectorStore> = RefCell::new(SelectorStore::new());
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3544, msg.into())
}

fn nhtml_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3543, "nhtml_error", msg.into(), span)
}

fn invalid_handle(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3545, "nhtml_error", msg.into(), span)
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3542,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3542,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
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

fn doc_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    let id = int_arg(args, idx, name, span)?;
    if id <= 0 {
        return Err(type_err(span, format!("{name}() expects a positive document handle")));
    }
    Ok(id)
}

fn node_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    let id = int_arg(args, idx, name, span)?;
    if id <= 0 {
        return Err(type_err(span, format!("{name}() expects a positive node handle")));
    }
    Ok(id)
}

fn bool_val(b: bool) -> NiaoResult<ValueRef> {
    Ok(Value::Bool(b).ref_cell())
}

fn str_val(s: impl Into<String>) -> NiaoResult<ValueRef> {
    Ok(Value::String(s.into()).ref_cell())
}

fn int_val(n: i64) -> NiaoResult<ValueRef> {
    Ok(Value::Int(n).ref_cell())
}

fn nil_val() -> NiaoResult<ValueRef> {
    Ok(Value::Nil.ref_cell())
}

fn string_array(items: Vec<String>) -> NiaoResult<ValueRef> {
    Ok(Value::Array(
        items.into_iter().map(|s| Value::String(s).ref_cell()).collect(),
    )
    .ref_cell())
}

fn int_array(items: Vec<i64>) -> NiaoResult<ValueRef> {
    Ok(Value::Array(
        items.into_iter().map(|n| Value::Int(n).ref_cell()).collect(),
    )
    .ref_cell())
}

fn optional_node(items: Option<i64>) -> NiaoResult<ValueRef> {
    match items {
        Some(n) => int_val(n),
        None => nil_val(),
    }
}

fn string_list_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<String>> {
    match &*args[idx].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::String(s) => out.push(s.clone()),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() expects string array; item {} is {}",
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
                "{name}() expects an array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn parse_opts(args: &[ValueRef], idx: usize, span: Span) -> NiaoResult<HashMap<String, ValueRef>> {
    if args.len() <= idx {
        return Ok(HashMap::new());
    }
    match &*args[idx].borrow() {
        Value::Object(map) => Ok(map.clone()),
        Value::Nil => Ok(HashMap::new()),
        other => Err(type_err(
            span,
            format!("expected options object, got {}", other.type_name()),
        )),
    }
}

fn obj_bool(map: &HashMap<String, ValueRef>, key: &str, default: bool) -> bool {
    map.get(key)
        .map(|v| match &*v.borrow() {
            Value::Bool(b) => *b,
            Value::Int(n) => *n != 0,
            _ => default,
        })
        .unwrap_or(default)
}

fn obj_string(map: &HashMap<String, ValueRef>, key: &str, default: &str) -> String {
    map.get(key)
        .and_then(|v| match &*v.borrow() {
            Value::String(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_else(|| default.to_string())
}

fn obj_int(map: &HashMap<String, ValueRef>, key: &str, default: i64) -> i64 {
    map.get(key)
        .and_then(|v| match &*v.borrow() {
            Value::Int(n) => Some(*n),
            _ => None,
        })
        .unwrap_or(default)
}

fn text_opts_from(map: &HashMap<String, ValueRef>) -> TextOpts {
    TextOpts {
        strip: obj_bool(map, "strip", false),
        separator: obj_string(map, "separator", ""),
    }
}

fn html_result<T>(span: Span, r: Result<T, niao_html::HtmlError>) -> Result<T, ValueRef> {
    match r {
        Ok(v) => Ok(v),
        Err(e) => Err(nhtml_err(span, e.message())),
    }
}

fn ensure_doc(doc_id: i64, span: Span) -> Result<(), ValueRef> {
    DOCS.with(|d| {
        if d.borrow().get(doc_id).is_some() {
            Ok(())
        } else {
            Err(invalid_handle(
                span,
                format!("invalid or closed document handle {doc_id}"),
            ))
        }
    })
}

fn optional_attr_filter(
    map: &HashMap<String, ValueRef>,
    span: Span,
) -> NiaoResult<(Option<String>, Option<String>)> {
    match map.get("attrs") {
        None => Ok((None, None)),
        Some(vr) => match &*vr.borrow() {
            Value::Object(obj) => {
                if obj.len() == 1 {
                    let (k, v) = obj.iter().next().unwrap();
                    let val = match &*v.borrow() {
                        Value::String(s) => Some(s.clone()),
                        Value::Bool(true) => None,
                        Value::Nil => None,
                        other => {
                            return Err(type_err(
                                span,
                                format!("attrs values must be string, true, or nil — got {}", other.type_name()),
                            ));
                        }
                    };
                    Ok((Some(k.clone()), val))
                } else if obj.is_empty() {
                    Ok((None, None))
                } else {
                    Err(type_err(
                        span,
                        "find attrs filter supports one attribute key in v0.1.0",
                    ))
                }
            }
            Value::Nil => Ok((None, None)),
            other => Err(type_err(
                span,
                format!("find attrs must be an object, got {}", other.type_name()),
            )),
        },
    }
}

// ---------------------------------------------------------------------------
// Parse & handles
// ---------------------------------------------------------------------------

// >>> nhtml.parse("<p>hi</p>")
fn nhtml_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nhtml_parse", span)?;
    let html = string_arg(args, 0, "nhtml_parse", span)?;
    let opts = parse_opts(args, 1, span)?;
    let fragment = obj_bool(&opts, "fragment", false);
    let id = DOCS.with(|d| alloc_document(&mut d.borrow_mut(), &html, fragment));
    int_val(id)
}

// >>> nhtml.parse_fragment("<span>x</span>")
fn nhtml_parse_fragment(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_parse_fragment", span)?;
    let html = string_arg(args, 0, "nhtml_parse_fragment", span)?;
    let id = DOCS.with(|d| alloc_document(&mut d.borrow_mut(), &html, true));
    int_val(id)
}

fn nhtml_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_close", span)?;
    let id = doc_arg(args, 0, "nhtml_close", span)?;
    let ok = DOCS.with(|d| d.borrow_mut().remove(id));
    if ok {
        nil_val()
    } else {
        Ok(invalid_handle(span, format!("invalid or closed document handle {id}")))
    }
}

fn nhtml_root(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_root", span)?;
    let id = doc_arg(args, 0, "nhtml_root", span)?;
    if let Err(v) = ensure_doc(id, span) {
        return Ok(v);
    }
    DOCS.with(|d| {
        let store = d.borrow();
        match html_result(span, root_node(&store, id)) {
            Ok(n) => int_val(n),
            Err(v) => Ok(v),
        }
    })
}

// ---------------------------------------------------------------------------
// CSS selectors
// ---------------------------------------------------------------------------

// >>> len(nhtml.select(doc, "p")) >= 1
fn nhtml_select(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nhtml_select", span)?;
    let doc_id = doc_arg(args, 0, "nhtml_select", span)?;
    let css = string_arg(args, 1, "nhtml_select", span)?;
    if let Err(v) = ensure_doc(doc_id, span) {
        return Ok(v);
    }
    DOCS.with(|d| {
        let store = d.borrow();
        let root = match html_result(span, root_node(&store, doc_id)) {
            Ok(r) => r,
            Err(v) => return Ok(v),
        };
        match html_result(span, select_nodes(&store, root, &css)) {
            Ok(nodes) => int_array(nodes),
            Err(v) => Ok(v),
        }
    })
}

fn nhtml_select_on(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nhtml_select_on", span)?;
    let node = node_arg(args, 0, "nhtml_select_on", span)?;
    let css = string_arg(args, 1, "nhtml_select_on", span)?;
    DOCS.with(|d| {
        let store = d.borrow();
        match html_result(span, select_nodes(&store, node, &css)) {
            Ok(nodes) => int_array(nodes),
            Err(v) => Ok(v),
        }
    })
}

fn nhtml_select_one(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nhtml_select_one", span)?;
    let doc_id = doc_arg(args, 0, "nhtml_select_one", span)?;
    let css = string_arg(args, 1, "nhtml_select_one", span)?;
    if let Err(v) = ensure_doc(doc_id, span) {
        return Ok(v);
    }
    DOCS.with(|d| {
        let store = d.borrow();
        let root = match html_result(span, root_node(&store, doc_id)) {
            Ok(r) => r,
            Err(v) => return Ok(v),
        };
        match html_result(span, select_one(&store, root, &css)) {
            Ok(n) => optional_node(n),
            Err(v) => Ok(v),
        }
    })
}

fn nhtml_select_one_on(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nhtml_select_one_on", span)?;
    let node = node_arg(args, 0, "nhtml_select_one_on", span)?;
    let css = string_arg(args, 1, "nhtml_select_one_on", span)?;
    DOCS.with(|d| {
        let store = d.borrow();
        match html_result(span, select_one(&store, node, &css)) {
            Ok(n) => optional_node(n),
            Err(v) => Ok(v),
        }
    })
}

fn nhtml_compile_selector(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_compile_selector", span)?;
    let css = string_arg(args, 0, "nhtml_compile_selector", span)?;
    SELECTORS.with(|s| match compile_selector(&mut s.borrow_mut(), &css) {
        Ok(id) => int_val(id),
        Err(e) => Ok(nhtml_err(span, e.message())),
    })
}

fn nhtml_close_selector(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_close_selector", span)?;
    let id = int_arg(args, 0, "nhtml_close_selector", span)?;
    let ok = SELECTORS.with(|s| s.borrow_mut().remove(id));
    if ok {
        nil_val()
    } else {
        Ok(invalid_handle(span, format!("invalid or closed selector handle {id}")))
    }
}

fn nhtml_select_with(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nhtml_select_with", span)?;
    let node = node_arg(args, 0, "nhtml_select_with", span)?;
    let sel_id = int_arg(args, 1, "nhtml_select_with", span)?;
    DOCS.with(|d| {
        let store = d.borrow();
        SELECTORS.with(|s| {
            let sel_store = s.borrow();
            match html_result(span, select_with_handle(&store, &sel_store, node, sel_id)) {
                Ok(nodes) => int_array(nodes),
                Err(v) => Ok(v),
            }
        })
    })
}

fn nhtml_matches(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nhtml_matches", span)?;
    let node = node_arg(args, 0, "nhtml_matches", span)?;
    let css = string_arg(args, 1, "nhtml_matches", span)?;
    DOCS.with(|d| {
        let store = d.borrow();
        match html_result(span, matches(&store, node, &css)) {
            Ok(b) => bool_val(b),
            Err(v) => Ok(v),
        }
    })
}

fn nhtml_valid_selector(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_valid_selector", span)?;
    let css = string_arg(args, 0, "nhtml_valid_selector", span)?;
    bool_val(valid_selector(&css))
}

// ---------------------------------------------------------------------------
// Node metadata
// ---------------------------------------------------------------------------

fn nhtml_tag(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_tag", span)?;
    let node = node_arg(args, 0, "nhtml_tag", span)?;
    DOCS.with(|d| match html_result(span, tag(&d.borrow(), node)) {
        Ok(s) => str_val(s),
        Err(v) => Ok(v),
    })
}

fn nhtml_attr(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nhtml_attr", span)?;
    let node = node_arg(args, 0, "nhtml_attr", span)?;
    let name = string_arg(args, 1, "nhtml_attr", span)?;
    DOCS.with(|d| match html_result(span, attr(&d.borrow(), node, &name)) {
        Ok(Some(s)) => str_val(s),
        Ok(None) => nil_val(),
        Err(v) => Ok(v),
    })
}

fn nhtml_attrs(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_attrs", span)?;
    let node = node_arg(args, 0, "nhtml_attrs", span)?;
    DOCS.with(|d| match html_result(span, attrs(&d.borrow(), node)) {
        Ok(pairs) => {
            let mut map = HashMap::new();
            for (k, v) in pairs {
                map.insert(k, Value::String(v).ref_cell());
            }
            Ok(Value::Object(map).ref_cell())
        }
        Err(v) => Ok(v),
    })
}

fn nhtml_has_attr(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nhtml_has_attr", span)?;
    let node = node_arg(args, 0, "nhtml_has_attr", span)?;
    let name = string_arg(args, 1, "nhtml_has_attr", span)?;
    DOCS.with(|d| match html_result(span, has_attr(&d.borrow(), node, &name)) {
        Ok(b) => bool_val(b),
        Err(v) => Ok(v),
    })
}

fn nhtml_id(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_id", span)?;
    let node = node_arg(args, 0, "nhtml_id", span)?;
    DOCS.with(|d| match html_result(span, id_attr(&d.borrow(), node)) {
        Ok(Some(s)) => str_val(s),
        Ok(None) => nil_val(),
        Err(v) => Ok(v),
    })
}

fn nhtml_classes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_classes", span)?;
    let node = node_arg(args, 0, "nhtml_classes", span)?;
    DOCS.with(|d| match html_result(span, classes(&d.borrow(), node)) {
        Ok(c) => string_array(c),
        Err(v) => Ok(v),
    })
}

fn nhtml_has_class(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nhtml_has_class", span)?;
    let node = node_arg(args, 0, "nhtml_has_class", span)?;
    let cls = string_arg(args, 1, "nhtml_has_class", span)?;
    DOCS.with(|d| match html_result(span, has_class(&d.borrow(), node, &cls)) {
        Ok(b) => bool_val(b),
        Err(v) => Ok(v),
    })
}

fn nhtml_node_type(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_node_type", span)?;
    let node = node_arg(args, 0, "nhtml_node_type", span)?;
    DOCS.with(|d| match html_result(span, node_type(&d.borrow(), node)) {
        Ok(t) => str_val(t),
        Err(v) => Ok(v),
    })
}

fn nhtml_is_element(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_is_element", span)?;
    let node = node_arg(args, 0, "nhtml_is_element", span)?;
    DOCS.with(|d| match html_result(span, is_element(&d.borrow(), node)) {
        Ok(b) => bool_val(b),
        Err(v) => Ok(v),
    })
}

fn nhtml_is_text(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_is_text", span)?;
    let node = node_arg(args, 0, "nhtml_is_text", span)?;
    DOCS.with(|d| match html_result(span, is_text(&d.borrow(), node)) {
        Ok(b) => bool_val(b),
        Err(v) => Ok(v),
    })
}

fn nhtml_is_comment(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_is_comment", span)?;
    let node = node_arg(args, 0, "nhtml_is_comment", span)?;
    DOCS.with(|d| match html_result(span, is_comment(&d.borrow(), node)) {
        Ok(b) => bool_val(b),
        Err(v) => Ok(v),
    })
}

fn nhtml_is_tag(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nhtml_is_tag", span)?;
    let node = node_arg(args, 0, "nhtml_is_tag", span)?;
    let name = string_arg(args, 1, "nhtml_is_tag", span)?;
    DOCS.with(|d| match html_result(span, is_tag(&d.borrow(), node, &name)) {
        Ok(b) => bool_val(b),
        Err(v) => Ok(v),
    })
}

// ---------------------------------------------------------------------------
// Text & serialize
// ---------------------------------------------------------------------------

// >>> nhtml.text(node, {strip: true})
fn nhtml_text(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nhtml_text", span)?;
    let node = node_arg(args, 0, "nhtml_text", span)?;
    let opts = text_opts_from(&parse_opts(args, 1, span)?);
    DOCS.with(|d| match html_result(span, node_text(&d.borrow(), node, &opts)) {
        Ok(s) => str_val(s),
        Err(v) => Ok(v),
    })
}

fn nhtml_direct_text(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_direct_text", span)?;
    let node = node_arg(args, 0, "nhtml_direct_text", span)?;
    DOCS.with(|d| match html_result(span, node_direct_text(&d.borrow(), node)) {
        Ok(s) => str_val(s),
        Err(v) => Ok(v),
    })
}

fn nhtml_html(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_html", span)?;
    let node = node_arg(args, 0, "nhtml_html", span)?;
    DOCS.with(|d| match html_result(span, outer_html(&d.borrow(), node)) {
        Ok(s) => str_val(s),
        Err(v) => Ok(v),
    })
}

fn nhtml_inner_html(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_inner_html", span)?;
    let node = node_arg(args, 0, "nhtml_inner_html", span)?;
    DOCS.with(|d| match html_result(span, inner_html(&d.borrow(), node)) {
        Ok(s) => str_val(s),
        Err(v) => Ok(v),
    })
}

fn nhtml_prettify(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nhtml_prettify", span)?;
    let node = node_arg(args, 0, "nhtml_prettify", span)?;
    let opts = parse_opts(args, 1, span)?;
    let indent = obj_int(&opts, "indent", 2) as usize;
    DOCS.with(|d| match html_result(span, prettify(&d.borrow(), node, indent)) {
        Ok(s) => str_val(s),
        Err(v) => Ok(v),
    })
}

fn nhtml_strip_tags(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_strip_tags", span)?;
    let html = string_arg(args, 0, "nhtml_strip_tags", span)?;
    str_val(strip_tags(&html))
}

// >>> nhtml.extract_text("<p>x</p>", "p")
fn nhtml_extract_text(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nhtml_extract_text", span)?;
    let html = string_arg(args, 0, "nhtml_extract_text", span)?;
    let selector = if args.len() >= 2 {
        match &*args[1].borrow() {
            Value::String(s) => Some(s.clone()),
            Value::Nil => None,
            other => {
                return Err(type_err(
                    span,
                    format!(
                        "nhtml_extract_text() selector must be string or nil, got {}",
                        other.type_name()
                    ),
                ));
            }
        }
    } else {
        None
    };
    let opts = text_opts_from(&parse_opts(args, 2, span)?);
    match extract_text(&html, selector.as_deref(), &opts) {
        Ok(s) => str_val(s),
        Err(e) => Ok(nhtml_err(span, e.message())),
    }
}

// ---------------------------------------------------------------------------
// Tree walking
// ---------------------------------------------------------------------------

fn nhtml_parent(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_parent", span)?;
    let node = node_arg(args, 0, "nhtml_parent", span)?;
    DOCS.with(|d| match html_result(span, parent(&d.borrow(), node)) {
        Ok(p) => optional_node(p),
        Err(v) => Ok(v),
    })
}

fn nhtml_children(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_children", span)?;
    let node = node_arg(args, 0, "nhtml_children", span)?;
    DOCS.with(|d| match html_result(span, children(&d.borrow(), node)) {
        Ok(c) => int_array(c),
        Err(v) => Ok(v),
    })
}

fn nhtml_child_elements(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_child_elements", span)?;
    let node = node_arg(args, 0, "nhtml_child_elements", span)?;
    DOCS.with(|d| match html_result(span, child_elements(&d.borrow(), node)) {
        Ok(c) => int_array(c),
        Err(v) => Ok(v),
    })
}

fn nhtml_descendants(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_descendants", span)?;
    let node = node_arg(args, 0, "nhtml_descendants", span)?;
    DOCS.with(|d| match html_result(span, descendants(&d.borrow(), node)) {
        Ok(c) => int_array(c),
        Err(v) => Ok(v),
    })
}

fn nhtml_ancestors(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_ancestors", span)?;
    let node = node_arg(args, 0, "nhtml_ancestors", span)?;
    DOCS.with(|d| match html_result(span, ancestors(&d.borrow(), node)) {
        Ok(c) => int_array(c),
        Err(v) => Ok(v),
    })
}

fn nhtml_next_sibling(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_next_sibling", span)?;
    let node = node_arg(args, 0, "nhtml_next_sibling", span)?;
    DOCS.with(|d| match html_result(span, next_sibling(&d.borrow(), node)) {
        Ok(p) => optional_node(p),
        Err(v) => Ok(v),
    })
}

fn nhtml_prev_sibling(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_prev_sibling", span)?;
    let node = node_arg(args, 0, "nhtml_prev_sibling", span)?;
    DOCS.with(|d| match html_result(span, prev_sibling(&d.borrow(), node)) {
        Ok(p) => optional_node(p),
        Err(v) => Ok(v),
    })
}

fn nhtml_siblings(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_siblings", span)?;
    let node = node_arg(args, 0, "nhtml_siblings", span)?;
    DOCS.with(|d| match html_result(span, siblings(&d.borrow(), node)) {
        Ok(c) => int_array(c),
        Err(v) => Ok(v),
    })
}

// >>> nhtml.find(node, "a", {attrs: {href: true}})
fn nhtml_find(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nhtml_find", span)?;
    let node = node_arg(args, 0, "nhtml_find", span)?;
    let tag_name = string_arg(args, 1, "nhtml_find", span)?;
    let opts = parse_opts(args, 2, span)?;
    let (ak, av) = optional_attr_filter(&opts, span)?;
    DOCS.with(|d| {
        match html_result(
            span,
            find(
                &d.borrow(),
                node,
                &tag_name,
                ak.as_deref(),
                av.as_deref(),
            ),
        ) {
            Ok(n) => optional_node(n),
            Err(v) => Ok(v),
        }
    })
}

fn nhtml_find_all(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nhtml_find_all", span)?;
    let node = node_arg(args, 0, "nhtml_find_all", span)?;
    let tag_name = string_arg(args, 1, "nhtml_find_all", span)?;
    let opts = parse_opts(args, 2, span)?;
    let (ak, av) = optional_attr_filter(&opts, span)?;
    DOCS.with(|d| {
        match html_result(
            span,
            find_all(
                &d.borrow(),
                node,
                &tag_name,
                ak.as_deref(),
                av.as_deref(),
            ),
        ) {
            Ok(c) => int_array(c),
            Err(v) => Ok(v),
        }
    })
}

// ---------------------------------------------------------------------------
// Escape
// ---------------------------------------------------------------------------

// >>> nhtml.escape("a < b")
fn nhtml_escape(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_escape", span)?;
    str_val(escape(&string_arg(args, 0, "nhtml_escape", span)?))
}

fn nhtml_escape_attr(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_escape_attr", span)?;
    str_val(escape_attr(&string_arg(args, 0, "nhtml_escape_attr", span)?))
}

// >>> nhtml.unescape("&lt;p&gt;")
fn nhtml_unescape(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhtml_unescape", span)?;
    str_val(unescape(&string_arg(args, 0, "nhtml_unescape", span)?))
}

// ---------------------------------------------------------------------------
// Parallel batch
// ---------------------------------------------------------------------------

fn nhtml_parallel_extract(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nhtml_parallel_extract", span)?;
    let htmls = string_list_arg(args, 0, "nhtml_parallel_extract", span)?;
    let selector = if args.len() >= 2 {
        match &*args[1].borrow() {
            Value::String(s) => Some(s.clone()),
            Value::Nil => None,
            other => {
                return Err(type_err(
                    span,
                    format!("selector must be string or nil, got {}", other.type_name()),
                ));
            }
        }
    } else {
        None
    };
    let opts_map = parse_opts(args, 2, span)?;
    let opts = text_opts_from(&opts_map);
    let threads = obj_int(&opts_map, "threads", available_threads() as i64) as usize;
    match parallel_extract_text(&htmls, selector.as_deref(), &opts, threads) {
        Ok(texts) => string_array(texts),
        Err(e) => Ok(nhtml_err(span, e.message())),
    }
}

fn nhtml_parallel_select(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nhtml_parallel_select", span)?;
    let htmls = string_list_arg(args, 0, "nhtml_parallel_select", span)?;
    let css = string_arg(args, 1, "nhtml_parallel_select", span)?;
    let opts = parse_opts(args, 2, span)?;
    let threads = obj_int(&opts, "threads", available_threads() as i64) as usize;
    DOCS.with(|d| {
        match parallel_select(&mut d.borrow_mut(), &htmls, &css, threads) {
            Ok(groups) => {
                let mut out = Vec::with_capacity(groups.len());
                for (doc_id, nodes) in groups {
                    let mut map = HashMap::new();
                    map.insert("doc".to_string(), Value::Int(doc_id).ref_cell());
                    map.insert("nodes".to_string(), int_array(nodes)?);
                    out.push(Value::Object(map).ref_cell());
                }
                Ok(Value::Array(out).ref_cell())
            }
            Err(e) => Ok(nhtml_err(span, e.message())),
        }
    })
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nhtml_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nhtml_fns![
    ("nhtml_parse", "parse", nhtml_parse),
    ("nhtml_parse_fragment", "parse_fragment", nhtml_parse_fragment),
    ("nhtml_close", "close", nhtml_close),
    ("nhtml_root", "root", nhtml_root),
    ("nhtml_select", "select", nhtml_select),
    ("nhtml_select_on", "select_on", nhtml_select_on),
    ("nhtml_select_one", "select_one", nhtml_select_one),
    ("nhtml_select_one_on", "select_one_on", nhtml_select_one_on),
    ("nhtml_compile_selector", "compile_selector", nhtml_compile_selector),
    ("nhtml_close_selector", "close_selector", nhtml_close_selector),
    ("nhtml_select_with", "select_with", nhtml_select_with),
    ("nhtml_matches", "matches", nhtml_matches),
    ("nhtml_valid_selector", "valid_selector", nhtml_valid_selector),
    ("nhtml_tag", "tag", nhtml_tag),
    ("nhtml_attr", "attr", nhtml_attr),
    ("nhtml_attrs", "attrs", nhtml_attrs),
    ("nhtml_has_attr", "has_attr", nhtml_has_attr),
    ("nhtml_id", "id", nhtml_id),
    ("nhtml_classes", "classes", nhtml_classes),
    ("nhtml_has_class", "has_class", nhtml_has_class),
    ("nhtml_node_type", "node_type", nhtml_node_type),
    ("nhtml_is_element", "is_element", nhtml_is_element),
    ("nhtml_is_text", "is_text", nhtml_is_text),
    ("nhtml_is_comment", "is_comment", nhtml_is_comment),
    ("nhtml_is_tag", "is_tag", nhtml_is_tag),
    ("nhtml_text", "text", nhtml_text),
    ("nhtml_direct_text", "direct_text", nhtml_direct_text),
    ("nhtml_html", "html", nhtml_html),
    ("nhtml_inner_html", "inner_html", nhtml_inner_html),
    ("nhtml_prettify", "prettify", nhtml_prettify),
    ("nhtml_strip_tags", "strip_tags", nhtml_strip_tags),
    ("nhtml_extract_text", "extract_text", nhtml_extract_text),
    ("nhtml_parent", "parent", nhtml_parent),
    ("nhtml_children", "children", nhtml_children),
    ("nhtml_child_elements", "child_elements", nhtml_child_elements),
    ("nhtml_descendants", "descendants", nhtml_descendants),
    ("nhtml_ancestors", "ancestors", nhtml_ancestors),
    ("nhtml_next_sibling", "next_sibling", nhtml_next_sibling),
    ("nhtml_prev_sibling", "prev_sibling", nhtml_prev_sibling),
    ("nhtml_siblings", "siblings", nhtml_siblings),
    ("nhtml_find", "find", nhtml_find),
    ("nhtml_find_all", "find_all", nhtml_find_all),
    ("nhtml_escape", "escape", nhtml_escape),
    ("nhtml_escape_attr", "escape_attr", nhtml_escape_attr),
    ("nhtml_unescape", "unescape", nhtml_unescape),
    ("nhtml_parallel_extract", "parallel_extract", nhtml_parallel_extract),
    ("nhtml_parallel_select", "parallel_select", nhtml_parallel_select),
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

pub const MODULE_NAME: &str = "nhtml";
pub const MODULE_PATHS: &[&str] = &["nhtml", "std/nhtml"];

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
    fn parse_doctest() {
        let doc = nhtml_parse(
            &[Value::String("<p>hi</p>".into()).ref_cell()],
            span(),
        )
        .unwrap();
        match &*doc.borrow() {
            Value::Int(id) => assert!(*id > 0),
            other => panic!("expected doc handle, got {other:?}"),
        }
    }
}
