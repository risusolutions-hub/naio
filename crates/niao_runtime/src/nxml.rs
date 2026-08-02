//! Native nxml standard library — XML DOM + streaming parser, namespaces,
//! XPath subset, pretty-print (~xml.etree, lxml subset).
//!
//! Import with `import "nxml"` (or `import "std/nxml"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_parallel::available_threads;
use niao_xml::{
    deep_copy_element, findall, findtext, iter_elements, parent_path, parse, parse_bytes, pretty,
    pretty_doc, resolve_element, resolve_element_mut, to_string_doc, to_string_element,
    parallel_parse, Document, Element, Node, NodePath, StreamEvent, StreamOpts, XmlOpts,
    XmlStreamOwned,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::rc::Rc;

const E4310: u32 = codes::E4310_NXML_ARITY;
const E4311: u32 = codes::E4311_NXML_ERROR;
const E4312: u32 = codes::E4312_NXML_TYPE;
const E4313: u32 = codes::E4313_NXML_PARSE;
const E4314: u32 = codes::E4314_NXML_INVALID_HANDLE;

#[derive(Clone)]
struct ElemRef {
    doc: i64,
    path: NodePath,
}

thread_local! {
    static DOCS: RefCell<HashMap<i64, Document>> = RefCell::new(HashMap::new());
    static ELEMS: RefCell<HashMap<i64, ElemRef>> = RefCell::new(HashMap::new());
    static STREAMS: RefCell<HashMap<i64, XmlStreamOwned>> = RefCell::new(HashMap::new());
    static NEXT_DOC: RefCell<i64> = const { RefCell::new(1) };
    static NEXT_ELEM: RefCell<i64> = const { RefCell::new(1) };
    static NEXT_STREAM: RefCell<i64> = const { RefCell::new(1) };
}

fn alloc_doc(doc: Document) -> i64 {
    let id = NEXT_DOC.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    DOCS.with(|m| m.borrow_mut().insert(id, doc));
    id
}

fn alloc_elem(doc: i64, path: NodePath) -> i64 {
    let id = NEXT_ELEM.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    ELEMS.with(|m| m.borrow_mut().insert(id, ElemRef { doc, path }));
    id
}

fn alloc_stream(s: XmlStreamOwned) -> i64 {
    let id = NEXT_STREAM.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    STREAMS.with(|m| m.borrow_mut().insert(id, s));
    id
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E4312, msg.into())
}

fn xml_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E4311, "nxml_error", msg.into(), span)
}

fn parse_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E4313, "nxml_error", msg.into(), span)
}

fn invalid_doc(span: Span, id: i64) -> ValueRef {
    error_value(E4314, "nxml_error", format!("invalid or closed document handle {id}"), span)
}

fn invalid_elem(span: Span, id: i64) -> ValueRef {
    error_value(E4314, "nxml_error", format!("invalid or closed element handle {id}"), span)
}

fn invalid_stream(span: Span, id: i64) -> ValueRef {
    error_value(E4314, "nxml_error", format!("invalid or closed stream handle {id}"), span)
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E4310,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E4310,
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

fn parse_opts(args: &[ValueRef], idx: usize) -> XmlOpts {
    let mut opts = XmlOpts::default();
    if args.len() <= idx {
        return opts;
    }
    match &*args[idx].borrow() {
        Value::Object(map) => {
            opts.keep_comments = obj_bool(map, "keep_comments", opts.keep_comments);
            opts.keep_pi = obj_bool(map, "keep_pi", opts.keep_pi);
            opts.recover = obj_bool(map, "recover", opts.recover);
            opts.huge_tree = obj_bool(map, "huge_tree", opts.huge_tree);
            opts.xml_declaration = obj_bool(map, "xml_declaration", opts.xml_declaration);
            opts.pretty = obj_bool(map, "pretty", opts.pretty);
            if let Some(s) = obj_string(map, "encoding") {
                opts.encoding = Some(s);
            }
            if let Some(s) = obj_string(map, "indent") {
                opts.indent = Some(s);
            }
        }
        Value::Nil => {}
        _ => {}
    }
    opts
}

fn obj_bool(map: &HashMap<String, ValueRef>, key: &str, default: bool) -> bool {
    map.get(key)
        .and_then(|v| match &*v.borrow() {
            Value::Bool(b) => Some(*b),
            Value::Int(n) => Some(*n != 0),
            _ => None,
        })
        .unwrap_or(default)
}

fn obj_string(map: &HashMap<String, ValueRef>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.clone()),
        _ => None,
    })
}

