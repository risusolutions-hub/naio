# Library spec: `time`  →  crate `niao_time`

| | |
|---|---|
| Category | DateTime |
| Replaces Rust crate(s) | `chrono,chrono-tz` (v0.4) |
| Target Niao crate | `crates/niao_time` |
| Difficulty | 3/5 — Hard |
| Status | ALREADY BUILT (tasks 01-12) — verify/extend only |
| Depends on Niao libs | none (leaf — can start immediately) |

## Goal
Reimplement the functionality of `chrono,chrono-tz` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
RFC3339/2822, IANA tzdata

## Implementation blueprint (make it FAST + LIGHT)
Hinnant civil-date algos (no floats), Instant/DateTime/Duration, strftime subset, compiled tzdata subset.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`now/parse/format/tz`
Expose to Niao programs through a `niao_libs/time/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
>= chrono format/parse

## Tests required
DST edges, :30 offsets
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
tz subset only

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
