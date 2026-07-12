# Library spec: `brotli`  →  crate `niao_archive`

| | |
|---|---|
| Category | Compression |
| Replaces Rust crate(s) | `(tower-http br)` (vn/a) |
| Target Niao crate | `crates/niao_archive` |
| Difficulty | 4/5 — Very Hard |
| Status | TO BUILD — high risk, read the risk note |
| Depends on Niao libs | archive |

## Goal
Reimplement the functionality of `(tower-http br)` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
RFC 7932

## Implementation blueprint (make it FAST + LIGHT)
brotli decode + basic encode (quality subset) for tower-http compression-br.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`brotli.compress/decompress`
Expose to Niao programs through a `niao_libs/archive/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
decode fast; encode ratio ok

## Tests required
RFC7932 test vectors
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
complex format; encode can be low-quality v1

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
