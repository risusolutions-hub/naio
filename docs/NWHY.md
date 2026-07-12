# nwhy standard library

Value lineage / provenance tracking. Wrap any Niao value in a handle with a label, derive new values from parent handles, then explain or graph how a result was produced.

## Import

```niao
import "nwhy"
```

Paths `import "std/nwhy"` and `import "nwhy"` are equivalent. Flat builtins (`nwhy_track`, `nwhy_derive`, …) are also available globally after import.

## Quick start

```niao
import "nwhy"

let x = nwhy.track(2, "x")
let y = nwhy.track(3, "y")
let sum = nwhy.derive([x, y], 5, "add")

print(nwhy.value(sum))     // 5
print(nwhy.explain(sum))   // add ← x, y
print(nwhy.graph(sum))     // {nodes: [...], edges: [...]}
print(nwhy.same(x, y))     // false

nwhy.close(sum)
nwhy.close(x)
nwhy.close(y)
```

## API

| Method | Description |
|--------|-------------|
| `nwhy.track(value, label)` | Root node: store `value` with `label`, no parents. Returns handle. |
| `nwhy.derive(inputs, value, op_label)` | Child node from `inputs` (array of handles) with result `value` and `op_label`. |
| `nwhy.value(h)` | Underlying stored value. |
| `nwhy.label(h)` | Node label string. |
| `nwhy.parents(h)` | Array of parent handle ints (empty for roots). |
| `nwhy.explain(h)` | Human string: root → `"label"`; derived → `"op ← parent labels..."`. |
| `nwhy.graph(h)` | Ancestor DAG as `{nodes:[{id,label}], edges:[{from,to}]}`. |
| `nwhy.same(a, b)` | Compare underlying values (`==`-style, with Debug/type fallback). |
| `nwhy.close(h)` | Drop the node; `true` if it existed. |

Handles are integers. Closing a handle does not close its parents or children.

## Errors

| Code | Meaning |
|------|---------|
| 2970 | Wrong argument count. |
| 2971 | Operation failed (e.g. empty label). |
| 2972 | Type mismatch (hard error). |
| 2973 | Invalid or closed lineage handle (catchable `error`). |