fn obj_map_arg(args: &[ValueRef], idx: usize) -> HashMap<String, String> {
    if args.len() <= idx {
        return HashMap::new();
    }
    match &*args[idx].borrow() {
        Value::Object(m) => {
            let mut out = HashMap::new();
            for (k, v) in m {
                if let Value::String(s) = &*v.borrow() {
                    out.insert(k.clone(), s.clone());
                }
            }
            out
        }
        _ => HashMap::new(),
    }
}

fn doc_op<F>(id: i64, span: Span, f: F) -> NiaoResult<ValueRef>
where
    F: FnOnce(&Document) -> NiaoResult<ValueRef>,
{
    DOCS.with(|m| {
        let m = m.borrow();
        match m.get(&id) {
            Some(doc) => f(doc),
            None => Ok(invalid_doc(span, id)),
        }
    })
}

fn elem_op<F>(id: i64, span: Span, f: F) -> NiaoResult<ValueRef>
where
    F: FnOnce(&ElemRef) -> NiaoResult<ValueRef>,
{
    ELEMS.with(|m| {
        let m = m.borrow();
        match m.get(&id) {
            Some(er) => f(er),
            None => Ok(invalid_elem(span, id)),
        }
    })
}

fn elem_op_with<F>(id: i64, span: Span, f: F) -> NiaoResult<ValueRef>
where
    F: FnOnce(&Element, &ElemRef) -> NiaoResult<ValueRef>,
{
    let er = ELEMS.with(|m| m.borrow().get(&id).cloned());
    let Some(er) = er else {
        return Ok(invalid_elem(span, id));
    };
    DOCS.with(|m| {
        let m = m.borrow();
        let Some(doc) = m.get(&er.doc) else {
            return Ok(invalid_doc(span, er.doc));
        };
        match resolve_element(doc, &er.path) {
            Ok(el) => f(el, &er),
            Err(e) => Ok(xml_err(span, e.message())),
        }
    })
}

fn elem_op_mut<F>(id: i64, span: Span, f: F) -> NiaoResult<ValueRef>
where
    F: FnOnce(&mut Element, &ElemRef) -> NiaoResult<ValueRef>,
{
    let er = ELEMS.with(|m| m.borrow().get(&id).cloned());
    let Some(er) = er else {
        return Ok(invalid_elem(span, id));
    };
    DOCS.with(|m| {
        let mut m = m.borrow_mut();
        let Some(doc) = m.get_mut(&er.doc) else {
            return Ok(invalid_doc(span, er.doc));
        };
        match resolve_element_mut(doc, &er.path) {
            Ok(el) => f(el, &er),
            Err(e) => Ok(xml_err(span, e.message())),
        }
    })
}

fn int_val(n: i64) -> NiaoResult<ValueRef> {
    Ok(Value::Int(n).ref_cell())
}

fn str_val(s: String) -> NiaoResult<ValueRef> {
    Ok(Value::String(s).ref_cell())
}

fn bool_val(b: bool) -> NiaoResult<ValueRef> {
    Ok(Value::Bool(b).ref_cell())
}

fn nil_val() -> NiaoResult<ValueRef> {
    Ok(Value::Nil.ref_cell())
}

