# Library spec: `mongodb`  →  crate `niao_runtime`

| | |
|---|---|
| Category | Database |
| Replaces Rust crate(s) | `mongodb` (v3.1) |
| Target Niao crate | `crates/niao_runtime` |
| Difficulty | 4/5 — Very Hard |
| Status | TO BUILD — high risk, read the risk note |
| Depends on Niao libs | bson, crypto, db |

## Goal
Reimplement the functionality of `mongodb` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
MongoDB wire protocol

## Implementation blueprint (make it FAST + LIGHT)
OP_MSG wire protocol, SCRAM-SHA-256 auth, BSON via niao bson, connection pool, CRUD + aggregation passthrough, topology/replica discovery (basic).

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`mongo.collection.find/insert/...`
Expose to Niao programs through a `niao_libs/runtime/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
~ mongodb crate

## Tests required
env-guarded integ
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
LARGE; replica-set topology + retryable writes are complex. Scope to standalone + basic pool first.

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
