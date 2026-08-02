//! Native ngraph standard library — practical graph algorithms over mutable
//! graph handles (shortest paths, centrality, communities, flow, toposort, layouts).
//!
//! Import with `import "ngraph"` (or `import "std/ngraph"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};
use std::f64::consts::PI;
use std::rc::Rc;

const E3460_NGRAPH_ARITY: u32 = 3460;
const E3461_NGRAPH_ERROR: u32 = 3461;
const E3462_NGRAPH_TYPE: u32 = 3462;
const E3463_NGRAPH_NOT_FOUND: u32 = 3463;

#[derive(Debug, Clone)]
struct Graph {
    directed: bool,
    nodes: HashMap<String, Value>,
    adj: HashMap<String, Vec<(String, f64)>>,
}

impl Graph {
    fn new(directed: bool) -> Self {
        Self {
            directed,
            nodes: HashMap::new(),
            adj: HashMap::new(),
        }
    }

    fn ensure_node(&mut self, key: String, value: Value) {
        self.nodes.entry(key.clone()).or_insert(value);
        self.adj.entry(key).or_default();
    }

    fn add_edge(&mut self, a: String, av: Value, b: String, bv: Value, weight: f64) {
        self.ensure_node(a.clone(), av);
        self.ensure_node(b.clone(), bv);
        self.adj.entry(a.clone()).or_default().push((b.clone(), weight));
        if !self.directed {
            self.adj.entry(b).or_default().push((a, weight));
        }
    }
}

thread_local! {
    static GRAPHS: RefCell<HashMap<u64, Graph>> = RefCell::new(HashMap::new());
    static NEXT_GRAPH: RefCell<u64> = const { RefCell::new(1) };
}

fn alloc_graph(g: Graph) -> u64 {
    let id = NEXT_GRAPH.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    GRAPHS.with(|h| {
        h.borrow_mut().insert(id, g);
    });
    id
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3460_NGRAPH_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3462_NGRAPH_TYPE, msg.into())
}

fn graph_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3461_NGRAPH_ERROR, "ngraph_error", msg.into(), span)
}

fn key_of_value(v: &Value) -> String {
    format!("{}::{v:?}", v.type_name())
}

fn value_to_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Float(x) => Some(*x),
        Value::Int(n) => Some(*n as f64),
        _ => None,
    }
}

fn bool_opt(args: &[ValueRef], idx: usize, key: &str, default: bool) -> bool {
    if args.len() <= idx {
        return default;
    }
    let Value::Object(map) = &*args[idx].borrow() else {
        return default;
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Bool(b)) => b,
        Some(Value::Int(n)) => n != 0,
        _ => default,
    }
}

fn int_opt(args: &[ValueRef], idx: usize, key: &str, default: i64) -> i64 {
    if args.len() <= idx {
        return default;
    }
    let Value::Object(map) = &*args[idx].borrow() else {
        return default;
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Int(n)) => n,
        _ => default,
    }
}

fn float_opt(args: &[ValueRef], idx: usize, key: &str, default: f64) -> f64 {
    if args.len() <= idx {
        return default;
    }
    let Value::Object(map) = &*args[idx].borrow() else {
        return default;
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(v) => value_to_f64(&v).unwrap_or(default),
        _ => default,
    }
}

fn graph_handle(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<u64> {
    match &*args[idx].borrow() {
        Value::Int(id) if *id > 0 => Ok(*id as u64),
        other => Err(type_err(
            span,
            format!("{name}() expects graph handle as argument {}, got {}", idx + 1, other.type_name()),
        )),
    }
}

fn edge_weight(v: &Value, default: f64) -> Result<f64, ()> {
    match v {
        Value::Int(n) => Ok(*n as f64),
        Value::Float(x) => Ok(*x),
        Value::Nil => Ok(default),
        _ => Err(()),
    }
}

fn with_graph<R, F>(id: u64, span: Span, f: F) -> NiaoResult<R>
where
    F: FnOnce(&Graph) -> NiaoResult<R>,
{
    GRAPHS.with(|h| {
        let map = h.borrow();
        let g = map
            .get(&id)
            .ok_or_else(|| RuntimeError::at(span, E3463_NGRAPH_NOT_FOUND, "invalid graph handle"))?;
        f(g)
    })
}

fn with_graph_mut<R, F>(id: u64, span: Span, f: F) -> NiaoResult<R>
where
    F: FnOnce(&mut Graph) -> NiaoResult<R>,
{
    GRAPHS.with(|h| {
        let mut map = h.borrow_mut();
        let g = map
            .get_mut(&id)
            .ok_or_else(|| RuntimeError::at(span, E3463_NGRAPH_NOT_FOUND, "invalid graph handle"))?;
        f(g)
    })
}

fn neighbors_unique(g: &Graph, k: &str) -> Vec<(String, f64)> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    if let Some(ns) = g.adj.get(k) {
        for (n, w) in ns {
            if seen.insert(n.clone()) {
                out.push((n.clone(), *w));
            }
        }
    }
    out
}

fn bfs_path(g: &Graph, src: &str, dst: &str) -> Option<Vec<String>> {
    let mut q = VecDeque::new();
    let mut prev: HashMap<String, String> = HashMap::new();
    let mut seen = HashSet::new();
    q.push_back(src.to_string());
    seen.insert(src.to_string());
    while let Some(u) = q.pop_front() {
        if u == dst {
            break;
        }
        for (v, _) in neighbors_unique(g, &u) {
            if seen.insert(v.clone()) {
                prev.insert(v.clone(), u.clone());
                q.push_back(v);
            }
        }
    }
    if !seen.contains(dst) {
        return None;
    }
    let mut path = vec![dst.to_string()];
    let mut cur = dst.to_string();
    while cur != src {
        cur = prev.get(&cur)?.clone();
        path.push(cur.clone());
    }
    path.reverse();
    Some(path)
}

