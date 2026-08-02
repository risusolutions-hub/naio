//! Native nwhy standard library — value lineage / provenance tracking.
//! Handles wrap a stored value plus label and optional parent handles so
//! callers can explain and graph how a result was derived.
//!
//! Import with `import "nwhy"` (or `import "std/nwhy"`).

use crate::{error_value, values_equal, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;

// Wired in codes.rs by central integration.
const E2970_NWHY_ARITY: u32 = 2970;
const E2971_NWHY_ERROR: u32 = 2971;
const E2972_NWHY_TYPE: u32 = 2972;
const E2973_NWHY_INVALID_HANDLE: u32 = 2973;

// ---------------------------------------------------------------------------
// Lineage model
// ---------------------------------------------------------------------------

struct Node {
    value: ValueRef,
    label: String,
    parents: Vec<i64>,
}

thread_local! {
    static NODES: RefCell<HashMap<i64, Node>> = RefCell::new(HashMap::new());
    static NEXT_HANDLE: RefCell<i64> = const { RefCell::new(1) };
}

fn new_handle() -> i64 {
    NEXT_HANDLE.with(|h| {
        let mut h = h.borrow_mut();
        let id = *h;
        *h += 1;
        id
    })
}

fn invalid_handle(span: Span, id: i64) -> ValueRef {
    error_value(
        E2973_NWHY_INVALID_HANDLE,
        "nwhy_error",
        format!("invalid or closed lineage handle {id}"),
        span,
    )
}

fn nwhy_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E2971_NWHY_ERROR, "nwhy_error", msg.into(), span)
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E2970_NWHY_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E2972_NWHY_TYPE, msg.into())
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

