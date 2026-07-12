# Library spec: `hf-hub`  →  crate `niao_hub`

| | |
|---|---|
| Category | ML |
| Replaces Rust crate(s) | `hf-hub` (v0.5) |
| Target Niao crate | `crates/niao_hub` |
| Difficulty | 2/5 — Medium |
| Status | TO BUILD |
| Depends on Niao libs | http, json |

## Goal
Reimplement the functionality of `hf-hub` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
HF hub HTTP API

## Implementation blueprint (make it FAST + LIGHT)
model/file download from HF over niao_http, resume, sha check, local cache layout. Already vendored — reimplement on niao_http.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`hub.download(repo,file)`
Expose to Niao programs through a `niao_libs/hub/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
n/a

## Tests required
against HF api (net-guarded)
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
none

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