#[derive(Copy, Clone)]
struct QItem {
    d: f64,
    i: usize,
}
impl Eq for QItem {}
impl PartialEq for QItem {
    fn eq(&self, other: &Self) -> bool {
        self.d == other.d && self.i == other.i
    }
}
impl Ord for QItem {
    fn cmp(&self, other: &Self) -> Ordering {
        other.d.partial_cmp(&self.d).unwrap_or(Ordering::Equal)
    }
}
impl PartialOrd for QItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn dijkstra(g: &Graph, src: &str) -> (HashMap<String, f64>, HashMap<String, String>) {
    let nodes: Vec<String> = g.nodes.keys().cloned().collect();
    let mut idx = HashMap::new();
    for (i, n) in nodes.iter().enumerate() {
        idx.insert(n.clone(), i);
    }
    let mut dist: Vec<f64> = vec![f64::INFINITY; nodes.len()];
    let mut prev = HashMap::new();
    if let Some(&si) = idx.get(src) {
        dist[si] = 0.0;
    }
    let mut heap = BinaryHeap::new();
    if let Some(&si) = idx.get(src) {
        heap.push(QItem { d: 0.0, i: si });
    }
    while let Some(QItem { d, i }) = heap.pop() {
        if d > dist[i] {
            continue;
        }
        let u = &nodes[i];
        for (v, w) in neighbors_unique(g, u) {
            if w < 0.0 {
                continue;
            }
            let Some(&vi) = idx.get(&v) else { continue };
            let nd = d + w;
            if nd < dist[vi] {
                dist[vi] = nd;
                prev.insert(v.clone(), u.clone());
                heap.push(QItem { d: nd, i: vi });
            }
        }
    }
    let mut out = HashMap::new();
    for (i, n) in nodes.iter().enumerate() {
        out.insert(n.clone(), dist[i]);
    }
    (out, prev)
}

fn rebuild_path(prev: &HashMap<String, String>, src: &str, dst: &str) -> Option<Vec<String>> {
    if src == dst {
        return Some(vec![src.to_string()]);
    }
    let mut cur = dst.to_string();
    let mut path = vec![cur.clone()];
    while cur != src {
        cur = prev.get(&cur)?.clone();
        path.push(cur.clone());
    }
    path.reverse();
    Some(path)
}

fn kosaraju_scc(g: &Graph) -> Vec<Vec<String>> {
    fn dfs1(g: &Graph, u: &str, seen: &mut HashSet<String>, order: &mut Vec<String>) {
        if !seen.insert(u.to_string()) {
            return;
        }
        for (v, _) in neighbors_unique(g, u) {
            dfs1(g, &v, seen, order);
        }
        order.push(u.to_string());
    }
    let mut rev = Graph::new(true);
    for (k, v) in &g.nodes {
        rev.ensure_node(k.clone(), v.clone());
    }
    for (u, ns) in &g.adj {
        for (v, w) in ns {
            rev.add_edge(v.clone(), Value::String(v.clone()), u.clone(), Value::String(u.clone()), *w);
        }
    }
    let mut seen = HashSet::new();
    let mut order = Vec::new();
    for n in g.nodes.keys() {
        dfs1(g, n, &mut seen, &mut order);
    }
    fn dfs2(g: &Graph, u: &str, seen: &mut HashSet<String>, comp: &mut Vec<String>) {
        if !seen.insert(u.to_string()) {
            return;
        }
        comp.push(u.to_string());
        for (v, _) in neighbors_unique(g, u) {
            dfs2(g, &v, seen, comp);
        }
    }
    let mut seen2 = HashSet::new();
    let mut comps = Vec::new();
    while let Some(u) = order.pop() {
        if seen2.contains(&u) {
            continue;
        }
        let mut c = Vec::new();
        dfs2(&rev, &u, &mut seen2, &mut c);
        comps.push(c);
    }
    comps
}

fn to_obj_f64(map: HashMap<String, f64>, g: &Graph) -> ValueRef {
    let mut out = HashMap::new();
    for (k, v) in map {
        let node = g.nodes.get(&k).cloned().unwrap_or(Value::String(k));
        out.insert(node.to_string(), Value::Float(v).ref_cell());
    }
    Value::Object(out).ref_cell()
}

fn nodes_array(keys: &[String], g: &Graph) -> ValueRef {
    Value::Array(
        keys.iter()
            .map(|k| g.nodes.get(k).cloned().unwrap_or(Value::String(k.clone())).ref_cell())
            .collect(),
    )
    .ref_cell()
}

// >>> let g = ngraph.new()
// >>> ngraph.node_count(g)
// => 0
fn ngraph_new(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "ngraph_new", span)?;
    let directed = bool_opt(args, 0, "directed", false);
    Ok(Value::Int(alloc_graph(Graph::new(directed)) as i64).ref_cell())
}

