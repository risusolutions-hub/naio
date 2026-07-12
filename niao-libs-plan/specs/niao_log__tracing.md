# Library spec: `tracing`  →  crate `niao_log`

| | |
|---|---|
| Category | Observability |
| Replaces Rust crate(s) | `tracing,tracing-subscriber` (v0.1) |
| Target Niao crate | `crates/niao_log` |
| Difficulty | 2/5 — Medium |
| Status | TO BUILD |
| Depends on Niao libs | none (leaf — can start immediately) |

## Goal
Reimplement the functionality of `tracing,tracing-subscriber` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
structured logging

## Implementation blueprint (make it FAST + LIGHT)
span/event macros-free API, level filter via env, fields as kv, layered subscribers (fmt + json + file), thread-local span stack, async-safe.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`info!/span!/subscriber init`
Expose to Niao programs through a `niao_libs/log/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
disabled-level near-zero cost

## Tests required
level filtering, json output shape
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
span propagation across niao_io tasks

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
