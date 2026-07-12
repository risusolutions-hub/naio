# Library spec: `futures`  →  crate `niao_io`

| | |
|---|---|
| Category | Async |
| Replaces Rust crate(s) | `futures,futures-util` (v0.3) |
| Target Niao crate | `crates/niao_io` |
| Difficulty | 4/5 — Very Hard |
| Status | TO BUILD — high risk, read the risk note |
| Depends on Niao libs | io-async |

## Goal
Reimplement the functionality of `futures,futures-util` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
Future combinators

## Implementation blueprint (make it FAST + LIGHT)
Future trait glue on niao_io executor: join/select/try_join, Stream trait, channels, FuturesUnordered.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`join!/select!/Stream`
Expose to Niao programs through a `niao_libs/io/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
zero-cost combinators

## Tests required
combinator semantics
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
pin/unsafe correctness

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
