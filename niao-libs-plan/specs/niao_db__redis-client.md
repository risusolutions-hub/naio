# Library spec: `redis-client`  →  crate `niao_db`

| | |
|---|---|
| Category | Database |
| Replaces Rust crate(s) | `redis` (v0.27) |
| Target Niao crate | `crates/niao_db` |
| Difficulty | 2/5 — Medium |
| Status | ALREADY BUILT (tasks 01-12) — verify/extend only |
| Depends on Niao libs | crypto |

## Goal
Reimplement the functionality of `redis` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
RESP2/3

## Implementation blueprint (make it FAST + LIGHT)
(covered by niao_db task 10; pub/sub + cluster remain).

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`redis.*`
Expose to Niao programs through a `niao_libs/db/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
ok

## Tests required
fixtures
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
cluster/pubsub extra

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
