# Library spec: `db`  →  crate `niao_db`

| | |
|---|---|
| Category | Database |
| Replaces Rust crate(s) | `redis,postgres,r2d2,r2d2_postgres` (vn/a) |
| Target Niao crate | `crates/niao_db` |
| Difficulty | 4/5 — Very Hard |
| Status | ALREADY BUILT (tasks 01-12) — verify/extend only |
| Depends on Niao libs | crypto |

## Goal
Reimplement the functionality of `redis,postgres,r2d2,r2d2_postgres` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
RESP2/3, PG wire v3

## Implementation blueprint (make it FAST + LIGHT)
RESP codec + sync redis client, PG v3 startup+SCRAM+extended query+text decode, generic pool.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`redis.*/pg.*`
Expose to Niao programs through a `niao_libs/db/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
~ postgres crate latency

## Tests required
byte fixtures + env-guarded integ
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
TLS/binary format deferred

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
