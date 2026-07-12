# Library spec: `socket2`  →  crate `niao_io`

| | |
|---|---|
| Category | HTTP |
| Replaces Rust crate(s) | `socket2` (v0.5) |
| Target Niao crate | `crates/niao_io` |
| Difficulty | 2/5 — Medium |
| Status | TO BUILD |
| Depends on Niao libs | none (leaf — can start immediately) |

## Goal
Reimplement the functionality of `socket2` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
OS socket API

## Implementation blueprint (make it FAST + LIGHT)
thin safe wrapper over raw socket()/setsockopt/bind/connect with per-OS cfg; SO_REUSEADDR, TCP_NODELAY, timeouts, keepalive.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`Socket::new/set_opt/bind/connect`
Expose to Niao programs through a `niao_libs/io/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
n/a

## Tests required
option round-trip
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
windows winsock init

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
