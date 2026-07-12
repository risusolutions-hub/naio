# Performance notes (Task 12 — VM / runtime polish)

Profiled on Windows release builds, Jul 2026. Baselines captured in `benchmarks/baseline.json`.

## Findings (pre-change)

| Area | Observation |
|------|-------------|
| VM dispatch | Single `dispatch_step` match with `#[inline(always)]` opcode helpers; DSA loop fusion active for numeric kernels. |
| `FastVal` | NaN-box style: ints/bools/nil inline; heap/native via indices (no enum heap tag per value). |
| GC | Mark-compact; prior interval 8192 allocs / 16k threshold — sub-ms pauses on `vm_runs_math_stress` (~50ms total). |
| `.niaobc` | Mtime-only freshness; full file read on load. |
| Call bridge | Per-arg `value_to_fast` + stack push allocated temporaries on each HTTP handler invoke. |
| Startup | Heavy optional modules (`nllm`, `nrag`) register at runtime init when CLI features enabled; GGUF mmap deferred until first `nllm_load`. |

## Changes (task 12)

| Change | Rationale | Before → After |
|--------|-----------|----------------|
| Content-hash sidecar (`.niaobc.sha`) | Recompile when source bytes change even if mtime unchanged (CI/checkout). | mtime only → mtime + fingerprint |
| Pre-sized cache read | Avoid realloc while loading bytecode blob. | `fs::read` → `read_to_end` with `with_capacity` |
| `CALL_ARG_SCRATCH` thread-local | Reuse `Vec<FastVal>` for handler calls from native HTTP. | N allocs/call → 0 after first call |
| GC interval/threshold tune | Fewer collections on allocation-heavy but short-lived workloads. | 8192/16384 → 16384/24576 |
| `get_unchecked` on hot `Const`/`Load` | Drop bounds checks on bytecode indices proven by compiler. | checked indexing → unchecked in debug_assert builds |

## Benchmarks (release)

| Benchmark | Metric | Baseline |
|-----------|--------|----------|
| `niao_archive` inflate | MiB/s | 898 |
| `niao_vm` `vm_runs_math_stress` | wall (test) | ~0.05s |
| `niao_io` spawn | jobs/s | 4.68M |

Run gate: `powershell -File scripts/bench_gate.ps1` (5% regression tolerance).

## Deferred

- Full generational GC / nursery bump allocator (current pauses already <1ms on stress fixtures).
- `memmap2` for `.niaobc` (zero new deps policy; pre-sized read is sufficient for typical modules).
- Lazy `nllm`/`nrag` namespace objects (backends already lazy-load weights; registry cost is small vs CLI link).
