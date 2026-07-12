# Library spec: `sqlx`  →  crate `niao_db`

| | |
|---|---|
| Category | Database |
| Replaces Rust crate(s) | `sqlx` (v0.8) |
| Target Niao crate | `crates/niao_db` |
| Difficulty | 4/5 — Very Hard |
| Status | TO BUILD — high risk, read the risk note |
| Depends on Niao libs | db |

## Goal
Reimplement the functionality of `sqlx` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
async multi-DB

## Implementation blueprint (make it FAST + LIGHT)
sqlx gives ahiru async MySQL/PG/SQLite. Provide async query layer on niao_db (PG done) + native MySQL wire protocol + sqlite via niao_sqlite; unified query API.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`query/execute/pool`
Expose to Niao programs through a `niao_libs/db/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
~ sqlx

## Tests required
per-backend integ (env-guarded)
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
MySQL wire proto + async are sizable

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
