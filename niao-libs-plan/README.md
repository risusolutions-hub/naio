# niao-libs-plan

Complete plan to reimplement every third-party Rust dependency as a native Niao library.

- `MASTER_PLAN.md` — target crates, reality tiers (green/amber/red), parallel WAVES, ground rules.
- `MULTIAGENT_PROMPT.md` — paste into Cursor multi-agent mode; orchestrator + per-agent instructions.
- `specs/` — 50 detailed specs, one per library (blueprint, API, perf target, tests, risk).
- `REPORT.md` — agents append results here (created on first run).

## Quick start
1. Open `C:\Risu\Neko` in Cursor, enable multi-agent/parallel mode.
2. Paste the contents of `MULTIAGENT_PROMPT.md`.
3. It runs wave by wave, parallel agents within each wave, verifying the workspace between waves.

## Read the risk notes
`niao_tls` (rustls), `niao_jit` (cranelift), and the ML crates are flagged RED in MASTER_PLAN.md.
These are security-critical or person-years of work — the specs describe FFI/keep fallbacks. The
agents will pause and ask before hand-rolling them.
