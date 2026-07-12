# Library spec: `dashmap`  →  crate `niao_collections`

| | |
|---|---|
| Category | Concurrency |
| Replaces Rust crate(s) | `dashmap` (v6) |
| Target Niao crate | `crates/niao_collections` |
| Difficulty | 3/5 — Hard |
| Status | TO BUILD |
| Depends on Niao libs | ahash |

## Goal
Reimplement the functionality of `dashmap` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
sharded concurrent map

## Implementation blueprint (make it FAST + LIGHT)
N shards each RwLock<HashMap>, shard by hash high bits; try_lock fast path; entry API.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`DashMap: insert/get/remove/entry/iter`
Expose to Niao programs through a `niao_libs/collections/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
near-linear scaling to 16 threads

## Tests required
concurrent stress (loom-style if possible), no deadlock
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
iteration consistency semantics

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