fn stream_event_to_object(ev: StreamEvent) -> HashMap<String, ValueRef> {
    let mut m = HashMap::new();
    match ev {
        StreamEvent::Start { tag, attrs, line, col } => {
            m.insert("kind".into(), Value::String("start".into()).ref_cell());
            m.insert("tag".into(), Value::String(tag).ref_cell());
            let mut am = HashMap::new();
            for (k, v) in attrs {
                am.insert(k, Value::String(v).ref_cell());
            }
            m.insert("attrs".into(), Value::Object(am).ref_cell());
            m.insert("line".into(), Value::Int(line as i64).ref_cell());
            m.insert("col".into(), Value::Int(col as i64).ref_cell());
        }
        StreamEvent::End { tag, line, col } => {
            m.insert("kind".into(), Value::String("end".into()).ref_cell());
            m.insert("tag".into(), Value::String(tag).ref_cell());
            m.insert("line".into(), Value::Int(line as i64).ref_cell());
            m.insert("col".into(), Value::Int(col as i64).ref_cell());
        }
        StreamEvent::Text { text, line, col } => {
            m.insert("kind".into(), Value::String("text".into()).ref_cell());
            m.insert("text".into(), Value::String(text).ref_cell());
            m.insert("line".into(), Value::Int(line as i64).ref_cell());
            m.insert("col".into(), Value::Int(col as i64).ref_cell());
        }
        StreamEvent::Comment { text, line, col } => {
            m.insert("kind".into(), Value::String("comment".into()).ref_cell());
            m.insert("text".into(), Value::String(text).ref_cell());
            m.insert("line".into(), Value::Int(line as i64).ref_cell());
            m.insert("col".into(), Value::Int(col as i64).ref_cell());
        }
        StreamEvent::Pi { target, data, line, col } => {
            m.insert("kind".into(), Value::String("pi".into()).ref_cell());
            m.insert("target".into(), Value::String(target).ref_cell());
            m.insert("data".into(), Value::String(data).ref_cell());
            m.insert("line".into(), Value::Int(line as i64).ref_cell());
            m.insert("col".into(), Value::Int(col as i64).ref_cell());
        }
        StreamEvent::Decl { version, encoding, line, col } => {
            m.insert("kind".into(), Value::String("decl".into()).ref_cell());
            if let Some(v) = version {
                m.insert("version".into(), Value::String(v).ref_cell());
            }
            if let Some(e) = encoding {
                m.insert("encoding".into(), Value::String(e).ref_cell());
            }
            m.insert("line".into(), Value::Int(line as i64).ref_cell());
            m.insert("col".into(), Value::Int(col as i64).ref_cell());
        }
    }
    m
}

// ---------------------------------------------------------------------------
// Parse / emit
// ---------------------------------------------------------------------------

fn nxml_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nxml.parse", span)?;
    let opts = parse_opts(args, 1);
    let input = match &*args[0].borrow() {
        Value::String(s) => parse(s, &opts).map_err(|e| e.message()),
        Value::ByteArray(b) => parse_bytes(b, &opts).map_err(|e| e.message()),
        other => {
            return Err(type_err(
                span,
                format!("nxml.parse() expects string or byte[] as argument 1, got {}", other.type_name()),
            ));
        }
    };
    match input {
        Ok(doc) => int_val(alloc_doc(doc)),
        Err(m) => Ok(parse_err(span, m)),
    }
}

fn nxml_fromstring(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    nxml_parse(args, span)
}

fn nxml_parse_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nxml.parse_file", span)?;
    let path = string_arg(args, 0, "nxml.parse_file", span)?;
    let opts = parse_opts(args, 1);
    let data = fs::read(&path).map_err(|e| {
        type_err(span, format!("nxml.parse_file() cannot read '{path}': {e}"))
    })?;
    match parse_bytes(&data, &opts) {
        Ok(doc) => int_val(alloc_doc(doc)),
        Err(e) => Ok(parse_err(span, e.message())),
    }
}

