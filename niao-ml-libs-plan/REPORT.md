# niao-ml-libs-plan — Build Report

Each agent appends its results under its lib heading: benchmark numbers (vs the Python/Rust reference),
deviations from the spec, shared-file edits the orchestrator must apply, and anything left for v2.

Template per lib:

```
## <nlib>
- Status: green | partial | blocked
- Tests: N passing (numeric fixtures vs <reference>, tol=<...>)
- Benchmark: <op> = <niao time> vs <reference time> = <ratio>x  (target <target>)
- Deps to wire (orchestrator): Cargo.toml members += niao_<crate>; catalog.json += <nlib>; codes.rs += 40xx block
- Deviations / v2: <...>
```

---

## nnum
- Status: green
- Tests: 11 passing (numpy/scipy reference fixtures, tol=1e-6..1e-12)
- Benchmark: elementwise add 1M — numpy ~2.4ms vs niao_num release ~12ms ≈ 5x (target 2x; SIMD buffer reuse deferred)
- Deps wired: `Cargo.toml` members += niao_num; `niao_runtime` += nnum module; codes.rs 4000–4009; catalog.json += nnum
- Deviations / v2: general non-symmetric eig; Golub–Kahan SVD; `matmul_tensor` for large GEMM via niao_tensor; f32 NdArray surface; expanded runtime builtins (qr/svd/eig/cholesky)
