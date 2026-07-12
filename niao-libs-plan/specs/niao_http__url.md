# Library spec: `url`  →  crate `niao_http`

| | |
|---|---|
| Category | HTTP |
| Replaces Rust crate(s) | `url` (v2) |
| Target Niao crate | `crates/niao_http` |
| Difficulty | 2/5 — Medium |
| Status | TO BUILD |
| Depends on Niao libs | none (leaf — can start immediately) |

## Goal
Reimplement the functionality of `url` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
WHATWG URL / RFC 3986

## Implementation blueprint (make it FAST + LIGHT)
parse scheme/authority/host/port/path/query/fragment, percent enc/dec, IDNA optional; already partly in niao_http task 07 — finish full spec.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`Url::parse, components, join`
Expose to Niao programs through a `niao_libs/http/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
alloc-light

## Tests required
WPT url subset
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
IDNA/punycode scope

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
