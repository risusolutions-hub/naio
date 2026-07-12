# Library spec: `bson`  →  crate `niao_json_core`

| | |
|---|---|
| Category | Serialization |
| Replaces Rust crate(s) | `bson` (v2.13) |
| Target Niao crate | `crates/niao_json_core` |
| Difficulty | 2/5 — Medium |
| Status | TO BUILD |
| Depends on Niao libs | json |

## Goal
Reimplement the functionality of `bson` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
bsonspec.org

## Implementation blueprint (make it FAST + LIGHT)
reuse Value; binary reader/writer for all element types (double,string,doc,array,binary,objectid,bool,datetime,null,regex,int32/64,decimal128,timestamp).

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`bson.encode/decode <-> Value`
Expose to Niao programs through a `niao_libs/json_core/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
zero-copy where possible

## Tests required
round-trip vs mongo fixtures, ObjectId gen
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
decimal128 arithmetic

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
