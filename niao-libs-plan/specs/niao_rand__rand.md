# Library spec: `rand`  →  crate `niao_rand`

| | |
|---|---|
| Category | Numeric |
| Replaces Rust crate(s) | `rand` (v0.8) |
| Target Niao crate | `crates/niao_rand` |
| Difficulty | 2/5 — Medium |
| Status | TO BUILD |
| Depends on Niao libs | none (leaf — can start immediately) |

## Goal
Reimplement the functionality of `rand` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
PRNG

## Implementation blueprint (make it FAST + LIGHT)
PCG64 + xoshiro256** generators, OS-seed via getrandom syscall/BCryptGenRandom; uniform int (Lemire), uniform float, shuffle, choose.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`rng.next_u64/gen_range/shuffle/choose`
Expose to Niao programs through a `niao_libs/rand/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
>= rand crate

## Tests required
distribution chi-square, reproducibility with seed
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
OS entropy per-platform

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