fn handle_ids_from_array(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> NiaoResult<Vec<i64>> {
    match &*args[idx].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::Int(id) => out.push(*id),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() expects handle ints in inputs array, got {} at index {i}",
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
                "{name}() expects an array of handles as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

/// ==-style compare with Debug/type fallback for variants `values_equal` skips.
fn same_values(a: &Value, b: &Value) -> bool {
    if values_equal(a, b) {
        return true;
    }
    a.type_name() == b.type_name() && format!("{a:?}") == format!("{b:?}")
}

fn collect_ancestors(root: i64) -> Option<(Vec<(i64, String)>, Vec<(i64, i64)>)> {
    NODES.with(|nodes| {
        let nodes = nodes.borrow();
        if !nodes.contains_key(&root) {
            return None;
        }
        let mut seen = HashSet::new();
        let mut order = Vec::new();
        let mut edges = Vec::new();
        let mut q = VecDeque::new();
        q.push_back(root);
        seen.insert(root);
        while let Some(id) = q.pop_front() {
            let Some(node) = nodes.get(&id) else {
                continue;
            };
            order.push((id, node.label.clone()));
            for &p in &node.parents {
                edges.push((p, id));
                if seen.insert(p) {
                    q.push_back(p);
                }
            }
        }
        Some((order, edges))
    })
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// nwhy_track(value, label) → handle
fn nwhy_track(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nwhy_track", span)?;
    let label = string_arg(args, 1, "nwhy_track", span)?;
    if label.is_empty() {
        return Ok(nwhy_err(span, "nwhy_track() label must be non-empty"));
    }
    let id = new_handle();
    NODES.with(|nodes| {
        nodes.borrow_mut().insert(
            id,
            Node {
                value: Rc::clone(&args[0]),
                label,
                parents: Vec::new(),
            },
        );
    });
    Ok(Value::Int(id).ref_cell())
}

/// nwhy_derive(inputs_array, value, op_label) → handle
fn nwhy_derive(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nwhy_derive", span)?;
    let parents = handle_ids_from_array(args, 0, "nwhy_derive", span)?;
    let label = string_arg(args, 2, "nwhy_derive", span)?;
    if label.is_empty() {
        return Ok(nwhy_err(span, "nwhy_derive() op_label must be non-empty"));
    }
    // Validate every parent exists before allocating a new node.
    let missing = NODES.with(|nodes| {
        let nodes = nodes.borrow();
        parents.iter().copied().find(|p| !nodes.contains_key(p))
    });
    if let Some(bad) = missing {
        return Ok(invalid_handle(span, bad));
    }
    let id = new_handle();
    NODES.with(|nodes| {
        nodes.borrow_mut().insert(
            id,
            Node {
                value: Rc::clone(&args[1]),
                label,
                parents,
            },
        );
    });
    Ok(Value::Int(id).ref_cell())
}

fn nwhy_value(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nwhy_value", span)?;
    let id = int_arg(args, 0, "nwhy_value", span)?;
    NODES.with(|nodes| match nodes.borrow().get(&id) {
        Some(n) => Ok(Rc::clone(&n.value)),
        None => Ok(invalid_handle(span, id)),
    })
}

fn nwhy_label(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nwhy_label", span)?;
    let id = int_arg(args, 0, "nwhy_label", span)?;
    NODES.with(|nodes| match nodes.borrow().get(&id) {
        Some(n) => Ok(Value::String(n.label.clone()).ref_cell()),
        None => Ok(invalid_handle(span, id)),
    })
}

fn nwhy_parents(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nwhy_parents", span)?;
    let id = int_arg(args, 0, "nwhy_parents", span)?;
    NODES.with(|nodes| match nodes.borrow().get(&id) {
        Some(n) => {
            let arr = n
                .parents
                .iter()
                .map(|p| Value::Int(*p).ref_cell())
                .collect::<Vec<_>>();
            Ok(Value::Array(arr).ref_cell())
        }
        None => Ok(invalid_handle(span, id)),
    })
}

/// Human string: `"op ← parent labels..."` (or just the label for roots).
fn nwhy_explain(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nwhy_explain", span)?;
    let id = int_arg(args, 0, "nwhy_explain", span)?;
    NODES.with(|nodes| {
        let nodes = nodes.borrow();
        let Some(node) = nodes.get(&id) else {
            return Ok(invalid_handle(span, id));
        };
        if node.parents.is_empty() {
            return Ok(Value::String(node.label.clone()).ref_cell());
        }
        let mut parent_labels = Vec::with_capacity(node.parents.len());
        for &p in &node.parents {
            match nodes.get(&p) {
                Some(pn) => parent_labels.push(pn.label.clone()),
                None => parent_labels.push(format!("#{p}?")),
            }
        }
        let s = format!("{} ← {}", node.label, parent_labels.join(", "));
        Ok(Value::String(s).ref_cell())
    })
}

/// `{nodes:[{id,label}], edges:[{from,to}]}` over the ancestor DAG.
fn nwhy_graph(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nwhy_graph", span)?;
    let id = int_arg(args, 0, "nwhy_graph", span)?;
    let Some((order, edges)) = collect_ancestors(id) else {
        return Ok(invalid_handle(span, id));
    };
    let node_vals = order
        .into_iter()
        .map(|(nid, label)| {
            let mut m = HashMap::new();
            m.insert("id".to_string(), Value::Int(nid).ref_cell());
            m.insert("label".to_string(), Value::String(label).ref_cell());
            Value::Object(m).ref_cell()
        })
        .collect::<Vec<_>>();
    let edge_vals = edges
        .into_iter()
        .map(|(from, to)| {
            let mut m = HashMap::new();
            m.insert("from".to_string(), Value::Int(from).ref_cell());
            m.insert("to".to_string(), Value::Int(to).ref_cell());
            Value::Object(m).ref_cell()
        })
        .collect::<Vec<_>>();
    let mut out = HashMap::new();
    out.insert("nodes".to_string(), Value::Array(node_vals).ref_cell());
    out.insert("edges".to_string(), Value::Array(edge_vals).ref_cell());
    Ok(Value::Object(out).ref_cell())
}

fn nwhy_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nwhy_close", span)?;
    let id = int_arg(args, 0, "nwhy_close", span)?;
    let removed = NODES.with(|nodes| nodes.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

fn nwhy_same(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nwhy_same", span)?;
    let a = int_arg(args, 0, "nwhy_same", span)?;
    let b = int_arg(args, 1, "nwhy_same", span)?;
    NODES.with(|nodes| {
        let nodes = nodes.borrow();
        let Some(na) = nodes.get(&a) else {
            return Ok(invalid_handle(span, a));
        };
        let Some(nb) = nodes.get(&b) else {
            return Ok(invalid_handle(span, b));
        };
        let eq = same_values(&na.value.borrow(), &nb.value.borrow());
        Ok(Value::Bool(eq).ref_cell())
    })
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nwhy_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nwhy_fns![
    ("nwhy_track", "track", nwhy_track),
    ("nwhy_derive", "derive", nwhy_derive),
    ("nwhy_value", "value", nwhy_value),
    ("nwhy_label", "label", nwhy_label),
    ("nwhy_parents", "parents", nwhy_parents),
    ("nwhy_explain", "explain", nwhy_explain),
    ("nwhy_graph", "graph", nwhy_graph),
    ("nwhy_close", "close", nwhy_close),
    ("nwhy_same", "same", nwhy_same),
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

pub const MODULE_NAME: &str = "nwhy";
pub const MODULE_PATHS: &[&str] = &["nwhy", "std/nwhy"];

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

    fn i(v: i64) -> ValueRef {
        Value::Int(v).ref_cell()
    }

    fn s(v: &str) -> ValueRef {
        Value::String(v.to_string()).ref_cell()
    }

    fn arr(items: Vec<ValueRef>) -> ValueRef {
        Value::Array(items).ref_cell()
    }

    fn handle(r: NiaoResult<ValueRef>) -> ValueRef {
        let v = r.unwrap();
        assert!(matches!(&*v.borrow(), Value::Int(_)), "expected handle int");
        v
    }

    fn as_int(v: &ValueRef) -> i64 {
        match &*v.borrow() {
            Value::Int(n) => *n,
            other => panic!("expected int, got {other:?}"),
        }
    }

    #[test]
    fn track_and_read_back() {
        let h = handle(nwhy_track(&[i(42), s("answer")], span()));
        let v = nwhy_value(&[h.clone()], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Int(42)));
        let lab = nwhy_label(&[h.clone()], span()).unwrap();
        assert!(matches!(&*lab.borrow(), Value::String(x) if x == "answer"));
        let parents = nwhy_parents(&[h.clone()], span()).unwrap();
        match &*parents.borrow() {
            Value::Array(a) => assert!(a.is_empty()),
            other => panic!("expected array, got {other:?}"),
        }
        nwhy_close(&[h], span()).unwrap();
    }

    #[test]
    fn derive_explain_and_graph() {
        let a = handle(nwhy_track(&[i(2), s("x")], span()));
        let b = handle(nwhy_track(&[i(3), s("y")], span()));
        let sum = handle(nwhy_derive(
            &[arr(vec![a.clone(), b.clone()]), i(5), s("add")],
            span(),
        ));
        let expl = nwhy_explain(&[sum.clone()], span()).unwrap();
        assert!(matches!(&*expl.borrow(), Value::String(x) if x == "add ← x, y"));

        let parents = nwhy_parents(&[sum.clone()], span()).unwrap();
        match &*parents.borrow() {
            Value::Array(ps) => {
                assert_eq!(ps.len(), 2);
                assert_eq!(as_int(&ps[0]), as_int(&a));
                assert_eq!(as_int(&ps[1]), as_int(&b));
            }
            other => panic!("expected array, got {other:?}"),
        }

        let g = nwhy_graph(&[sum.clone()], span()).unwrap();
        match &*g.borrow() {
            Value::Object(map) => {
                let nodes = map.get("nodes").unwrap();
                let edges = map.get("edges").unwrap();
                match (&*nodes.borrow(), &*edges.borrow()) {
                    (Value::Array(ns), Value::Array(es)) => {
                        assert_eq!(ns.len(), 3);
                        assert_eq!(es.len(), 2);
                    }
                    _ => panic!("bad graph shape"),
                }
            }
            other => panic!("expected object, got {other:?}"),
        }

        nwhy_close(&[sum], span()).unwrap();
        nwhy_close(&[a], span()).unwrap();
        nwhy_close(&[b], span()).unwrap();
    }

    #[test]
    fn same_compares_underlying_values() {
        let a = handle(nwhy_track(&[i(7), s("a")], span()));
        let b = handle(nwhy_track(&[i(7), s("b")], span()));
        let c = handle(nwhy_track(&[i(8), s("c")], span()));
        let ab = nwhy_same(&[a.clone(), b.clone()], span()).unwrap();
        assert!(matches!(&*ab.borrow(), Value::Bool(true)));
        let ac = nwhy_same(&[a.clone(), c.clone()], span()).unwrap();
        assert!(matches!(&*ac.borrow(), Value::Bool(false)));
        nwhy_close(&[a], span()).unwrap();
        nwhy_close(&[b], span()).unwrap();
        nwhy_close(&[c], span()).unwrap();
    }

    #[test]
    fn invalid_handle_is_error_value() {
        let v = nwhy_value(&[i(424_242)], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn close_returns_bool() {
        let h = handle(nwhy_track(&[s("hi"), s("msg")], span()));
        let ok = nwhy_close(&[h.clone()], span()).unwrap();
        assert!(matches!(&*ok.borrow(), Value::Bool(true)));
        let again = nwhy_close(&[h], span()).unwrap();
        assert!(matches!(&*again.borrow(), Value::Bool(false)));
    }

    #[test]
    fn arity_error() {
        let err = nwhy_track(&[i(1)], span()).unwrap_err();
        assert_eq!(err.code(), E2970_NWHY_ARITY);
    }
}