// >>> let g = ngraph.from_edges([["a","b"],["b","c"]])
// >>> ngraph.edge_count(g)
// => 2
fn ngraph_from_edges(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ngraph_from_edges", span)?;
    let directed = bool_opt(args, 1, "directed", false);
    let mut g = Graph::new(directed);
    let Value::Array(items) = &*args[0].borrow() else {
        return Err(type_err(span, "ngraph_from_edges() expects array of edges"));
    };
    for (i, edge) in items.iter().enumerate() {
        let Value::Array(parts) = &*edge.borrow() else {
            return Err(type_err(span, format!("edge[{i}] must be an array")));
        };
        if parts.len() < 2 || parts.len() > 3 {
            return Err(type_err(span, format!("edge[{i}] must have 2 or 3 items")));
        }
        let ua = parts[0].borrow().clone();
        let va = parts[1].borrow().clone();
        let w = if parts.len() == 3 {
            edge_weight(&parts[2].borrow(), 1.0).map_err(|_| type_err(span, "edge weight must be int/float"))?
        } else {
            1.0
        };
        let uk = key_of_value(&ua);
        let vk = key_of_value(&va);
        g.add_edge(uk, ua, vk, va, w);
    }
    Ok(Value::Int(alloc_graph(g) as i64).ref_cell())
}

// >>> let g = ngraph.new()
// >>> let h = ngraph.clone(g)
// >>> ngraph.node_count(h)
// => 0
fn ngraph_clone(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ngraph_clone", span)?;
    let id = graph_handle(args, 0, "ngraph_clone", span)?;
    with_graph(id, span, |g| Ok(Value::Int(alloc_graph(g.clone()) as i64).ref_cell()))
}

// >>> let g = ngraph.new()
// >>> ngraph.clear(g)
// => true
fn ngraph_clear(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ngraph_clear", span)?;
    let id = graph_handle(args, 0, "ngraph_clear", span)?;
    with_graph_mut(id, span, |g| {
        g.nodes.clear();
        g.adj.clear();
        Ok(Value::Bool(true).ref_cell())
    })
}

// >>> let g = ngraph.new()
// >>> ngraph.add_node(g, "x")
// => true
fn ngraph_add_node(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "ngraph_add_node", span)?;
    let id = graph_handle(args, 0, "ngraph_add_node", span)?;
    let v = args[1].borrow().clone();
    let k = key_of_value(&v);
    with_graph_mut(id, span, |g| {
        g.ensure_node(k, v);
        Ok(Value::Bool(true).ref_cell())
    })
}

fn ngraph_add_nodes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "ngraph_add_nodes", span)?;
    let id = graph_handle(args, 0, "ngraph_add_nodes", span)?;
    let Value::Array(items) = &*args[1].borrow() else {
        return Err(type_err(span, "ngraph_add_nodes() expects array"));
    };
    with_graph_mut(id, span, |g| {
        for item in items {
            let v = item.borrow().clone();
            g.ensure_node(key_of_value(&v), v);
        }
        Ok(Value::Int(items.len() as i64).ref_cell())
    })
}

// >>> let g = ngraph.new()
// >>> ngraph.add_edge(g, "a", "b", 2.0)
// => true
fn ngraph_add_edge(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "ngraph_add_edge", span)?;
    let id = graph_handle(args, 0, "ngraph_add_edge", span)?;
    let ua = args[1].borrow().clone();
    let va = args[2].borrow().clone();
    let w = if args.len() == 4 {
        edge_weight(&args[3].borrow(), 1.0).map_err(|_| type_err(span, "edge weight must be int/float"))?
    } else {
        1.0
    };
    let uk = key_of_value(&ua);
    let vk = key_of_value(&va);
    with_graph_mut(id, span, |g| {
        g.add_edge(uk, ua, vk, va, w);
        Ok(Value::Bool(true).ref_cell())
    })
}

fn ngraph_add_edges(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "ngraph_add_edges", span)?;
    let id = graph_handle(args, 0, "ngraph_add_edges", span)?;
    let Value::Array(items) = &*args[1].borrow() else {
        return Err(type_err(span, "ngraph_add_edges() expects array"));
    };
    with_graph_mut(id, span, |g| {
        for (i, edge) in items.iter().enumerate() {
            let Value::Array(parts) = &*edge.borrow() else {
                return Err(type_err(span, format!("edge[{i}] must be an array")));
            };
            if parts.len() < 2 || parts.len() > 3 {
                return Err(type_err(span, format!("edge[{i}] must have 2 or 3 items")));
            }
            let ua = parts[0].borrow().clone();
            let va = parts[1].borrow().clone();
            let w = if parts.len() == 3 {
                edge_weight(&parts[2].borrow(), 1.0).map_err(|_| type_err(span, "edge weight must be int/float"))?
            } else {
                1.0
            };
            g.add_edge(key_of_value(&ua), ua, key_of_value(&va), va, w);
        }
        Ok(Value::Int(items.len() as i64).ref_cell())
    })
}

fn ngraph_has_node(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "ngraph_has_node", span)?;
    let id = graph_handle(args, 0, "ngraph_has_node", span)?;
    let k = key_of_value(&args[1].borrow());
    with_graph(id, span, |g| Ok(Value::Bool(g.nodes.contains_key(&k)).ref_cell()))
}

fn ngraph_has_edge(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 3, "ngraph_has_edge", span)?;
    let id = graph_handle(args, 0, "ngraph_has_edge", span)?;
    let uk = key_of_value(&args[1].borrow());
    let vk = key_of_value(&args[2].borrow());
    with_graph(id, span, |g| {
        let yes = g
            .adj
            .get(&uk)
            .map(|ns| ns.iter().any(|(n, _)| n == &vk))
            .unwrap_or(false);
        Ok(Value::Bool(yes).ref_cell())
    })
}

