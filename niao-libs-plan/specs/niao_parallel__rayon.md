# Library spec: `rayon`  →  crate `niao_parallel`

| | |
|---|---|
| Category | Concurrency |
| Replaces Rust crate(s) | `rayon` (v1) |
| Target Niao crate | `crates/niao_parallel` |
| Difficulty | 4/5 — Very Hard |
| Status | TO BUILD — high risk, read the risk note |
| Depends on Niao libs | io-async |

## Goal
Reimplement the functionality of `rayon` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
work-stealing data parallelism

## Implementation blueprint (make it FAST + LIGHT)
global thread pool (once), Chase-Lev deque per worker, par_iter via recursive split (join), parallel map/reduce/sort; sits on niao_io executor primitives.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`par_iter/join/par_sort/scope`
Expose to Niao programs through a `niao_libs/parallel/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
near-linear speedup, low overhead vs rayon

## Tests required
parallel sum/sort correctness, nested join
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
work-stealing correctness, panic propagation

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
