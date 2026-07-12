# Library spec: `axum`  →  crate `niao_web2`

| | |
|---|---|
| Category | HTTP |
| Replaces Rust crate(s) | `axum` (v0.8) |
| Target Niao crate | `crates/niao_web2` |
| Difficulty | 4/5 — Very Hard |
| Status | TO BUILD — high risk, read the risk note |
| Depends on Niao libs | hyper, tower |

## Goal
Reimplement the functionality of `axum` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
router/extractors

## Implementation blueprint (make it FAST + LIGHT)
matchit-style radix router, extractors (Path/Query/Json/State), handler trait, ws+multipart. Powers ahiru block DSL.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`Router::new().route()`
Expose to Niao programs through a `niao_libs/web2/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
>= axum routing

## Tests required
ahiru example apps still serve identically
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
ahiru depends on this; migrate behind flag

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