fn ngraph_nodes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ngraph_nodes", span)?;
    let id = graph_handle(args, 0, "ngraph_nodes", span)?;
    with_graph(id, span, |g| {
        let mut keys: Vec<String> = g.nodes.keys().cloned().collect();
        keys.sort();
        Ok(nodes_array(&keys, g))
    })
}

fn ngraph_edges(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ngraph_edges", span)?;
    let id = graph_handle(args, 0, "ngraph_edges", span)?;
    let weighted = args.len() == 2 && matches!(&*args[1].borrow(), Value::Bool(true));
    with_graph(id, span, |g| {
        let mut out = Vec::new();
        for (u, ns) in &g.adj {
            for (v, w) in ns {
                if !g.directed && u > v {
                    continue;
                }
                let uv = g.nodes.get(u).cloned().unwrap_or(Value::String(u.clone())).ref_cell();
                let vv = g.nodes.get(v).cloned().unwrap_or(Value::String(v.clone())).ref_cell();
                let mut row = vec![uv, vv];
                if weighted {
                    row.push(Value::Float(*w).ref_cell());
                }
                out.push(Value::Array(row).ref_cell());
            }
        }
        Ok(Value::Array(out).ref_cell())
    })
}

fn ngraph_neighbors(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "ngraph_neighbors", span)?;
    let id = graph_handle(args, 0, "ngraph_neighbors", span)?;
    let k = key_of_value(&args[1].borrow());
    with_graph(id, span, |g| {
        if !g.nodes.contains_key(&k) {
            return Ok(graph_err(span, "node not found"));
        }
        let keys: Vec<String> = neighbors_unique(g, &k).into_iter().map(|(n, _)| n).collect();
        Ok(nodes_array(&keys, g))
    })
}

fn ngraph_degree(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "ngraph_degree", span)?;
    let id = graph_handle(args, 0, "ngraph_degree", span)?;
    let k = key_of_value(&args[1].borrow());
    with_graph(id, span, |g| {
        if !g.nodes.contains_key(&k) {
            return Ok(graph_err(span, "node not found"));
        }
        let d = neighbors_unique(g, &k).len() as i64;
        Ok(Value::Int(d).ref_cell())
    })
}

fn ngraph_node_count(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ngraph_node_count", span)?;
    let id = graph_handle(args, 0, "ngraph_node_count", span)?;
    with_graph(id, span, |g| Ok(Value::Int(g.nodes.len() as i64).ref_cell()))
}

fn ngraph_edge_count(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ngraph_edge_count", span)?;
    let id = graph_handle(args, 0, "ngraph_edge_count", span)?;
    with_graph(id, span, |g| {
        let mut c: usize = g.adj.values().map(|v| v.len()).sum();
        if !g.directed {
            c /= 2;
        }
        Ok(Value::Int(c as i64).ref_cell())
    })
}

fn ngraph_is_directed(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ngraph_is_directed", span)?;
    let id = graph_handle(args, 0, "ngraph_is_directed", span)?;
    with_graph(id, span, |g| Ok(Value::Bool(g.directed).ref_cell()))
}

// >>> let g = ngraph.from_edges([["a","b"],["b","c"],["a","c"]])
// >>> ngraph.shortest_path(g, "a", "c")
// => ["a","c"]
fn ngraph_shortest_path(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "ngraph_shortest_path", span)?;
    let id = graph_handle(args, 0, "ngraph_shortest_path", span)?;
    let src = key_of_value(&args[1].borrow());
    let dst = key_of_value(&args[2].borrow());
    let weighted = args.len() == 4 && matches!(&*args[3].borrow(), Value::Bool(true));
    with_graph(id, span, |g| {
        if !g.nodes.contains_key(&src) || !g.nodes.contains_key(&dst) {
            return Ok(graph_err(span, "source or target node not found"));
        }
        let keys = if weighted {
            let (_dist, prev) = dijkstra(g, &src);
            rebuild_path(&prev, &src, &dst)
        } else {
            bfs_path(g, &src, &dst)
        };
        match keys {
            Some(path) => Ok(nodes_array(&path, g)),
            None => Ok(Value::Nil.ref_cell()),
        }
    })
}

fn ngraph_shortest_distance(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "ngraph_shortest_distance", span)?;
    let id = graph_handle(args, 0, "ngraph_shortest_distance", span)?;
    let src = key_of_value(&args[1].borrow());
    let dst = key_of_value(&args[2].borrow());
    let weighted = args.len() == 4 && matches!(&*args[3].borrow(), Value::Bool(true));
    with_graph(id, span, |g| {
        if !g.nodes.contains_key(&src) || !g.nodes.contains_key(&dst) {
            return Ok(graph_err(span, "source or target node not found"));
        }
        if weighted {
            let (dist, _) = dijkstra(g, &src);
            let d = dist.get(&dst).copied().unwrap_or(f64::INFINITY);
            if d.is_finite() {
                Ok(Value::Float(d).ref_cell())
            } else {
                Ok(Value::Nil.ref_cell())
            }
        } else {
            match bfs_path(g, &src, &dst) {
                Some(path) => Ok(Value::Int((path.len().saturating_sub(1)) as i64).ref_cell()),
                None => Ok(Value::Nil.ref_cell()),
            }
        }
    })
}

