# nlazy — fused lazy array pipelines

Build deferred map / filter / take pipelines over packed arrays. Stages fuse at `collect` or `sum` without materializing intermediates.

Native code cannot call Niao functions, so map and filter stages use built-in op names (same pattern as `npipe`).

## Import

```niao
import "nlazy"
```

Paths `import "std/nlazy"` and `import "nlazy"` are equivalent. Flat builtins (`nlazy_from`, `nlazy_map`, …) are also available globally after import.

## Quick start

```niao
import "nlazy"
import "ncl"

let data = ncl.array([1, 2, 3, 4, 5, 6, 7, 8, 9, 10])
let p = nlazy.from(data)

nlazy.filter(p, "even")
nlazy.map(p, "double")
nlazy.take(p, 3)

print(nlazy.sum(p))            // 4 + 8 + 12 = 24
print(nlazy.collect(p))        // int_array[4, 8, 12]
print(nlazy.describe(p))       // nlazy[filter(even) → map(double) → take(3)]
nlazy.close(p)
```

## Functions

| Method | Description |
|--------|-------------|
| `nlazy.from(array)` | Create a pipeline handle from `IntArray` or `FloatArray`. |
| `nlazy.map(h, op)` | Append a map stage. Returns `nil`. |
| `nlazy.filter(h, pred)` | Append a filter stage. Returns `nil`. |
| `nlazy.take(h, n)` | Keep at most `n` elements after prior stages. `n` must be ≥ 0. |
| `nlazy.collect(h)` | Materialize the fused pipeline into a new packed array. |
| `nlazy.sum(h)` | Fused sum without building the full output vector. |
| `nlazy.len(h)` | Count of elements after fusion (may execute the pipeline). |
| `nlazy.describe(h)` | Human-readable stage list. |
| `nlazy.close(h)` | Drop the handle → `true` if it existed. |

### Map ops

| Op | `IntArray` | `FloatArray` |
|----|------------|--------------|
| `id` | ✓ | ✓ |
| `neg` | ✓ | ✓ |
| `abs` | ✓ | ✓ |
| `double` | ×2 | — |
| `square` | — | × self |
| `sqrt` | — | `sqrt()` |

### Filter predicates

| Pred | `IntArray` | `FloatArray` |
|------|------------|--------------|
| `positive` | `> 0` | `> 0` |
| `negative` | `< 0` | `< 0` |
| `nonzero` | `!= 0` | `!= 0` |
| `even` | `% 2 == 0` | — |
| `odd` | `% 2 != 0` | — |

## Errors

| Code | Meaning |
|------|---------|
| 3410 | Wrong argument count. |
| 3411 | Missing source, bad op/pred, or take count (catchable `nlazy_error`). |
| 3412 | Wrong argument type (hard error). |
| 3413 | Invalid or closed handle (catchable `nlazy_error`). |

## See also

- `npipe` — value-level op pipelines (not array-specific).
- `nsimd` / `npar` — eager numeric kernels on packed arrays.
