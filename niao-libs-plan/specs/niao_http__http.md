# Library spec: `http`  →  crate `niao_http`

| | |
|---|---|
| Category | HTTP |
| Replaces Rust crate(s) | `httparse,tiny_http,ureq,url,http` (v1) |
| Target Niao crate | `crates/niao_http` |
| Difficulty | 4/5 — Very Hard |
| Status | ALREADY BUILT (tasks 01-12) — verify/extend only |
| Depends on Niao libs | codec |

## Goal
Reimplement the functionality of `httparse,tiny_http,ureq,url,http` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
RFC 7230/3986

## Implementation blueprint (make it FAST + LIGHT)
incremental HTTP/1.1 parser (state machine, no copies), chunked, connection pool client, thread-pool server, own URL parser + percent-encoding.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`nhttp.get/post/serve`
Expose to Niao programs through a `niao_libs/http/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
>= tiny_http rps, ~ureq latency

## Tests required
smuggling rejection
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
HTTP/2 not covered

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