fn ngraph_all_pairs_shortest_paths(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ngraph_all_pairs_shortest_paths", span)?;
    let id = graph_handle(args, 0, "ngraph_all_pairs_shortest_paths", span)?;
    let weighted = args.len() == 2 && matches!(&*args[1].borrow(), Value::Bool(true));
    with_graph(id, span, |g| {
        let mut outer = HashMap::new();
        let mut keys: Vec<String> = g.nodes.keys().cloned().collect();
        keys.sort();
        for src in &keys {
            let mut inner = HashMap::new();
            if weighted {
                let (dist, _) = dijkstra(g, src);
                for dst in &keys {
                    let d = dist.get(dst).copied().unwrap_or(f64::INFINITY);
                    if d.is_finite() {
                        inner.insert(dst.clone(), Value::Float(d).ref_cell());
                    }
                }
            } else {
                for dst in &keys {
                    if let Some(path) = bfs_path(g, src, dst) {
                        inner.insert(dst.clone(), Value::Int((path.len() - 1) as i64).ref_cell());
                    }
                }
            }
            outer.insert(src.clone(), Value::Object(inner).ref_cell());
        }
        Ok(Value::Object(outer).ref_cell())
    })
}

fn ngraph_closeness_centrality(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ngraph_closeness_centrality", span)?;
    let id = graph_handle(args, 0, "ngraph_closeness_centrality", span)?;
    with_graph(id, span, |g| {
        let mut out = HashMap::new();
        let n = g.nodes.len() as f64;
        for src in g.nodes.keys() {
            let mut sum = 0.0;
            let mut reachable = 0.0;
            let (dist, _) = dijkstra(g, src);
            for (dst, d) in dist {
                if dst == *src || !d.is_finite() {
                    continue;
                }
                sum += d;
                reachable += 1.0;
            }
            let score = if sum > 0.0 && n > 1.0 {
                (reachable / sum) * (reachable / (n - 1.0))
            } else {
                0.0
            };
            out.insert(src.clone(), score);
        }
        Ok(to_obj_f64(out, g))
    })
}

fn ngraph_betweenness_centrality(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ngraph_betweenness_centrality", span)?;
    let id = graph_handle(args, 0, "ngraph_betweenness_centrality", span)?;
    let normalized = args.len() == 2 && matches!(&*args[1].borrow(), Value::Bool(true));
    with_graph(id, span, |g| {
        let nodes: Vec<String> = g.nodes.keys().cloned().collect();
        let mut cb: HashMap<String, f64> = nodes.iter().map(|n| (n.clone(), 0.0)).collect();
        for s in &nodes {
            let mut stack = Vec::new();
            let mut pred: HashMap<String, Vec<String>> = HashMap::new();
            let mut sigma: HashMap<String, f64> = nodes.iter().map(|n| (n.clone(), 0.0)).collect();
            let mut dist: HashMap<String, i64> = nodes.iter().map(|n| (n.clone(), -1)).collect();
            sigma.insert(s.clone(), 1.0);
            dist.insert(s.clone(), 0);
            let mut q = VecDeque::new();
            q.push_back(s.clone());
            while let Some(v) = q.pop_front() {
                stack.push(v.clone());
                for (w, _) in neighbors_unique(g, &v) {
                    if *dist.get(&w).unwrap_or(&-1) < 0 {
                        q.push_back(w.clone());
                        let dv = *dist.get(&v).unwrap_or(&0);
                        dist.insert(w.clone(), dv + 1);
                    }
                    if *dist.get(&w).unwrap_or(&-1) == *dist.get(&v).unwrap_or(&0) + 1 {
                        let sw = *sigma.get(&w).unwrap_or(&0.0);
                        let sv = *sigma.get(&v).unwrap_or(&0.0);
                        sigma.insert(w.clone(), sw + sv);
                        pred.entry(w.clone()).or_default().push(v.clone());
                    }
                }
            }
            let mut delta: HashMap<String, f64> = nodes.iter().map(|n| (n.clone(), 0.0)).collect();
            while let Some(w) = stack.pop() {
                if let Some(ps) = pred.get(&w) {
                    for v in ps {
                        let sv = *sigma.get(v).unwrap_or(&0.0);
                        let sw = *sigma.get(&w).unwrap_or(&1.0);
                        if sw > 0.0 {
                            let inc = (sv / sw) * (1.0 + *delta.get(&w).unwrap_or(&0.0));
                            let dv = *delta.get(v).unwrap_or(&0.0) + inc;
                            delta.insert(v.clone(), dv);
                        }
                    }
                }
                if &w != s {
                    let cw = *cb.get(&w).unwrap_or(&0.0) + *delta.get(&w).unwrap_or(&0.0);
                    cb.insert(w, cw);
                }
            }
        }
        if !g.directed {
            for v in cb.values_mut() {
                *v /= 2.0;
            }
        }
        if normalized {
            let n = nodes.len() as f64;
            if n > 2.0 {
                let scale = if g.directed {
                    1.0 / ((n - 1.0) * (n - 2.0))
                } else {
                    2.0 / ((n - 1.0) * (n - 2.0))
                };
                for v in cb.values_mut() {
                    *v *= scale;
                }
            }
        }
        Ok(to_obj_f64(cb, g))
    })
}

