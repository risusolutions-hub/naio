# Library spec: `nnum`  →  crate `niao_num`

| | |
|---|---|
| Category | Numeric foundation |
| Replaces (Python) | `numpy` + `scipy.linalg` + `scipy.fft` |
| Rust reference | `ndarray`, `nalgebra`, `faer` (pure-Rust decompositions) |
| Target Niao crate | `crates/niao_num` |
| Niao import name | `nnum` |
| Difficulty | 4/5 — Very Hard |
| Wave | 0 (build first, alone — everything else depends on this) |
| Depends on Niao libs | `niao_tensor` (GEMM), `nrand` (random arrays) |
| Error block | 4000–4009 |

## Goal
The numeric bedrock for the whole ML stack: an n-dimensional array type with broadcasting, elementwise
ops, reductions, slicing, and a linear-algebra + FFT layer. **Zero external deps** — only `std`,
`niao_tensor`, and `nrand`. Route heavy matmul through `niao_tensor`'s existing SIMD/blocked GEMM instead
of writing a new one.

## Scope (v1)
- **Array (`NdArray<f64>` + `f32` variant):** shape/strides, contiguous storage, views/slices (basic +
  strided), reshape, transpose/permute-axes, broadcasting, `astype`.
- **Creation:** `zeros`, `ones`, `full`, `eye`, `arange`, `linspace`, `from_slice`, `rand`/`randn` (via nrand).
- **Elementwise:** `+ - * /`, `pow`, `exp/log/sqrt/abs`, trig, comparisons, `where`/`clip`, `maximum/minimum`.
- **Reductions (axis-aware):** `sum`, `mean`, `std`, `var`, `min`, `max`, `argmin`, `argmax`, `prod`, `cumsum`.
- **Linear algebra (`nnum.linalg`):** `matmul`/`dot` (→ niao_tensor GEMM), `solve` (LU w/ partial pivot),
  `inv`, `det`, `lstsq`, `qr`, `cholesky`, `svd`, `eig` (symmetric via Jacobi; general via QR-iteration),
  `norm` (1/2/inf/fro), `pinv`, `rank`, `trace`.
- **FFT (`nnum.fft`):** `fft`/`ifft` (radix-2 Cooley–Tukey + Bluestein for non-power-of-2), `rfft`, `fft2`.

## Implementation blueprint (make it FAST + LIGHT)
- Storage = single `Vec<f64>` + `shape: Vec<usize>` + `strides: Vec<isize>` + `offset`. Contiguous by default;
  views share the buffer via `Rc`/`Arc` slice or index math — no copies for slicing/transpose.
- Broadcasting: compute a broadcast shape, iterate with strided index; **no materialized broadcasts** in binops.
- Reductions: cache-friendly axis loops; accumulate in f64 even for f32 arrays (Kahan option for `sum`/`mean`).
- LU: in-place with partial pivoting, row-swap vector. QR: Householder reflections. Cholesky: lower, checks SPD.
- SVD: one-sided Jacobi (robust, easy to get right) for v1; note Golub–Kahan as v2. Symmetric eig: cyclic Jacobi.
- FFT: iterative in-place bit-reversal radix-2; Bluestein wraps arbitrary N to a power-of-2 convolution.

### Performance rules
- No heap allocation inside hot loops; reuse scratch buffers, pre-size `Vec`s.
- Prefer `&[f64]`/slices and in-place ops; offer `_mut` variants to avoid copies.
- `#[inline]` small hot fns; SIMD elementwise via `std::simd` or intrinsics **with scalar fallback**.
- Matmul is NOT re-implemented — call `niao_tensor` GEMM.

## Public API surface
`NdArray`, creation fns, elementwise + reductions, `linalg::{solve,inv,det,lstsq,qr,cholesky,svd,eig,norm,pinv}`,
`fft::{fft,ifft,rfft}`. Expose to Niao through `niao_libs/nnum/` wrapper + runtime builtins, mirroring
`niao_libs/nvalid`. Niao-facing surface stays small and array-first (e.g. `nnum.array`, `nnum.matmul`,
`nnum.svd`, `nnum.linspace`, `nnum.fft`).

## Performance target
- Elementwise/reduction: within **2×** of numpy on 1e6–1e8 element arrays.
- matmul: inherits `niao_tensor` GEMM target (no separate goal).
- Decompositions: **correct + numerically stable, nalgebra-class**. No MKL/OpenBLAS comparison required.

## Tests required
- Elementwise/reduction/broadcasting vs numpy fixtures (values pasted in tests), `rtol=1e-12` f64.
- `solve`/`inv`: `A @ inv(A) ≈ I`; residual `||Ax−b|| < 1e-10`. `det` vs known matrices.
- `qr`: `Q@R ≈ A`, `Qᵀ@Q ≈ I`. `cholesky`: `L@Lᵀ ≈ A`. `svd`: `U@diag(S)@Vᵀ ≈ A`, singular values vs numpy.
- `eig` (symmetric): eigenvalues vs numpy (sorted); eigenvectors orthonormal.
- `fft`: round-trip `ifft(fft(x)) ≈ x`; compare to numpy for random + known signals, power-of-2 and prime N.
- Degenerate: singular matrix → error 4004; shape mismatch → 4003; non-convergent eig → 4005 (no panic/NaN).
- Plus: in-crate unit tests, one `examples/nnum_demo.niao`, one `benchmarks/benchmark_nnum.py` vs numpy.

## Risk / notes
- SVD/eig are the trap. Ship one-sided Jacobi SVD + cyclic Jacobi symmetric eig (both easy to verify);
  defer Golub–Kahan / general non-symmetric eig to v2 behind a documented limitation.
- Sign/permutation ambiguity in SVD/eig/PCA — tests compare `|values|` and allow column-sign flips.
- Keep f32 and f64 paths; accumulate reductions in f64.

## Done criteria
- `cargo check --workspace` and `cargo test -p niao_num` green.
- Numeric fixtures pass within stated tolerances; degenerate inputs return typed errors, never panic.
- `niao_libs/nnum/` wrapper present with correct `builtin_count`; `examples/nnum_demo.niao` runs.
- Benchmark logged in `REPORT.md`; `CHANGELOG.md` updated. Shared-file edits reported to orchestrator (not applied).
