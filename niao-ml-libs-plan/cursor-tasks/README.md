# cursor-tasks — one file per ML library

Self-contained task cards for Cursor agents. Each card is the actionable checklist; the full blueprint,
API surface, and test list live in the matching `../specs/*.md`. **Read `../MASTER_PLAN.md` and your spec
before starting.**

## Order (waves — see MASTER_PLAN.md)
- **Wave 0:** `task-00-nnum.md`  ← build first, alone.
- **Wave 1 (parallel):** `task-01-nframe.md`, `task-02-nstats.md`, `task-03-noptim.md`, `task-04-nplot.md`
- **Wave 2 (parallel):** `task-05-nlearn.md`, `task-06-nboost.md`, `task-07-nts.md`, `task-08-nnlp.md`, `task-09-nvision.md`

Do not start a wave until the previous one is green on `cargo check --workspace && cargo test --workspace`.

## Every task obeys the same ground rules
See `../cursor-rules.md` (copy to `.cursor/rules/niao-ml.mdc`). In short: zero new third-party crates; reuse
`niao_tensor`/`nnum`/`neval`/`ntune`/`ntok`/`ncodec`/etc.; numeric tests vs a known reference within a stated
tolerance; typed errors from your reserved 40xx block; ship the wrapper + example + benchmark + docs + REPORT entry;
never edit shared files (catalog/Cargo members/codes.rs) — report them to the orchestrator.