fn ngraph_pagerank(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ngraph_pagerank", span)?;
    let id = graph_handle(args, 0, "ngraph_pagerank", span)?;
    let alpha = float_opt(args, 1, "alpha", 0.85).clamp(0.0, 0.9999);
    let tol = float_opt(args, 1, "tol", 1e-6).max(1e-12);
    let max_iter = int_opt(args, 1, "max_iter", 100).max(1) as usize;
    with_graph(id, span, |g| {
        let nodes: Vec<String> = g.nodes.keys().cloned().collect();
        let n = nodes.len();
        if n == 0 {
            return Ok(Value::Object(HashMap::new()).ref_cell());
        }
        let mut idx = HashMap::new();
        for (i, k) in nodes.iter().enumerate() {
            idx.insert(k.clone(), i);
        }
        let mut rank = vec![1.0 / n as f64; n];
        let base = (1.0 - alpha) / n as f64;
        for _ in 0..max_iter {
            let mut next = vec![base; n];
            let mut sink = 0.0;
            for (i, u) in nodes.iter().enumerate() {
                let ns = neighbors_unique(g, u);
                if ns.is_empty() {
                    sink += rank[i];
                    continue;
                }
                let share = rank[i] / ns.len() as f64;
                for (v, _) in ns {
                    if let Some(&j) = idx.get(&v) {
                        next[j] += alpha * share;
                    }
                }
            }
            let sink_share = alpha * sink / n as f64;
            for v in &mut next {
                *v += sink_share;
            }
            let diff: f64 = rank.iter().zip(next.iter()).map(|(a, b)| (a - b).abs()).sum();
            rank = next;
            if diff < tol {
                break;
            }
        }
        let mut out = HashMap::new();
        for (i, nkey) in nodes.iter().enumerate() {
            out.insert(nkey.clone(), rank[i]);
        }
        Ok(to_obj_f64(out, g))
    })
}

fn ngraph_connected_components(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ngraph_connected_components", span)?;
    let id = graph_handle(args, 0, "ngraph_connected_components", span)?;
    with_graph(id, span, |g| {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        let mut keys: Vec<String> = g.nodes.keys().cloned().collect();
        keys.sort();
        for s in keys {
            if seen.contains(&s) {
                continue;
            }
            let mut q = VecDeque::new();
            let mut comp = Vec::new();
            seen.insert(s.clone());
            q.push_back(s);
            while let Some(u) = q.pop_front() {
                comp.push(u.clone());
                for (v, _) in neighbors_unique(g, &u) {
                    if seen.insert(v.clone()) {
                        q.push_back(v);
                    }
                }
            }
            out.push(nodes_array(&comp, g));
        }
        Ok(Value::Array(out).ref_cell())
    })
}

fn ngraph_strongly_connected_components(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ngraph_strongly_connected_components", span)?;
    let id = graph_handle(args, 0, "ngraph_strongly_connected_components", span)?;
    with_graph(id, span, |g| {
        let comps = if g.directed {
            kosaraju_scc(g)
        } else {
            let mut seen = HashSet::new();
            let mut out = Vec::new();
            for n in g.nodes.keys() {
                if seen.contains(n) {
                    continue;
                }
                let mut q = VecDeque::new();
                let mut c = Vec::new();
                seen.insert(n.clone());
                q.push_back(n.clone());
                while let Some(u) = q.pop_front() {
                    c.push(u.clone());
                    for (v, _) in neighbors_unique(g, &u) {
                        if seen.insert(v.clone()) {
                            q.push_back(v);
                        }
                    }
                }
                out.push(c);
            }
            out
        };
        Ok(Value::Array(comps.iter().map(|c| nodes_array(c, g)).collect()).ref_cell())
    })
}

fn ngraph_toposort(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ngraph_toposort", span)?;
    let id = graph_handle(args, 0, "ngraph_toposort", span)?;
    with_graph(id, span, |g| {
        let mut indeg: HashMap<String, usize> = g.nodes.keys().map(|k| (k.clone(), 0)).collect();
        for ns in g.adj.values() {
            for (v, _) in ns {
                *indeg.entry(v.clone()).or_insert(0) += 1;
            }
        }
        let mut q: VecDeque<String> = indeg
            .iter()
            .filter_map(|(k, d)| if *d == 0 { Some(k.clone()) } else { None })
            .collect();
        let mut out = Vec::new();
        while let Some(u) = q.pop_front() {
            out.push(u.clone());
            for (v, _) in neighbors_unique(g, &u) {
                let d = indeg.entry(v.clone()).or_insert(0);
                if *d > 0 {
                    *d -= 1;
                    if *d == 0 {
                        q.push_back(v);
                    }
                }
            }
        }
        if out.len() != g.nodes.len() {
            return Ok(graph_err(span, "toposort requires a DAG (cycle detected)"));
        }
        Ok(nodes_array(&out, g))
    })
}