fn nxml_tostring(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nxml.tostring", span)?;
    let opts = parse_opts(args, 1);
    if let Value::Int(doc_id) = *args[0].borrow() {
        if doc_id > 0 {
            return doc_op(doc_id, span, |doc| match to_string_doc(doc, &opts) {
                Ok(s) => str_val(s),
                Err(e) => Ok(xml_err(span, e.message())),
            });
        }
    }
    let eh = handle_arg(args, 0, "nxml.tostring", span)?;
    elem_op_with(eh, span, |el, _| match to_string_element(el, &opts) {
        Ok(s) => str_val(s),
        Err(e) => Ok(xml_err(span, e.message())),
    })
}

fn nxml_pretty(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nxml.pretty", span)?;
    let indent = optional_string(args, 1);
    let indent_ref = indent.as_deref();
    if let Value::Int(doc_id) = *args[0].borrow() {
        if doc_id > 0 {
            return doc_op(doc_id, span, |doc| match pretty_doc(doc, indent_ref) {
                Ok(s) => str_val(s),
                Err(e) => Ok(xml_err(span, e.message())),
            });
        }
    }
    let eh = handle_arg(args, 0, "nxml.pretty", span)?;
    elem_op_with(eh, span, |el, _| match pretty(el, indent_ref) {
        Ok(s) => str_val(s),
        Err(e) => Ok(xml_err(span, e.message())),
    })
}

fn nxml_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nxml.close", span)?;
    let id = handle_arg(args, 0, "nxml.close", span)?;
    DOCS.with(|m| {
        m.borrow_mut().remove(&id);
    });
    ELEMS.with(|m| {
        m.borrow_mut().retain(|_, er| er.doc != id);
    });
    nil_val()
}

// ---------------------------------------------------------------------------
// Tree navigation
// ---------------------------------------------------------------------------

fn nxml_root(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nxml.root", span)?;
    let doc_id = handle_arg(args, 0, "nxml.root", span)?;
    doc_op(doc_id, span, |doc| {
        if doc.root.is_some() {
            int_val(alloc_elem(doc_id, NodePath::root()))
        } else {
            Ok(xml_err(span, "document has no root element"))
        }
    })
}

fn nxml_tag(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nxml.tag", span)?;
    elem_op_with(handle_arg(args, 0, "nxml.tag", span)?, span, |el, _| {
        str_val(el.tag.clone())
    })
}

fn nxml_text(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nxml.text", span)?;
    elem_op_with(handle_arg(args, 0, "nxml.text", span)?, span, |el, _| {
        str_val(el.text.clone())
    })
}

fn nxml_tail(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nxml.tail", span)?;
    elem_op_with(handle_arg(args, 0, "nxml.tail", span)?, span, |el, _| {
        str_val(el.tail.clone())
    })
}

fn nxml_set_text(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nxml.set_text", span)?;
    let text = string_arg(args, 1, "nxml.set_text", span)?;
    elem_op_mut(handle_arg(args, 0, "nxml.set_text", span)?, span, |el, _| {
        el.text = text;
        nil_val()
    })
}

fn nxml_set_tag(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nxml.set_tag", span)?;
    let tag = string_arg(args, 1, "nxml.set_tag", span)?;
    elem_op_mut(handle_arg(args, 0, "nxml.set_tag", span)?, span, |el, _| {
        el.tag = tag;
        nil_val()
    })
}

fn nxml_attrib(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nxml.attrib", span)?;
    elem_op_with(handle_arg(args, 0, "nxml.attrib", span)?, span, |el, _| {
        let mut m = HashMap::new();
        for (k, v) in el.attr_map() {
            m.insert(k, Value::String(v).ref_cell());
        }
        Ok(Value::Object(m).ref_cell())
    })
}

fn nxml_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nxml.get", span)?;
    let key = string_arg(args, 1, "nxml.get", span)?;
    let default = optional_string(args, 2);
    elem_op_with(handle_arg(args, 0, "nxml.get", span)?, span, |el, _| {
        match el.get_attr(&key) {
            Some(v) => str_val(v.to_string()),
            None => {
                if let Some(d) = default {
                    str_val(d)
                } else {
                    nil_val()
                }
            }
        }
    })
}

