# Library spec: `num-bigint`  →  crate `niao_bignum`

| | |
|---|---|
| Category | Numeric |
| Replaces Rust crate(s) | `num-bigint,num-traits` (v0.4) |
| Target Niao crate | `crates/niao_bignum` |
| Difficulty | 3/5 — Hard |
| Status | TO BUILD |
| Depends on Niao libs | none (leaf — can start immediately) |

## Goal
Reimplement the functionality of `num-bigint,num-traits` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
arbitrary precision

## Implementation blueprint (make it FAST + LIGHT)
sign+magnitude Vec<u64> limbs; schoolbook mul + Karatsuba above threshold; Knuth div; base conversion; used by VM bigint.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`BigInt +-*/% pow cmp to/from_str`
Expose to Niao programs through a `niao_libs/bignum/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
Karatsuba beats naive >256 bits; VM path not regressed

## Tests required
property tests vs known values, VM bigint suite
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
division correctness edge cases

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
