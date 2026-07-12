# Niao stdlib review — 2026-07

A full review of all **112 Niao libraries**: code read, defects fixed, and a v0.2.4 roadmap of
features, changes, and speed wins. **This whole folder is a staging area — nothing in your live tree
was modified.** Review, then merge what you want.

## Read in this order

1. **`MASTER_REPORT.md`** — start here. Scope, headline numbers, the 112-lib inventory, cross-cutting
   findings, defects, and the top-10 highest-impact changes.
2. **`PERFORMANCE.md`** — deep, code-grounded optimization notes (the "make it faster" part), each
   citing the exact file/loop, with how to measure.
3. **`ROADMAP_v0.2.4.md`** — the per-library breakdown: for every lib, what to **Add**, **Change**,
   and speed up. This is the "update in each library" deliverable.

## Staged artifacts

| Path | What |
|---|---|
| `manifest-fixes/` | 35 repaired `package.json` / `lib.json` files + apply instructions |
| `docs-proposed/` | New docs for undocumented libs (`IO.md`, `NOS.md`, `CORE.md`) + `DOCS_STATUS.md` for the rest |
| `harness/` | Runnable per-lib benchmark harness (`run_bench.py` + `benches/*.niao`) to get real numbers on Windows |
| `inventory.json` | Machine-readable inventory (lib, LOC, builtins, doc/manifest state) |

## The one honest caveat

The review ran in a Linux sandbox with no Rust toolchain and no wine, and your built `niao` is a
Windows binary — so **the code was read but not executed**. Every performance figure is either a
**measured number already in your repo** (v0.2.2 baselines, clearly labeled) or a **static estimate**
from reading the actual loops. The `harness/` exists to convert the estimates into measured numbers on
your machine. Land one optimization, run the harness, record the real delta.

## Scale, honestly

"Make all 112 perfect, superfast, with new features, tested" is person-months of Rust work. What this
pass delivers is the **complete map and plan** to get there — every lib reviewed, every defect found
and staged, and a prioritized, code-grounded backlog — plus the harness to measure progress. Execute
it in the waves suggested at the end of `ROADMAP_v0.2.4.md`, one lib at a time, benchmark each.
