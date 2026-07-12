# Library spec: `rand-distr`  →  crate `niao_rand`

| | |
|---|---|
| Category | Numeric |
| Replaces Rust crate(s) | `rand_distr` (v0.4) |
| Target Niao crate | `crates/niao_rand` |
| Difficulty | 2/5 — Medium |
| Status | TO BUILD |
| Depends on Niao libs | rand |

## Goal
Reimplement the functionality of `rand_distr` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
statistical distributions

## Implementation blueprint (make it FAST + LIGHT)
Normal (Ziggurat), Uniform, Bernoulli, Exponential, Poisson, Gamma; used by niao_ml init.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`Normal/Uniform/etc .sample(rng)`
Expose to Niao programs through a `niao_libs/rand/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
Ziggurat fast normal

## Tests required
moment tests (mean/variance)
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
Ziggurat tables correctness

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
