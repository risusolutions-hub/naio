# Library spec: `hyper`  →  crate `niao_http`

| | |
|---|---|
| Category | HTTP |
| Replaces Rust crate(s) | `hyper,hyper-util` (v1) |
| Target Niao crate | `crates/niao_http` |
| Difficulty | 5/5 — Extreme |
| Status | TO BUILD — high risk, read the risk note |
| Depends on Niao libs | http, io-async |

## Goal
Reimplement the functionality of `hyper,hyper-util` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
RFC 7230 + RFC 9113 (HTTP/2)

## Implementation blueprint (make it FAST + LIGHT)
HTTP/1.1 already in niao_http (sync). For ahiru: async HTTP/1.1 on niao_io + HTTP/2 (HPACK, framing, flow control, multiplexing). Big.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`server::conn, client::conn`
Expose to Niao programs through a `niao_libs/http/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
>= hyper h1 rps; h2 functional

## Tests required
h2spec subset
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
HTTP/2 is a major project; consider h1-only first, gate h2

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
