# Library spec: `indexmap`  →  crate `niao_collections`

| | |
|---|---|
| Category | DataStruct |
| Replaces Rust crate(s) | `indexmap` (v2) |
| Target Niao crate | `crates/niao_collections` |
| Difficulty | 2/5 — Medium |
| Status | TO BUILD |
| Depends on Niao libs | none (leaf — can start immediately) |

## Goal
Reimplement the functionality of `indexmap` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
insertion-ordered map

## Implementation blueprint (make it FAST + LIGHT)
hash table of indices + dense entry Vec; robin-hood or swiss-table probing; preserves insertion order, O(1) get/insert, stable iteration.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`IndexMap/IndexSet: insert/get/remove(swap+shift)/iter`
Expose to Niao programs through a `niao_libs/collections/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
>= indexmap crate; no per-op alloc

## Tests required
order preservation, remove variants, 1M insert bench
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
none

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
