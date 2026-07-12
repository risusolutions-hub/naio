# Library spec: `clap`  →  crate `niao_args`

| | |
|---|---|
| Category | CLI |
| Replaces Rust crate(s) | `clap` (v4) |
| Target Niao crate | `crates/niao_args` |
| Difficulty | 2/5 — Medium |
| Status | TO BUILD |
| Depends on Niao libs | none (leaf — can start immediately) |

## Goal
Reimplement the functionality of `clap` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
CLI arg parsing

## Implementation blueprint (make it FAST + LIGHT)
declarative builder: flags/options/positionals/subcommands, short+long, =/space values, help+usage gen, env fallback; no proc-macro (runtime builder).

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`Command::new().arg().subcommand().parse()`
Expose to Niao programs through a `niao_libs/args/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
n/a (parse once)

## Tests required
every existing niao_cli/niao_nm invocation parses identically
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
must match current clap CLI exactly

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
