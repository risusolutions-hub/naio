# Cursor Multi-Agent Prompt — Niao ML Libraries

Paste the block below into Cursor with **multi-agent / parallel mode enabled**, repo `C:\Risu\Neko` open.
First (recommended): copy `niao-ml-libs-plan/cursor-rules.md` → `.cursor/rules/niao-ml.mdc`.

---

You are the **ORCHESTRATOR** for a parallel library-build project in the Niao repo (a Rust workspace that
implements the Niao language). Mission: add **10 native, std-only, zero-dependency machine-learning libraries**
that complete Niao's scientific-ML stack around the existing `niao_tensor` + `niao_ml` (the PyTorch side).

Authoritative docs (read all three before dispatching anything):
- `niao-ml-libs-plan/MASTER_PLAN.md` — the 10 libs, dependency **waves**, difficulty tiers, error-code map, ground rules.
- `niao-ml-libs-plan/specs/*.md` — one full spec per library (blueprint, API surface, perf target, tests, risk).
- `niao-ml-libs-plan/cursor-tasks/task-*.md` — the actionable checklist per library.

## The 10 libraries and their waves
- **Wave 0 (run FIRST, alone):** `nnum` (numpy+scipy.linalg+fft → `crates/niao_num`). Everything depends on it.
- **Wave 1 (parallel — depend only on nnum):** `nframe`, `nstats`, `noptim`, `nplot`.
- **Wave 2 (parallel — depend on Wave 0/1):** `nlearn`, `nboost`, `nts`, `nnlp`, `nvision`.

## How to run
Process the waves **in order**. Within a wave, spawn **ONE sub-agent per library IN PARALLEL** — they are
dependency-independent. **Do NOT start the next wave** until the current wave is fully merged and
`cargo check --workspace && cargo test --workspace` is green.

Before spawning each wave, print the wave plan (which libraries, their crates, their error blocks) and wait for
my go-ahead. For any library a spec marks **RED / scope-trap** (nnum full-LAPACK parity, nvision pretrained
backbones, nnlp neural embeddings), build only the v1 scope in the spec and **STOP and ask me** before going beyond it.

## Instruction given to EACH sub-agent (fill in {SLUG} = nnum, nframe, …)
```
You implement exactly ONE Niao ML library: {SLUG}. Your task card is
niao-ml-libs-plan/cursor-tasks/task-*-{SLUG}.md and your full spec is
niao-ml-libs-plan/specs/*__{SLUG}.md — read BOTH fully and follow them exactly.

Rules:
- Work ONLY inside your target crate (crates/niao_<x>), its niao_libs/{SLUG}/ wrapper, your docs/<LIB>.md,
  your examples/{SLUG}_demo.niao, and your benchmarks/benchmark_{SLUG}.*. Do NOT edit other libraries' crates.
- ZERO new third-party crates. std + existing niao_* only. REUSE, don't duplicate: niao_tensor (GEMM/tensors),
  nnum (arrays/linalg/fft), nrand (RNG), neval (metrics), ntune (CV/splits), ntok (BPE), nembed (neural emb),
  ncsv/njson (IO), ncodec (image codecs), nregex (regex).
- Fast + lightweight: no hot-loop allocations, reuse/pre-size buffers, #[inline] small hot fns, SIMD + scalar fallback.
- Numeric correctness is the gate: test every algorithm against a known-good reference (numpy/scipy/sklearn/
  statsmodels/torchvision values pasted as fixtures) within the STATED tolerance. Fixed seeds — no flaky tests.
- Typed errors ONLY from your reserved block (see the error-code map in MASTER_PLAN.md). Never panic / silent NaN.
- Estimator libs (nlearn/nboost/nts/nnlp): expose the shared fit/predict/transform/score shape.
- Deliver: crate code, niao_libs/{SLUG}/ wrapper (package.json + 0.2.2/lib.json + 0.2.3/lib.json, kind native,
  correct builtin_count, mirror niao_libs/nvalid), docs/<LIB>.md, in-crate unit tests, one examples/{SLUG}_demo.niao,
  one benchmark vs the reference.
- Do NOT edit shared files (niao_libs/catalog.json, workspace Cargo.toml members, niao_runtime wiring, codes.rs).
  List the exact edits you need under your heading in niao-ml-libs-plan/REPORT.md — the orchestrator applies them.
- Acceptance: `cargo test -p crates/niao_<x>` green, your example runs, benchmark meets the spec target.
- Write results (benchmark numbers, deviations, deps/wiring to add, anything deferred to v2) to
  niao-ml-libs-plan/REPORT.md under a heading '## {SLUG}'.
```

## Orchestrator loop per wave
1. `git commit -am "checkpoint before ML wave N"`.
2. Spawn all sub-agents for wave N in parallel with the instruction above.
3. When all report done, **serially** (one file at a time, to avoid Cargo.lock / merge conflicts) apply the shared
   edits they listed:
   - add each new crate to the workspace `Cargo.toml` `members`,
   - add each `{SLUG}` to `niao_libs/catalog.json`,
   - wire runtime builtins + the reserved error-code block into `niao_runtime` / `codes.rs`.
4. Run `cargo check --workspace && cargo test --workspace`.
   - **Green:** `git commit -am "ML wave N complete"`, update `CHANGELOG.md` with the wave summary, go to wave N+1.
   - **Red:** fix or revert. **Never advance on red.**
5. After Wave 2: run every `examples/n*_demo.niao`, confirm all 10 wrappers import, and print a final summary
   (libs shipped, benchmark ratios vs reference, anything deferred to v2).

Begin with **Wave 0 (nnum)** now. Print the Wave 0 plan and wait for my go-ahead before spawning.

---

### Quick reference — libs, crates, error blocks
| Wave | Lib | Crate | Error block | Replaces |
|:---:|-----|-------|:---:|----------|
| 0 | nnum    | niao_num    | 4000–4009 | numpy + scipy.linalg + scipy.fft |
| 1 | nframe  | niao_frame  | 4010–4019 | pandas / polars |
| 1 | nstats  | niao_stats  | 4020–4029 | scipy.stats + statsmodels |
| 1 | noptim  | niao_optim  | 4030–4039 | scipy.optimize |
| 1 | nplot   | niao_plot   | 4040–4049 | matplotlib / seaborn |
| 2 | nlearn  | niao_learn  | 4050–4059 | scikit-learn |
| 2 | nboost  | niao_boost  | 4060–4069 | XGBoost / LightGBM |
| 2 | nts     | niao_ts     | 4070–4079 | statsmodels.tsa / prophet |
| 2 | nnlp    | niao_nlp    | 4080–4089 | nltk / gensim |
| 2 | nvision | niao_vision | 4090–4099 | torchvision / OpenCV |