fn nxml_set(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nxml.set", span)?;
    let key = string_arg(args, 1, "nxml.set", span)?;
    let val = string_arg(args, 2, "nxml.set", span)?;
    elem_op_mut(handle_arg(args, 0, "nxml.set", span)?, span, |el, _| {
        el.set_attr(key, val);
        nil_val()
    })
}

fn nxml_keys(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nxml.keys", span)?;
    elem_op_with(handle_arg(args, 0, "nxml.keys", span)?, span, |el, _| {
        let items: Vec<ValueRef> = el
            .attrs
            .iter()
            .map(|a| Value::String(a.key()).ref_cell())
            .collect();
        Ok(Value::Array(items).ref_cell())
    })
}

fn nxml_namespace(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nxml.namespace", span)?;
    elem_op_with(handle_arg(args, 0, "nxml.namespace", span)?, span, |el, _| {
        match &el.namespace {
            Some(ns) => str_val(ns.clone()),
            None => nil_val(),
        }
    })
}

fn nxml_qname(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nxml.qname", span)?;
    elem_op_with(handle_arg(args, 0, "nxml.qname", span)?, span, |el, _| str_val(el.qname()))
}

fn child_path_from_iter(root: &Element, target: &Element) -> Option<NodePath> {
    fn walk(el: &Element, target: *const Element, path: &mut Vec<usize>) -> bool {
        if std::ptr::eq(el, target) {
            return true;
        }
        let mut idx = 0usize;
        for child in &el.children {
            if let Node::Element(c) = child {
                path.push(idx);
                if walk(c, target, path) {
                    return true;
                }
                path.pop();
                idx += 1;
            }
        }
        false
    }
    let mut path = Vec::new();
    if walk(root, target, &mut path) {
        Some(NodePath(path))
    } else {
        None
    }
}

fn path_from_doc_root(doc: &Document, target: &Element) -> Option<NodePath> {
    let root = doc.root.as_ref()?;
    child_path_from_iter(root, target)
}

fn nxml_find_with_paths(args: &[ValueRef], span: Span, all: bool) -> NiaoResult<ValueRef> {
    arity(args, 2, if all { "nxml.findall" } else { "nxml.find" }, span)?;
    let eh = handle_arg(args, 0, if all { "nxml.findall" } else { "nxml.find" }, span)?;
    let xpath = string_arg(args, 1, if all { "nxml.findall" } else { "nxml.find" }, span)?;
    elem_op_with(eh, span, |el, er| {
        match findall(el, &xpath) {
            Ok(hits) => {
                let mut handles = Vec::new();
                if let Some(doc) = DOCS.with(|m| m.borrow().get(&er.doc).cloned()) {
                    for hit in hits {
                        if let Some(p) = path_from_doc_root(&doc, hit) {
                            handles.push(Value::Int(alloc_elem(er.doc, p)).ref_cell());
                        }
                    }
                }
                if all {
                    Ok(Value::Array(handles).ref_cell())
                } else if let Some(h) = handles.into_iter().next() {
                    Ok(h)
                } else {
                    nil_val()
                }
            }
            Err(e) => Ok(xml_err(span, e.message())),
        }
    })
}

fn nxml_find_fixed(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    nxml_find_with_paths(args, span, false)
}

fn nxml_findall(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    nxml_find_with_paths(args, span, true)
}

fn nxml_findtext(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nxml.findtext", span)?;
    let xpath = string_arg(args, 1, "nxml.findtext", span)?;
    let default = optional_string(args, 2);
    elem_op_with(handle_arg(args, 0, "nxml.findtext", span)?, span, |el, _| {
        match findtext(el, &xpath, default.as_deref()) {
            Ok(Some(s)) => str_val(s),
            Ok(None) => nil_val(),
            Err(e) => Ok(xml_err(span, e.message())),
        }
    })
}

