# Library spec: `regex`  →  crate `niao_regex`

| | |
|---|---|
| Category | Text |
| Replaces Rust crate(s) | `regex` (v1) |
| Target Niao crate | `crates/niao_regex` |
| Difficulty | 4/5 — Very Hard |
| Status | ALREADY BUILT (tasks 01-12) — verify/extend only |
| Depends on Niao libs | none (leaf — can start immediately) |

## Goal
Reimplement the functionality of `regex` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
PCRE-subset / Thompson NFA

## Implementation blueprint (make it FAST + LIGHT)
parse->NFA->Pike VM, linear-time, capture slots, literal prefix fast-path (hand memchr). No catastrophic backtracking.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`is_match/find/find_iter/captures/replace_all/split`
Expose to Niao programs through a `niao_libs/regex/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
within 2x of regex crate, no exp blowup

## Tests required
pathological (a+)+b stays linear
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
lazy quantifier ordering partial

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
