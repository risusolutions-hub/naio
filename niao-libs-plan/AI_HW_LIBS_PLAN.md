# Niao AI + Hardware Libraries — 8 New Native Libs

Goal: make Niao the fastest, lightest way to run and tune AI on any device. Eight new
std-only native modules in `crates/niao_runtime` — zero new Rust dependencies. Hardware
readings use vendor tools when present (`nvidia-smi`, `rocm-smi`, `wmic`/`powershell`,
`/proc`, `/sys`) with a 1-second cache; when a reading is unavailable the API says so
honestly (`nil` / `available: false`) — no fake numbers.

## Honest enforcement model (important)

Budgets and thermal protection are **cooperative**: they control workloads that run
*through Niao* — worker-pool sizes, recommended batch/thread counts, paced hot loops,
allocation guards. They do not hard-cap other processes or reprogram the hardware.
This is the same model used by every user-space runtime; combined with the safety
monitor it is what actually keeps a device cool: fewer threads, paced decode loops,
smaller batches, deferred work.

## The 8 libraries

| # | Lib | Errors | Purpose |
|---|-----|--------|---------|
| 1 | `ncpu` | 2700–2702 | Cores (logical/physical), arch/brand, system+process usage %, temp (when exposed), user limit (`set_limit(40)`), `threads()` = allowed workers under limit+throttle |
| 2 | `ngpu` | 2710–2713 | Detect GPUs, VRAM total/used, utilization %, temperature; `set_limit(pct)`, `set_max_temp(c)`, `ok()`, `wait_cool()`; backends: nvidia-smi → rocm-smi → generic detect |
| 3 | `nram` | 2720–2722 | Total/available/process MB, usage %, budget (`set_limit_mb/pct`), `ok(extra_mb)`, `pressure()` low→critical, `headroom_mb()`, GC hint |
| 4 | `nnpu` | 2730–2732 | Best-effort NPU detection (Intel NPU / Apple ANE / Snapdragon / /dev/accel), capability report, budget mirror; honest `available: false` elsewhere |
| 5 | `ndevice` | 2740–2743 | The brain: `detect()` everything, profiles (eco/balanced/performance), background safety guard with max-temp auto-throttle (levels 0–3), `pace()`, `threads()`, `best_device(task)`, `status()` |
| 6 | `ntune` | 2750–2753 | Training helpers: LR schedules (cosine/step/exp/warmup), early stopping handles, grid + random hyperparameter search, k-fold + train/test split, run trackers |
| 7 | `neval` | 2760–2763 | Model testing: accuracy/precision/recall/F1/confusion, MAE/MSE/RMSE/R², exact-match + token-F1 + similarity for text, `bench(fn, iters)` latency stats (p50/p95), `compare(a, b)` |
| 8 | `ntok` | 2770–2773 | Byte-level BPE tokenizer (GPT-2 style vocab.json + merges.txt), encode/decode/count, per-word cache, `count_approx()` heuristic, `chunk(text, max_tokens)`, `fit()` for context budgeting |

## Safety / thermal design (ndevice guard)

- `ndevice.guard_start({interval_ms: 1000, gpu_max_temp: 80, cpu_max_temp: 90, ram_max_pct: 90})`
  spawns one background sampler thread (std::thread, no async runtime).
- Each tick reads cached probes and sets a global **throttle level**:
  `0` ok · `1` warm (≥ max−10°C) · `2` hot (≥ max) · `3` critical (≥ max+5°C or RAM critical).
- Enforcement points consult atomics (one load, ~1 ns):
  - `ndevice.threads()` / `ncpu.threads()` shrink worker counts (level 2 halves, level 3 → 1).
  - `ndevice.pace()` sleeps 0/2/8/25 ms by level — drop it into hot loops (training steps,
    LLM decode, batch jobs) for automatic cool-down.
  - `ngpu.ok()` / `nram.ok()` gate new heavy work.
  - `ngpu.wait_cool(target?, timeout_ms?)` blocks between batches until the GPU cools.
- User limits are respected even with the guard off: `threads()` honors `ncpu.set_limit(40)`,
  `ngpu.ok()` honors `set_limit`/`set_max_temp` on demand-read temps.

## Probe backends

| Reading | Linux | Windows | macOS |
|---------|-------|---------|-------|
| CPU usage | `/proc/stat` delta | `wmic cpu get loadpercentage` | `ps -A -o %cpu` sum (approx) |
| CPU temp | `/sys/class/thermal`, `hwmon` | usually unavailable (nil) | unavailable (nil) |
| RAM | `/proc/meminfo` | `wmic OS get Free/Total` | `sysctl hw.memsize` + `vm_stat` |
| Process RSS | existing `mem::process_rss_bytes()` (all platforms) | 〃 | 〃 |
| GPU | `nvidia-smi` → `rocm-smi` → `lspci` detect | `nvidia-smi` → `wmic VideoController` | `system_profiler` detect |
| NPU | `/dev/accel*`, cpuinfo hints | PnP/CPU-brand hints | arch = Apple Silicon → ANE |

All command probes run with a watchdog timeout (500 ms) and cache results for 1 s, so
polling APIs stay cheap even in tight loops.

## Integration checklist (per lib — same as stdlib expansion)

module file → error codes → lib.rs (mod/extend/define/paths/export) → niao_pkg catalog →
niao_libs package + catalog.json → docs/<NAME>.md → examples/<name>_demo.niao.
Shared plumbing lives in `crates/niao_runtime/src/hw.rs` (not user-facing).
