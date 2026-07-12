# Library spec: `tower`  →  crate `niao_web2`

| | |
|---|---|
| Category | HTTP |
| Replaces Rust crate(s) | `tower,tower-http` (v0.5) |
| Target Niao crate | `crates/niao_web2` |
| Difficulty | 3/5 — Hard |
| Status | TO BUILD — high risk, read the risk note |
| Depends on Niao libs | hyper |

## Goal
Reimplement the functionality of `tower,tower-http` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
Service abstraction

## Implementation blueprint (make it FAST + LIGHT)
Service/Layer traits, middleware stack: CORS, body limit, compression(gzip/br via niao_archive), set-header, static fs, trace.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`ServiceBuilder.layer()`
Expose to Niao programs through a `niao_libs/web2/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
minimal per-request overhead

## Tests required
each middleware behaviour
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
needs hyper+brotli

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
