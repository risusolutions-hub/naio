# Library spec: `llama`  →  crate `niao_llm`

| | |
|---|---|
| Category | ML |
| Replaces Rust crate(s) | `llama-cpp-2,llama-cpp-sys-2` (v0.1.151) |
| Target Niao crate | `crates/niao_llm` |
| Difficulty | 5/5 — Extreme |
| Status | TO BUILD — high risk, read the risk note |
| Depends on Niao libs | tensor |

## Goal
Reimplement the functionality of `llama-cpp-2,llama-cpp-sys-2` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
GGUF inference

## Implementation blueprint (make it FAST + LIGHT)
Either (a) native GGUF loader + quantized (Q4/Q8) matmul kernels on niao_tensor, or (b) keep as FFI. Pure-Niao path = reimplementing llama.cpp kernels incl. SIMD quant dequant.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`llm.load_gguf/generate`
Expose to Niao programs through a `niao_libs/llm/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
tokens/sec within 3x llama.cpp CPU

## Tests required
output matches llama.cpp greedy decode
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
ENORMOUS; C++ llama.cpp is heavily optimized. Recommend FFI wrapper.

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
