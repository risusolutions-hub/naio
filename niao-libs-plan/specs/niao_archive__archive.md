# Library spec: `archive`  →  crate `niao_archive`

| | |
|---|---|
| Category | Compression |
| Replaces Rust crate(s) | `flate2,tar,zip` (vn/a) |
| Target Niao crate | `crates/niao_archive` |
| Difficulty | 4/5 — Very Hard |
| Status | ALREADY BUILT (tasks 01-12) — verify/extend only |
| Depends on Niao libs | none (leaf — can start immediately) |

## Goal
Reimplement the functionality of `flate2,tar,zip` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
RFC 1951/1952, ustar, zip

## Implementation blueprint (make it FAST + LIGHT)
inflate + gzip, deflate(fixed huffman+greedy LZ77), crc32 slice-by-8, adler32, tar ustar/pax, zip r/w zip64.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`gzip/deflate/tar/zip`
Expose to Niao programs through a `niao_libs/archive/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
inflate >=60% flate2, correctness 100%

## Tests required
cross-crate fixtures
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
deflate ratio below flate2

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