fn nxml_iter(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nxml.iter", span)?;
    let tag = optional_string(args, 1);
    elem_op_with(handle_arg(args, 0, "nxml.iter", span)?, span, |el, er| {
        let hits = iter_elements(el, tag.as_deref());
        let mut handles = Vec::new();
        if let Some(doc) = DOCS.with(|m| m.borrow().get(&er.doc).cloned()) {
            for hit in hits {
                if let Some(p) = path_from_doc_root(&doc, hit) {
                    handles.push(Value::Int(alloc_elem(er.doc, p)).ref_cell());
                }
            }
        }
        Ok(Value::Array(handles).ref_cell())
    })
}

fn nxml_children(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nxml.children", span)?;
    elem_op_with(handle_arg(args, 0, "nxml.children", span)?, span, |el, er| {
        let mut handles = Vec::new();
        let mut idx = 0usize;
        for child in &el.children {
            if matches!(child, Node::Element(_)) {
                let mut p = er.path.0.clone();
                p.push(idx);
                handles.push(Value::Int(alloc_elem(er.doc, NodePath(p))).ref_cell());
                idx += 1;
            }
        }
        Ok(Value::Array(handles).ref_cell())
    })
}

fn nxml_parent(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nxml.parent", span)?;
    elem_op(handle_arg(args, 0, "nxml.parent", span)?, span, |er| {
        match parent_path(&er.path) {
            Some(p) => int_val(alloc_elem(er.doc, p)),
            None => nil_val(),
        }
    })
}

fn nxml_sub_element(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 4, "nxml.sub_element", span)?;
    let tag = string_arg(args, 1, "nxml.sub_element", span)?;
    let attrs = obj_map_arg(args, 2);
    let text = optional_string(args, 3);
    elem_op_mut(handle_arg(args, 0, "nxml.sub_element", span)?, span, move |el, er| {
        let mut child = Element::new(tag);
        for (k, v) in attrs {
            child.set_attr(k, v);
        }
        if let Some(t) = text {
            child.text = t;
        }
        let child_count = el.child_elements().len();
        if let Err(e) = el.append_element(child) {
            return Ok(xml_err(span, e.message()));
        }
        let mut p = er.path.0.clone();
        p.push(child_count);
        int_val(alloc_elem(er.doc, NodePath(p)))
    })
}

fn nxml_element(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nxml.element", span)?;
    let tag = string_arg(args, 0, "nxml.element", span)?;
    let attrs = obj_map_arg(args, 1);
    let text = optional_string(args, 2);
    let mut root = Element::new(tag);
    for (k, v) in attrs {
        root.set_attr(k, v);
    }
    if let Some(t) = text {
        root.text = t;
    }
    let doc_id = alloc_doc(Document::new(root));
    int_val(alloc_elem(doc_id, NodePath::root()))
}

fn nxml_clear(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nxml.clear", span)?;
    elem_op_mut(handle_arg(args, 0, "nxml.clear", span)?, span, |el, _| {
        el.clear();
        nil_val()
    })
}

fn nxml_copy(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nxml.copy", span)?;
    elem_op_with(handle_arg(args, 0, "nxml.copy", span)?, span, |el, _| {
        let doc_id = alloc_doc(Document::new(deep_copy_element(el)));
        int_val(alloc_elem(doc_id, NodePath::root()))
    })
}

// ---------------------------------------------------------------------------
// Streaming
// ---------------------------------------------------------------------------

