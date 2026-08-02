# NGRAPH — Graph Algorithms for Niao

`ngraph` is a native graph toolkit for production workloads: shortest paths,
centrality, communities, max-flow, topological sort, and 2D layouts.
It is an extended graph layer above `dsa` graph primitives, in a
NetworkX-style surface adapted to Niao's object namespace style.

Import:

```niao
import "ngraph"
// or
import "std/ngraph"
```

## Quickstart

```niao
import "ngraph"

let g = ngraph.new({directed: false})
ngraph.add_edges(g, [
  ["A", "B", 1.0],
  ["B", "C", 2.0],
  ["A", "C", 4.0],
  ["C", "D", 1.0]
])

print(ngraph.shortest_path(g, "A", "D", true))       // ["A","B","C","D"]
print(ngraph.shortest_distance(g, "A", "D", true))   // 4.0
print(ngraph.pagerank(g))
```

## API

### Construction and mutation

- `ngraph.new(opts?) -> graph_handle`
- `ngraph.from_edges(edges, opts?) -> graph_handle`
- `ngraph.clone(graph) -> graph_handle`
- `ngraph.clear(graph) -> true`
- `ngraph.add_node(graph, node) -> true`
- `ngraph.add_nodes(graph, nodes) -> int`
- `ngraph.add_edge(graph, u, v, weight?) -> true`
- `ngraph.add_edges(graph, edges) -> int`

`opts`:
- `directed: bool` (default `false`)

### Topology queries

- `ngraph.has_node(graph, node) -> bool`
- `ngraph.has_edge(graph, u, v) -> bool`
- `ngraph.nodes(graph) -> array`
- `ngraph.edges(graph, weighted?) -> array`
- `ngraph.neighbors(graph, node) -> array`
- `ngraph.degree(graph, node) -> int`
- `ngraph.node_count(graph) -> int`
- `ngraph.edge_count(graph) -> int`
- `ngraph.is_directed(graph) -> bool`

### Path and distance algorithms

- `ngraph.shortest_path(graph, source, target, weight?) -> array|nil|ngraph_error`
- `ngraph.shortest_distance(graph, source, target, weight?) -> int|float|nil|ngraph_error`
- `ngraph.all_pairs_shortest_paths(graph, weight?) -> object`

When `weight` is `true`, Dijkstra is used over edge weights.
When `weight` is omitted/false, BFS hop-distance is used.

### Centrality and ranking

- `ngraph.betweenness_centrality(graph, normalized?) -> object`
- `ngraph.closeness_centrality(graph) -> object`
- `ngraph.pagerank(graph, opts?) -> object`

`pagerank` opts:
- `alpha` (default `0.85`)
- `tol` (default `1e-6`)
- `max_iter` (default `100`)

### Components, DAGs, communities

- `ngraph.connected_components(graph) -> array`
- `ngraph.strongly_connected_components(graph) -> array`
- `ngraph.toposort(graph) -> array|ngraph_error`
- `ngraph.label_propagation_communities(graph, max_iter?) -> array`

### Flow and layouts

- `ngraph.max_flow(graph, source, sink) -> {value}`
- `ngraph.circular_layout(graph) -> {node: {x, y}}`
- `ngraph.spring_layout(graph, opts?) -> {node: {x, y}}`

`spring_layout` opts:
- `iterations` (default `50`)
- `step` (default `0.05`)

## Error codes

| Code | Meaning |
|------|---------|
| 3460 | Wrong argument count |
| 3461 | Algorithm/runtime recoverable error (`ngraph_error`) |
| 3462 | Wrong argument type |
| 3463 | Invalid graph handle |

Use `is_error(v)` and `error_message(v)` for recoverable results like cycle
detection in `toposort` or missing nodes in path queries.
