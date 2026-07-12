# npipe — typed step pipelines

Compose short transform pipelines from a built-in op registry. Native code cannot call Niao functions, so each stage is a named op (`id`, `len`, `type`, `keys`, `not_nil`, `str`, `abs`) applied left-to-right.

## Import

```niao
import "npipe"
```

Paths `import "std/npipe"` and `import "npipe"` are equivalent. Flat builtins (`npipe_new`, `npipe_add`, …) are also available globally after import.

## Quick start

```niao
import "npipe"

let p = npipe.new()
npipe.add(p, "abs")
npipe.add(p, "str")
print(npipe.run(p, -42))       // "42"
print(npipe.describe(p))       // npipe[abs → str]
print(npipe.steps(p))          // ["abs", "str"]
npipe.close(p)

// One-shot without a handle
print(npipe.run_ops(["type", "len"], "hello"))  // 6  (type → "string", len → 6)
```

## Functions

| Method | Description |
|--------|-------------|
| `npipe.new()` | Create an empty pipeline → handle (`int`). |
| `npipe.add(h, op_name)` | Append a built-in op. Returns `nil`. |
| `npipe.run(h, input)` | Apply stored ops in order to `input`. |
| `npipe.steps(h)` | Array of op name strings. |
| `npipe.clear(h)` | Remove all steps. Returns `nil`. |
| `npipe.close(h)` | Drop the handle → `true` if it existed. |
| `npipe.run_ops(ops_array, input)` | Apply ops without allocating a handle. |
| `npipe.describe(h)` | Human-readable string, e.g. `npipe[abs → str]`. |

## Built-in ops

| Op | Result |
|----|--------|
| `id` | Pass-through (same value). |
| `len` | Length as `int` (string, array, object, native DS). |
| `type` | Type name string (`int`, `string`, `object`, …). |
| `keys` | Sorted array of object key strings. |
| `not_nil` | `true` unless the value is `nil`. |
| `str` | Display string (`Value::to_string`). |
| `abs` | Absolute value for `int` / `float` / `bigint`. |

Unknown op names on `add` / `run_ops` raise a hard error. Op application failures (e.g. `abs` on a string, `keys` on a non-object) return a catchable `npipe_error` value.

## Errors

| Code | Meaning |
|------|---------|
| 3130 | Wrong argument count. |
| 3131 | Pipeline / op error (unknown op hard-throws; apply failures are catchable `npipe_error`). |
| 3132 | Wrong argument type. |
| 3133 | Invalid or closed pipeline handle (catchable `npipe_error`). |

## See also

- `nmath` — numeric helpers including `abs`.
- `nvalid` — declarative validation of values.
