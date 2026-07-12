# Task 00 — nnum: numpy + scipy.linalg + scipy.fft (crate `niao_num`)
Wave 0 — build FIRST, alone. Everything depends on this. Read `../MASTER_PLAN.md` + `../specs/niao_num__nnum.md` first.
Error block: **4000–4009**. Depends on: `niao_tensor` (GEMM), `nrand`.

## Build (`crates/niao_num`, zero new deps)
- `NdArray<f64>` (+ f32): shape/strides/offset over one contiguous buffer; views/slices share the buffer (no copy on
  slice/transpose/reshape); broadcasting without materialization.
- Creation: zeros/ones/full/eye/arange/linspace/from_slice/rand/randn(via nrand).
- Elementwise (+ - * / pow exp log sqrt abs trig cmp clip where max/min); reductions axis-aware
  (sum/mean/std/var/min/max/argmin/argmax/prod/cumsum), accumulate in f64.
- `linalg`: matmul→niao_tensor GEMM; solve(LU+pivot), inv, det, lstsq, qr(Householder), cholesky, svd(one-sided
  Jacobi), eig(symmetric cyclic Jacobi), norm, pinv, rank, trace.
- `fft`: fft/ifft (radix-2 + Bluestein for non-pow2), rfft, fft2.

## Wire up
- `niao_libs/nnum/` wrapper (package.json + 0.2.2/lib.json + 0.2.3/lib.json, kind native, correct builtin_count,
  mirror `niao_libs/nvalid`) + runtime builtins. `docs/NNUM.md`. `examples/nnum_demo.niao`.

## Acceptance
- Elementwise/reduction/broadcast vs numpy fixtures rtol 1e-12; solve/inv residual <1e-10; qr/cholesky/svd
  reconstruct A; symmetric eig eigenvalues vs numpy; fft round-trip + vs numpy (pow2 + prime N).
- Singular→4004, shape mismatch→4003, non-convergence→4005 (typed errors, no panic/NaN).
- `benchmarks/benchmark_nnum.py` vs numpy; elementwise within 2x. `cargo test -p niao_num` green.

See `../cursor-rules.md` for the ground rules that apply to every task.
