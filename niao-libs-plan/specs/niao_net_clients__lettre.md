# Library spec: `lettre`  →  crate `niao_net_clients`

| | |
|---|---|
| Category | Mail/FTP |
| Replaces Rust crate(s) | `lettre` (v0.11) |
| Target Niao crate | `crates/niao_net_clients` |
| Difficulty | 2/5 — Medium |
| Status | TO BUILD |
| Depends on Niao libs | crypto, http |

## Goal
Reimplement the functionality of `lettre` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
RFC 5321/5322

## Implementation blueprint (make it FAST + LIGHT)
SMTP client: EHLO, STARTTLS(rustls-or-nats), AUTH PLAIN/LOGIN, MIME message builder, attachments base64.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`smtp.send(message)`
Expose to Niao programs through a `niao_libs/net_clients/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
n/a

## Tests required
against local smtp mock
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
TLS dependency

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
