# Library spec: `ahash`  →  crate `niao_collections`

| | |
|---|---|
| Category | Hashing |
| Replaces Rust crate(s) | `ahash` (v0.8) |
| Target Niao crate | `crates/niao_collections` |
| Difficulty | 2/5 — Medium |
| Status | TO BUILD |
| Depends on Niao libs | none (leaf — can start immediately) |

## Goal
Reimplement the functionality of `ahash` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
non-crypto fast hash

## Implementation blueprint (make it FAST + LIGHT)
AES-NI-backed hash when available (aarch64/x86 intrinsics), fallback to fast fxhash/wyhash; per-process random seed for DoS resistance.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`Hasher impl + HashMap type alias`
Expose to Niao programs through a `niao_libs/collections/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
>= ahash on 8/64/1k byte keys

## Tests required
distribution/avalanche, seed randomness
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
needs cfg intrinsics + scalar fallback

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
