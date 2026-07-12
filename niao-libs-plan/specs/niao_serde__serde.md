# Library spec: `serde`  →  crate `niao_serde`

| | |
|---|---|
| Category | Serialization |
| Replaces Rust crate(s) | `serde` (v1) |
| Target Niao crate | `crates/niao_serde` |
| Difficulty | 4/5 — Very Hard |
| Status | TO BUILD — high risk, read the risk note |
| Depends on Niao libs | json |

## Goal
Reimplement the functionality of `serde` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
data model + derive

## Implementation blueprint (make it FAST + LIGHT)
Niao-native reflection: since Niao owns its type system, expose a runtime Value<->struct mapping instead of proc-macro derive. For Rust-internal structs, hand-write to/from Value impls (already the migration pattern). Provide a codegen helper in niao_cli to emit impls.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`Serialize/Deserialize traits + Value bridge`
Expose to Niao programs through a `niao_libs/serde/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
no reflection overhead in hot paths

## Tests required
every migrated struct round-trips
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
LARGE: touches 10 crates; do incrementally, keep serde until each is migrated

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