fn ngraph_max_flow(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 3, "ngraph_max_flow", span)?;
    let id = graph_handle(args, 0, "ngraph_max_flow", span)?;
    let s = key_of_value(&args[1].borrow());
    let t = key_of_value(&args[2].borrow());
    with_graph(id, span, |g| {
        if !g.nodes.contains_key(&s) || !g.nodes.contains_key(&t) {
            return Ok(graph_err(span, "source or sink node not found"));
        }
        let mut cap: HashMap<(String, String), f64> = HashMap::new();
        let mut neigh: HashMap<String, Vec<String>> = HashMap::new();
        for (u, ns) in &g.adj {
            for (v, w) in ns {
                let c = if *w < 0.0 { 0.0 } else { *w };
                *cap.entry((u.clone(), v.clone())).or_insert(0.0) += c;
                neigh.entry(u.clone()).or_default().push(v.clone());
                neigh.entry(v.clone()).or_default().push(u.clone());
                cap.entry((v.clone(), u.clone())).or_insert(0.0);
            }
        }
        let mut flow = 0.0;
        loop {
            let mut q = VecDeque::new();
            let mut prev: HashMap<String, String> = HashMap::new();
            q.push_back(s.clone());
            let mut seen = HashSet::new();
            seen.insert(s.clone());
            while let Some(u) = q.pop_front() {
                if u == t {
                    break;
                }
                for v in neigh.get(&u).cloned().unwrap_or_default() {
                    let c = *cap.get(&(u.clone(), v.clone())).unwrap_or(&0.0);
                    if c > 1e-12 && seen.insert(v.clone()) {
                        prev.insert(v.clone(), u.clone());
                        q.push_back(v);
                    }
                }
            }
            if !seen.contains(&t) {
                break;
            }
            let mut aug = f64::INFINITY;
            let mut v = t.clone();
            while v != s {
                let Some(u) = prev.get(&v).cloned() else { break };
                let c = *cap.get(&(u.clone(), v.clone())).unwrap_or(&0.0);
                aug = aug.min(c);
                v = u;
            }
            if !aug.is_finite() || aug <= 0.0 {
                break;
            }
            let mut v2 = t.clone();
            while v2 != s {
                let u = prev.get(&v2).cloned().unwrap_or_else(|| s.clone());
                *cap.entry((u.clone(), v2.clone())).or_insert(0.0) -= aug;
                *cap.entry((v2.clone(), u.clone())).or_insert(0.0) += aug;
                v2 = u;
            }
            flow += aug;
        }
        let mut obj = HashMap::new();
        obj.insert("value".to_string(), Value::Float(flow).ref_cell());
        Ok(Value::Object(obj).ref_cell())
    })
}

