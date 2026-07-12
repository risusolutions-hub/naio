# nsimd — autovectorized array kernels

Unrolled `chunks_exact(8)` f64/i64 kernels on packed `FloatArray` / `IntArray`. LLVM autovectorizes the inner loops; no explicit intrinsics required.

## Import

```niao
import "nsimd"
```

Paths `import "std/nsimd"` and `import "nsimd"` are equivalent. Flat builtins (`nsimd_sum`, `nsimd_add`, …) are also available globally after import.

## Quick start

```niao
import "nsimd"
import "ncl"

let a = ncl.array([1, 2, 3, 4, 5, 6, 7, 8])
let b = ncl.array([8, 7, 6, 5, 4, 3, 2, 1])

print(nsimd.sum(a))              // 36
print(nsimd.dot(a, b))           // 120
print(nsimd.add(a, b))         // int_array[9,9,...]
print(nsimd.scale(a, 2))         // int_array[2,4,6,...]
print(nsimd.min(a))              // 1
print(nsimd.max(a))              // 8
```

## Functions

| Method | Description |
|--------|-------------|
| `nsimd.sum(array)` | Sum of `IntArray` or `FloatArray`. Empty array → `0` / `0.0`. |
| `nsimd.add(a, b)` | Element-wise add (wrapping for int). Lengths must match. |
| `nsimd.sub(a, b)` | Element-wise subtract. |
| `nsimd.mul(a, b)` | Element-wise multiply (wrapping for int). |
| `nsimd.dot(a, b)` | Dot product scalar. |
| `nsimd.scale(array, k)` | Multiply every element by scalar `k` (`int` or `float` matching array kind). |
| `nsimd.abs(array)` | Absolute value per element. |
| `nsimd.min(array)` | Minimum element, or `nil` when empty. |
| `nsimd.max(array)` | Maximum element, or `nil` when empty. |

Both operands of binary ops must be the same kind (`IntArray`/`IntArray` or `FloatArray`/`FloatArray`).

## Errors

| Code | Meaning |
|------|---------|
| 3350 | Wrong argument count. |
| 3351 | Length mismatch or semantic error (catchable `nsimd_error`). |
| 3352 | Wrong argument type (hard error). |

## See also

- `npar` — explicit rayon parallel ops on the same packed arrays.
- `nlazy` — fused lazy pipelines before materialization.
- `ncl` — column/vector helpers including optional parallel threshold paths.
