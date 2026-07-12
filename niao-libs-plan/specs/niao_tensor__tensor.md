# Library spec: `tensor`  →  crate `niao_tensor`

| | |
|---|---|
| Category | ML |
| Replaces Rust crate(s) | `candle-core` (v0.8) |
| Target Niao crate | `crates/niao_tensor` |
| Difficulty | 5/5 — Extreme |
| Status | TO BUILD — high risk, read the risk note |
| Depends on Niao libs | matrixmultiply, rand-distr, parallel |

## Goal
Reimplement the functionality of `candle-core` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
tensor framework

## Implementation blueprint (make it FAST + LIGHT)
ndarray storage, ops (matmul via niao gemm, conv, softmax, layernorm), autograd tape, CPU backend; optional GPU later. Broadcasting, dtype f32/f16.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`Tensor ops + backward`
Expose to Niao programs through a `niao_libs/tensor/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
CPU within 2-3x candle

## Tests required
gradient checks, op correctness vs reference
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
HUGE; GPU is separate mountain

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
