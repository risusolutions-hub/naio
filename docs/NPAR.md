# npar — explicit parallel array ops

Rayon-backed parallel operations on packed `IntArray` / `FloatArray`, with optional custom thread-pool sizing via `set_threads`.

## Import

```niao
import "npar"
```

Paths `import "std/npar"` and `import "npar"` are equivalent. Flat builtins (`npar_sum`, `npar_set_threads`, …) are also available globally after import.

## Quick start

```niao
import "npar"
import "ncl"

npar.set_threads(4)
print(npar.threads())          // 4

let xs = ncl.array([1, 2, 3, 4, 5, 6, 7, 8])
let ys = ncl.array([8, 7, 6, 5, 4, 3, 2, 1])

print(npar.sum(xs))            // 36
print(npar.add(xs, ys))        // int_array[9,9,...]
print(npar.map(xs, "double"))  // int_array[2,4,6,...]
print(npar.dot(xs, ys))         // 120
```

## Functions

| Method | Description |
|--------|-------------|
| `npar.set_threads(n)` | Build a custom rayon pool with `n` threads (must be ≥ 1). Returns the count on success. |
| `npar.threads()` | Active rayon thread count (custom pool or global default). |
| `npar.sum(array)` | Parallel sum over `IntArray` or `FloatArray`. |
| `npar.add(a, b)` | Parallel element-wise add. |
| `npar.mul(a, b)` | Parallel element-wise multiply. |
| `npar.dot(a, b)` | Parallel dot product. |
| `npar.map(array, op)` | Parallel unary map with a built-in op name. |

### Built-in map ops

| Op | `IntArray` | `FloatArray` |
|----|------------|--------------|
| `id` | pass-through | pass-through |
| `neg` | unary negation | unary negation |
| `abs` | saturating abs | `abs()` |
| `double` | ×2 (wrapping) | — |
| `square` | — | × self |
| `sqrt` | — | `sqrt()` |

Unknown ops return a catchable `npar_error`.

## Errors

| Code | Meaning |
|------|---------|
| 3390 | Wrong argument count. |
| 3391 | Pool / length / op error (catchable `npar_error`). |
| 3392 | Wrong argument type (hard error). |

## See also

- `nsimd` — single-threaded unrolled SIMD-friendly kernels.
- `ncl.parallel_sum` — threshold-based auto parallel inside NCL.