fn nxml_stream(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nxml.stream", span)?;
    let mut sopts = StreamOpts::default();
    if args.len() > 1 {
        if let Value::Object(m) = &*args[1].borrow() {
            sopts.trim_text = obj_bool(m, "trim_text", sopts.trim_text);
            sopts.expand_empty = obj_bool(m, "expand_empty", sopts.expand_empty);
        }
    }
    let input = match &*args[0].borrow() {
        Value::String(s) => s.clone(),
        Value::ByteArray(b) => String::from_utf8_lossy(b).into_owned(),
        other => {
            return Err(type_err(
                span,
                format!(
                    "nxml.stream() expects string or byte[] as argument 1, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    match XmlStreamOwned::new(input, sopts) {
        Ok(s) => int_val(alloc_stream(s)),
        Err(e) => Ok(parse_err(span, e.message())),
    }
}

fn nxml_stream_next(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nxml.stream_next", span)?;
    let id = handle_arg(args, 0, "nxml.stream_next", span)?;
    STREAMS.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(s) => match s.next_event() {
                Ok(Some(ev)) => Ok(Value::Object(stream_event_to_object(ev)).ref_cell()),
                Ok(None) => nil_val(),
                Err(e) => Ok(parse_err(span, e.message())),
            },
            None => Ok(invalid_stream(span, id)),
        }
    })
}

fn nxml_stream_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nxml.stream_close", span)?;
    let id = handle_arg(args, 0, "nxml.stream_close", span)?;
    STREAMS.with(|m| {
        m.borrow_mut().remove(&id);
    });
    nil_val()
}

fn nxml_parallel_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nxml.parallel_parse", span)?;
    let opts = parse_opts(args, 1);
    let strings = match &*args[0].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::String(s) => out.push(s.clone()),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "nxml.parallel_parse() expects string array; item {} is {}",
                                i + 1,
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
                    "nxml.parallel_parse() expects an array as argument 1, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    let threads = available_threads();
    let results = parallel_parse(&strings, &opts, threads);
    let mut handles = Vec::with_capacity(results.len());
    for r in results {
        match r {
            Ok(doc) => handles.push(Value::Int(alloc_doc(doc)).ref_cell()),
            Err(e) => handles.push(parse_err(span, e.message())),
        }
    }
    Ok(Value::Array(handles).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nxml_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nxml_fns![
    ("nxml_parse", "parse", nxml_parse),
    ("nxml_fromstring", "fromstring", nxml_fromstring),
    ("nxml_parse_file", "parse_file", nxml_parse_file),
    ("nxml_tostring", "tostring", nxml_tostring),
    ("nxml_pretty", "pretty", nxml_pretty),
    ("nxml_close", "close", nxml_close),
    ("nxml_root", "root", nxml_root),
    ("nxml_tag", "tag", nxml_tag),
    ("nxml_text", "text", nxml_text),
    ("nxml_tail", "tail", nxml_tail),
    ("nxml_set_text", "set_text", nxml_set_text),
    ("nxml_set_tag", "set_tag", nxml_set_tag),
    ("nxml_attrib", "attrib", nxml_attrib),
    ("nxml_get", "get", nxml_get),
    ("nxml_set", "set", nxml_set),
    ("nxml_keys", "keys", nxml_keys),
    ("nxml_namespace", "namespace", nxml_namespace),
    ("nxml_qname", "qname", nxml_qname),
    ("nxml_find", "find", nxml_find_fixed),
    ("nxml_findall", "findall", nxml_findall),
    ("nxml_findtext", "findtext", nxml_findtext),
    ("nxml_iter", "iter", nxml_iter),
    ("nxml_children", "children", nxml_children),
    ("nxml_parent", "parent", nxml_parent),
    ("nxml_sub_element", "sub_element", nxml_sub_element),
    ("nxml_element", "element", nxml_element),
    ("nxml_clear", "clear", nxml_clear),
    ("nxml_copy", "copy", nxml_copy),
    ("nxml_stream", "stream", nxml_stream),
    ("nxml_stream_next", "stream_next", nxml_stream_next),
    ("nxml_stream_close", "stream_close", nxml_stream_close),
    ("nxml_parallel_parse", "parallel_parse", nxml_parallel_parse),
];

pub const MODULE_NAME: &str = "nxml";
pub const MODULE_PATHS: &[&str] = &["nxml", "std/nxml"];

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

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
    fn parse_doctest() {
        let v = nxml_parse(
            &[Value::String("<root><a>1</a></root>".into()).ref_cell()],
            span(),
        )
        .unwrap();
        assert!(matches!(*v.borrow(), Value::Int(n) if n > 0));
    }
}