fn ngraph_label_propagation_communities(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ngraph_label_propagation_communities", span)?;
    let id = graph_handle(args, 0, "ngraph_label_propagation_communities", span)?;
    let max_iter = if args.len() == 2 {
        match &*args[1].borrow() {
            Value::Int(n) if *n > 0 => *n as usize,
            _ => 20,
        }
    } else {
        20
    };
    with_graph(id, span, |g| {
        let mut keys: Vec<String> = g.nodes.keys().cloned().collect();
        keys.sort();
        let mut label: HashMap<String, String> = keys.iter().map(|k| (k.clone(), k.clone())).collect();
        for _ in 0..max_iter {
            let mut changed = false;
            for u in &keys {
                let mut freq: HashMap<String, usize> = HashMap::new();
                for (v, _) in neighbors_unique(g, u) {
                    let lv = label.get(&v).cloned().unwrap_or(v);
                    *freq.entry(lv).or_insert(0) += 1;
                }
                if freq.is_empty() {
                    continue;
                }
                let mut best = label.get(u).cloned().unwrap_or_else(|| u.clone());
                let mut best_n = 0usize;
                for (k, n) in freq {
                    if n > best_n || (n == best_n && k < best) {
                        best = k;
                        best_n = n;
                    }
                }
                if label.get(u).map(|x| x != &best).unwrap_or(true) {
                    label.insert(u.clone(), best);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let mut groups: HashMap<String, Vec<String>> = HashMap::new();
        for n in &keys {
            groups
                .entry(label.get(n).cloned().unwrap_or_else(|| n.clone()))
                .or_default()
                .push(n.clone());
        }
        let mut out = Vec::new();
        for mut ns in groups.into_values() {
            ns.sort();
            out.push(nodes_array(&ns, g));
        }
        Ok(Value::Array(out).ref_cell())
    })
}

fn ngraph_circular_layout(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ngraph_circular_layout", span)?;
    let id = graph_handle(args, 0, "ngraph_circular_layout", span)?;
    with_graph(id, span, |g| {
        let mut keys: Vec<String> = g.nodes.keys().cloned().collect();
        keys.sort();
        let n = keys.len().max(1);
        let mut out = HashMap::new();
        for (i, k) in keys.iter().enumerate() {
            let ang = (2.0 * PI * i as f64) / n as f64;
            let mut p = HashMap::new();
            p.insert("x".to_string(), Value::Float(ang.cos()).ref_cell());
            p.insert("y".to_string(), Value::Float(ang.sin()).ref_cell());
            let label = g.nodes.get(k).cloned().unwrap_or(Value::String(k.clone())).to_string();
            out.insert(label, Value::Object(p).ref_cell());
        }
        Ok(Value::Object(out).ref_cell())
    })
}

fn ngraph_spring_layout(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ngraph_spring_layout", span)?;
    let id = graph_handle(args, 0, "ngraph_spring_layout", span)?;
    let iters = int_opt(args, 1, "iterations", 50).clamp(1, 500) as usize;
    let step = float_opt(args, 1, "step", 0.05).clamp(0.001, 1.0);
    with_graph(id, span, |g| {
        let mut keys: Vec<String> = g.nodes.keys().cloned().collect();
        keys.sort();
        let n = keys.len();
        if n == 0 {
            return Ok(Value::Object(HashMap::new()).ref_cell());
        }
        let mut pos: Vec<(f64, f64)> = (0..n)
            .map(|i| {
                let ang = (2.0 * PI * i as f64) / n as f64;
                (ang.cos(), ang.sin())
            })
            .collect();
        let mut idx = HashMap::new();
        for (i, k) in keys.iter().enumerate() {
            idx.insert(k.clone(), i);
        }
        for _ in 0..iters {
            let mut disp = vec![(0.0, 0.0); n];
            for i in 0..n {
                for j in (i + 1)..n {
                    let dx = pos[i].0 - pos[j].0;
                    let dy = pos[i].1 - pos[j].1;
                    let d2 = dx * dx + dy * dy + 1e-6;
                    let f = 0.01 / d2;
                    disp[i].0 += dx * f;
                    disp[i].1 += dy * f;
                    disp[j].0 -= dx * f;
                    disp[j].1 -= dy * f;
                }
            }
            for (u, ns) in &g.adj {
                let Some(&i) = idx.get(u) else { continue };
                for (v, _) in ns {
                    let Some(&j) = idx.get(v) else { continue };
                    if i == j {
                        continue;
                    }
                    let dx = pos[j].0 - pos[i].0;
                    let dy = pos[j].1 - pos[i].1;
                    disp[i].0 += dx * 0.01;
                    disp[i].1 += dy * 0.01;
                }
            }
            for i in 0..n {
                pos[i].0 += disp[i].0 * step;
                pos[i].1 += disp[i].1 * step;
            }
        }
        let mut out = HashMap::new();
        for (i, k) in keys.iter().enumerate() {
            let mut p = HashMap::new();
            p.insert("x".to_string(), Value::Float(pos[i].0).ref_cell());
            p.insert("y".to_string(), Value::Float(pos[i].1).ref_cell());
            let label = g.nodes.get(k).cloned().unwrap_or(Value::String(k.clone())).to_string();
            out.insert(label, Value::Object(p).ref_cell());
        }
        Ok(Value::Object(out).ref_cell())
    })
}

macro_rules! ngraph_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ngraph_fns![
    ("ngraph_new", "new", ngraph_new),
    ("ngraph_from_edges", "from_edges", ngraph_from_edges),
    ("ngraph_clone", "clone", ngraph_clone),
    ("ngraph_clear", "clear", ngraph_clear),
    ("ngraph_add_node", "add_node", ngraph_add_node),
    ("ngraph_add_nodes", "add_nodes", ngraph_add_nodes),
    ("ngraph_add_edge", "add_edge", ngraph_add_edge),
    ("ngraph_add_edges", "add_edges", ngraph_add_edges),
    ("ngraph_has_node", "has_node", ngraph_has_node),
    ("ngraph_has_edge", "has_edge", ngraph_has_edge),
    ("ngraph_nodes", "nodes", ngraph_nodes),
    ("ngraph_edges", "edges", ngraph_edges),
    ("ngraph_neighbors", "neighbors", ngraph_neighbors),
    ("ngraph_degree", "degree", ngraph_degree),
    ("ngraph_node_count", "node_count", ngraph_node_count),
    ("ngraph_edge_count", "edge_count", ngraph_edge_count),
    ("ngraph_is_directed", "is_directed", ngraph_is_directed),
    ("ngraph_shortest_path", "shortest_path", ngraph_shortest_path),
    ("ngraph_shortest_distance", "shortest_distance", ngraph_shortest_distance),
    ("ngraph_all_pairs_shortest_paths", "all_pairs_shortest_paths", ngraph_all_pairs_shortest_paths),
    ("ngraph_betweenness_centrality", "betweenness_centrality", ngraph_betweenness_centrality),
    ("ngraph_closeness_centrality", "closeness_centrality", ngraph_closeness_centrality),
    ("ngraph_pagerank", "pagerank", ngraph_pagerank),
    ("ngraph_connected_components", "connected_components", ngraph_connected_components),
    ("ngraph_strongly_connected_components", "strongly_connected_components", ngraph_strongly_connected_components),
    ("ngraph_toposort", "toposort", ngraph_toposort),
    ("ngraph_max_flow", "max_flow", ngraph_max_flow),
    ("ngraph_label_propagation_communities", "label_propagation_communities", ngraph_label_propagation_communities),
    ("ngraph_circular_layout", "circular_layout", ngraph_circular_layout),
    ("ngraph_spring_layout", "spring_layout", ngraph_spring_layout),
];

pub const MODULE_NAME: &str = "ngraph";
pub const MODULE_PATHS: &[&str] = &["ngraph", "std/ngraph"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_graph() -> Graph {
        let mut g = Graph::new(false);
        g.add_edge(
            "String::a".to_string(),
            Value::String("a".to_string()),
            "String::b".to_string(),
            Value::String("b".to_string()),
            1.0,
        );
        g.add_edge(
            "String::b".to_string(),
            Value::String("b".to_string()),
            "String::c".to_string(),
            Value::String("c".to_string()),
            2.0,
        );
        g
    }

    #[test]
    fn bfs_path_works() {
        let g = mk_graph();
        let p = bfs_path(&g, "String::a", "String::c").expect("path");
        assert_eq!(p, vec!["String::a", "String::b", "String::c"]);
    }

    #[test]
    fn dijkstra_weighted_distance() {
        let g = mk_graph();
        let (dist, _) = dijkstra(&g, "String::a");
        assert!((dist["String::c"] - 3.0).abs() < 1e-9);
    }

    #[test]
    fn topo_cycle_detect() {
        let mut g = Graph::new(true);
        g.add_edge("String::a".into(), Value::String("a".into()), "String::b".into(), Value::String("b".into()), 1.0);
        g.add_edge("String::b".into(), Value::String("b".into()), "String::a".into(), Value::String("a".into()), 1.0);
        let mut indeg: HashMap<String, usize> = g.nodes.keys().map(|k| (k.clone(), 0)).collect();
        for ns in g.adj.values() {
            for (v, _) in ns {
                *indeg.entry(v.clone()).or_insert(0) += 1;
            }
        }
        assert!(indeg.values().all(|d| *d > 0));
    }
}
