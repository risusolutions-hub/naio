# Library spec: `toml`  →  crate `niao_json_core`

| | |
|---|---|
| Category | Serialization |
| Replaces Rust crate(s) | `toml` (v0.8) |
| Target Niao crate | `crates/niao_json_core` |
| Difficulty | 2/5 — Medium |
| Status | ALREADY BUILT (tasks 01-12) — verify/extend only |
| Depends on Niao libs | json |

## Goal
Reimplement the functionality of `toml` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
TOML 1.0

## Implementation blueprint (make it FAST + LIGHT)
reuse Value; tables, array-of-tables, inline tables, typed ints/floats, datetimes-as-string.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`toml.parse/encode`
Expose to Niao programs through a `niao_libs/json_core/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
n/a

## Tests required
round-trip niao.config
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
date types are strings

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
