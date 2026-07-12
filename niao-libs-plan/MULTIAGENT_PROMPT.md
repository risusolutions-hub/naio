# Cursor Multi-Agent Prompt — Niao Self-Hosted Libraries

Paste this into Cursor with multi-agent/parallel mode enabled, repo `C:\Risu\Neko` open.

---

You are the ORCHESTRATOR for a parallel library-rewrite project in the Niao repo (Rust workspace).
Mission: reimplement third-party Rust crates as native zero-dependency `niao_*` crates.
Full specs are in `niao-libs-plan/specs/` (one MD per library) and the plan is in
`niao-libs-plan/MASTER_PLAN.md`. Read both before dispatching.

## How to run
Process the WAVES from MASTER_PLAN.md in order. Within a wave, spawn ONE sub-agent PER library
IN PARALLEL — they are dependency-independent. Do NOT start the next wave until the current wave
is fully merged and `cargo check --workspace && cargo test --workspace` is green.

Skip libraries marked ✅ (already built) except to run their tests. For 🔴 high-risk libraries
(niao_tls, niao_jit, niao_tensor, transformers, llama, onnx, sqlite native), STOP and ask me before
building — implement the FFI/keep-crate fallback described in the spec's risk note unless I say otherwise.

## Instruction given to EACH sub-agent (fill in {SLUG})
```
You implement exactly ONE library for the Niao repo. Your spec is
niao-libs-plan/specs/*__{SLUG}.md — read it fully and follow it exactly.
Rules:
- Work ONLY inside your target crate + its niao_libs wrapper + your tests/examples/benchmarks.
  Do NOT edit other libraries' crates (avoid parallel merge conflicts). If you need a shared
  Cargo.toml/workspace edit, report it to the orchestrator instead of editing directly.
- ZERO new third-party crates. std + existing niao_* only.
- Fast + lightweight: no hot-loop allocs, reuse buffers, #[inline] hot fns, SIMD + scalar fallback.
- Deliver: crate code, niao_libs/<name>/ wrapper, unit tests, one examples/*.niao, one benchmark
  vs the replaced crate (generate fixtures from the old crate BEFORE anyone removes it).
- Do NOT remove the replaced crate from Cargo.toml yet — list it in your final report so the
  orchestrator removes deps in a single serialized pass after the wave (prevents Cargo.lock conflicts).
- Acceptance: your crate's `cargo test -p <crate>` is green and the benchmark meets the spec target.
- Write your results (benchmarks, deviations, deps-to-remove) to niao-libs-plan/REPORT.md under a
  heading '## {SLUG}'.
```

## Orchestrator loop per wave
1. `git commit -am "checkpoint before wave N"`.
2. Spawn all sub-agents for wave N in parallel with the instruction above.
3. When all report done: serially remove the replaced crates they listed from Cargo.toml(s),
   run `cargo tree` to confirm removal, then `cargo check --workspace && cargo test --workspace`.
4. If green: `git commit -am "wave N complete"`. If red: fix or revert; never advance on red.
5. Update CHANGELOG.md with the wave summary. Proceed to wave N+1.

Begin with Wave 0 now. Report the wave plan (which libraries, which are skipped/❓high-risk) before spawning.
