# Library spec: `jit`  →  crate `niao_jit`

| | |
|---|---|
| Category | JIT |
| Replaces Rust crate(s) | `cranelift-codegen,cranelift-frontend,cranelift-jit,cranelift-module,cranelift-native` (v0.133) |
| Target Niao crate | `crates/niao_jit` |
| Difficulty | 5/5 — Extreme |
| Status | TO BUILD — high risk, read the risk note |
| Depends on Niao libs | none (leaf — can start immediately) |

## Goal
Reimplement the functionality of `cranelift-codegen,cranelift-frontend,cranelift-jit,cranelift-module,cranelift-native` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
machine code generation

## Implementation blueprint (make it FAST + LIGHT)
Own JIT backend for niao_vm: lower niao IR -> SSA -> regalloc (linear scan) -> x86-64 + aarch64 encoders -> executable mmap; CPU feature detection; relocation/patching. This IS your compiler backend.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`jit.compile(ir)->fnptr`
Expose to Niao programs through a `niao_libs/jit/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
generated code within 2x cranelift; compile time low

## Tests required
every VM op path jitted matches interpreter result, differential fuzzing
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
ENORMOUS. Writing a correct multi-arch codegen is a multi-person-year effort. Recommend keeping cranelift; if pursued, start x86-64 only, interpreter fallback always available.

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
