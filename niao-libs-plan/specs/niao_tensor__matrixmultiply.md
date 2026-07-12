# Library spec: `matrixmultiply`  →  crate `niao_tensor`

| | |
|---|---|
| Category | Numeric |
| Replaces Rust crate(s) | `matrixmultiply` (v0.3) |
| Target Niao crate | `crates/niao_tensor` |
| Difficulty | 4/5 — Very Hard |
| Status | TO BUILD — high risk, read the risk note |
| Depends on Niao libs | none (leaf — can start immediately) |

## Goal
Reimplement the functionality of `matrixmultiply` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
GEMM

## Implementation blueprint (make it FAST + LIGHT)
cache-blocked sgemm/dgemm, microkernel with FMA intrinsics (x86 AVX2, aarch64 NEON) + scalar fallback, packed panels; multithread via niao_parallel.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`sgemm/dgemm(a,b,c)`
Expose to Niao programs through a `niao_libs/tensor/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
>= 60% of matrixmultiply on MxKxN

## Tests required
vs naive reference on random matrices
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
SIMD portability, numerical order

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
