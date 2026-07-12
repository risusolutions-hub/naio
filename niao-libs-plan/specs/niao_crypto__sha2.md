# Library spec: `sha2`  →  crate `niao_crypto`

| | |
|---|---|
| Category | Crypto |
| Replaces Rust crate(s) | `sha2` (v0.10) |
| Target Niao crate | `crates/niao_crypto` |
| Difficulty | 2/5 — Medium |
| Status | ALREADY BUILT (tasks 01-12) — verify/extend only |
| Depends on Niao libs | none (leaf — can start immediately) |

## Goal
Reimplement the functionality of `sha2` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
FIPS 180-4

## Implementation blueprint (make it FAST + LIGHT)
SHA-256/512 incremental Digest; constant-time compare.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`sha256/sha512`
Expose to Niao programs through a `niao_libs/crypto/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
within 10% of sha2 crate

## Tests required
NIST vectors
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
none

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
