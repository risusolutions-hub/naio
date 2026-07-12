# Task 12 — VM, runtime & toolchain perfection pass
Read MASTER_PLAN.md first. This is polish + performance, no new features.

## Do
1. PROFILE first: run benchmarks/ (math_bench_heavy, dsa_bench, ncl_bench) with a profiler; write findings to docs/perf_notes.md before touching code.
2. VM dispatch: verify the interpreter loop is a computed-goto-style match with #[inline(always)] handlers; check fast_val.rs NaN-boxing (or introduce it if it's an enum); eliminate per-op bounds checks with unsafe get_unchecked where indices are proven.
3. GC (gc.rs): measure pause times; add generational or at least a nursery bump-allocator if pauses >1ms on dsa_bench; tune growth heuristics.
4. Call bridge (call_bridge.rs): remove allocations per builtin call (reusable arg buffers).
5. .niaobc caching: hash-based invalidation, mmap load, version stamping.
6. String interning for identifiers + small-string optimization in the Value type if absent.
7. Startup time: measure `niao run hello.niao` cold; target <30ms; lazy-init heavy runtime modules (ML/LLM must not load unless imported).
8. CI gate: add scripts/bench_gate.(ps1|sh) that fails if any benchmark regresses >5% vs benchmarks/baseline.json (create the baseline).

## Acceptance
- All benchmarks equal or faster; document each change + before/after numbers in docs/perf_notes.md.
- Full test suite green; binary size of release `niao` not grown >5%.

## Ground rules (apply to EVERY task)
- ZERO new third-party crates. Only std + existing niao_* workspace crates.
- Lightweight & fast: no allocations in hot loops, prefer &str/&[u8], pre-size Vecs, add #[inline] on small hot fns.
- Public API surface goes to Niao programs via niao_runtime builtins + a niao_libs/<name>/ module (Niao-language wrapper), mirroring how niao_libs/json works today.
- Add unit tests in the crate + at least one .niao example under examples/.
- Add/extend a benchmark under benchmarks/ comparing before vs after.
- After changes: `cargo check --workspace` then `cargo test --workspace` must pass.
- Remove the replaced third-party dependency from every Cargo.toml that used it, and confirm it disappears from `cargo tree`.
- Update CHANGELOG.md with one line.
