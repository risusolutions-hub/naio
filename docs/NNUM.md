# NNUM — Numeric foundation for Niao

`nnum` replaces a subset of **numpy**, **scipy.linalg**, and **scipy.fft** with a std-only,
zero-dependency native library (`crates/niao_num`).

Import:

```niao
import "nnum"
```

## Arrays

Arrays are opaque handles returned by creation functions. Use `nnum.shape(handle)` and
`nnum.to_float_array(handle)` to inspect data.

| Function | Description |
|----------|-------------|
| `nnum.array(shape, data)` | Build array from `int_array` shape + `float_array` data |
| `nnum.zeros(shape)` | Zeros |
| `nnum.ones(shape)` | Ones |
| `nnum.linspace(start, stop, n)` | Evenly spaced vector |
| `nnum.arange(start, stop, step?)` | Range vector |
| `nnum.eye(n)` | Identity matrix |

## Elementwise & reductions

| Function | Description |
|----------|-------------|
| `nnum.add(a, b)` | Broadcast add |
| `nnum.sum(a, axis?)` | Sum |
| `nnum.mean(a, axis?)` | Mean |
| `nnum.dot(a, b)` | Inner product |

## Linear algebra

| Function | Description |
|----------|-------------|
| `nnum.matmul(a, b)` | Matrix multiply |
| `nnum.solve(a, b)` | Solve `Ax = b` |
| `nnum.inv(a)` | Matrix inverse |
| `nnum.det(a)` | Determinant |
| `nnum.transpose(a)` | 2-D transpose |
| `nnum.trace(a)` | Matrix trace |

## FFT

`nnum.fft(a)` returns `{re: float_array, im: float_array}` for a 1-D array.

## Error codes (4000–4009)

| Code | Meaning |
|------|---------|
| 4000 | arity |
| 4001 | general error / invalid handle |
| 4002 | type mismatch |
| 4003 | shape mismatch |
| 4004 | singular matrix |
| 4005 | non-convergence |

## v1 limitations

- Symmetric eig only (cyclic Jacobi); general non-symmetric eig deferred to v2
- SVD uses one-sided Jacobi (correct, not MKL-fast)
- Large matmul: use `matmul_tensor` in Rust for `niao_tensor` GEMM path
