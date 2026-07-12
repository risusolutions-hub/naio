# Library spec: `hmac`  →  crate `niao_crypto`

| | |
|---|---|
| Category | Crypto |
| Replaces Rust crate(s) | `hmac` (v0.12) |
| Target Niao crate | `crates/niao_crypto` |
| Difficulty | 1/5 — Trivial |
| Status | ALREADY BUILT (tasks 01-12) — verify/extend only |
| Depends on Niao libs | sha2 |

## Goal
Reimplement the functionality of `hmac` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
RFC 2104/4231

## Implementation blueprint (make it FAST + LIGHT)
HMAC over sha2 digests.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`hmac`
Expose to Niao programs through a `niao_libs/crypto/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
n/a

## Tests required
RFC4231 vectors
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
none

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
