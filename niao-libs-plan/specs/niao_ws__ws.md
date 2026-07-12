# Library spec: `ws`  →  crate `niao_ws`

| | |
|---|---|
| Category | WebSocket |
| Replaces Rust crate(s) | `tungstenite` (v0.26) |
| Target Niao crate | `crates/niao_ws` |
| Difficulty | 3/5 — Hard |
| Status | ALREADY BUILT (tasks 01-12) — verify/extend only |
| Depends on Niao libs | http, crypto |

## Goal
Reimplement the functionality of `tungstenite` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
RFC 6455

## Implementation blueprint (make it FAST + LIGHT)
handshake via sha1, frame codec masking/fragmentation/ping/pong/close, UTF-8 validate.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`ws.connect/accept`
Expose to Niao programs through a `niao_libs/ws/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
>= tungstenite throughput

## Tests required
autobahn subset
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
async server side pending

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
