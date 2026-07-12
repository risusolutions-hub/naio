# Library spec: `dotenv`  →  crate `niao_codec`

| | |
|---|---|
| Category | Env |
| Replaces Rust crate(s) | `dotenvy` (v0.15) |
| Target Niao crate | `crates/niao_codec` |
| Difficulty | 1/5 — Trivial |
| Status | ALREADY BUILT (tasks 01-12) — verify/extend only |
| Depends on Niao libs | none (leaf — can start immediately) |

## Goal
Reimplement the functionality of `dotenvy` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
dotenv de-facto

## Implementation blueprint (make it FAST + LIGHT)
line parser: quotes, escapes, comments, export prefix, CRLF.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`dotenv.load`
Expose to Niao programs through a `niao_libs/codec/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
n/a

## Tests required
edge cases
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
none

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
