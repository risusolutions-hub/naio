# Niao Core Language — Master Plan

**Goal:** make Niao the fastest and best language of its class.
**Premise:** it cannot be either until the core is as strong as the ecosystem around it.

## Where we actually are (measured, Aug 2026, v0.2.3 @ 5802f243)

| Layer | Lines | Tests |
|---|---:|---:|
| `niao_lexer` | 561 | 7 |
| `niao_parser` | 1,136 | 10 |
| `niao_ast` | 363 | 0 |
| `niao_ir` | 551 | 0 |
| `niao_bytecode` | 1,735 | 11 |
| `niao_vm` | 5,955 | 18 |
| `niao_interpreter` | 1,610 | 3 |
| **core total** | **~11.9k** | **49** |
| 130 library crates | — | **2,056** |

The ecosystem is 130 crates wide. The language is 1,136 lines of parser deep.
Every library rests on a compiler with 49 tests. **That ratio is the thing to fix.**

## Why correctness before speed

"Fastest" is downstream of the type checker, not in tension with it. Today the VM cannot
do the optimisations that make a language fast, because it does not know anything about
its values:

- No static types → every value is tagged and every operation dispatches on the tag.
- No static types → no unboxed locals, no specialised arithmetic, no monomorphisation.
- No static types → inline caches are guesses instead of guarantees.
- No resolved names → globals are hash lookups instead of slot indices.

`niao_sema` (task C05) is not a detour from performance. It is the precondition for it.
Tasks C01–C04 exist so that C05–C07 can be landed without silently breaking the language.

## Execution order

Run C01 → C08 in order. Review `git diff` after each. Do not start a task until the
previous one is green.

| # | Task | What it buys |
|---|---|---|
| **C01** | Foundation & CI | A safety net. Nothing else is safe without it. |
| **C02** | Lexer correctness | Fixes real bugs; self-contained; lexer 7 → 100+ tests |
| **C03** | No silent drops + conformance harness | Makes engine divergence *visible* |
| **C04** | Operators (bitwise, compound assign) | First feature landed through the new harness |
| **C05** | `niao_sema` — the type checker | The unlock. Errors, speed, and the LSP all follow. |
| **C06** | Closures / first-class functions | Largest expressiveness gap in the language |
| **C07** | `match` + enums | Exhaustiveness, real error handling |
| **C08** | Language spec + LSP | Adoption |

C01–C04 are days each. C05–C07 are the cycle. C08 follows C05.

## Ground rules (apply to EVERY task)

These are enforced by `.cursor/rules/niao-core.mdc`, which is loaded automatically when
you touch core files. Read it. The short version:

- **ZERO new third-party crates.** `std` + existing `niao_*` only. Hand-write it.
- **Two engines, one language.** Semantics land in `niao_vm` AND `niao_interpreter` in the
  same commit, with a conformance test proving they agree.
- **No `_ => {}`** in any lowering or eval match. Unhandled constructs return a typed error.
- **No panics on user input.** Malformed source → `NiaoError` with a code and a span.
- **Every error gets a code** in `crates/niao_errors/src/codes.rs` and a span pointing at
  the narrowest range that is actually wrong.
- **Tests are the deliverable.** A feature without tests is not done. Assert error *codes*
  and *positions*, not just failure.
- **No perf regressions** against `benchmarks/baseline.json` without an explicit, written
  justification.
- Ends with `cargo check --workspace`, `cargo test --workspace`, `niao test tests` all
  green, plus one line in `CHANGELOG.md`.

## Definition of "perfect" for this wave

When C01–C08 are done:

1. CI is green on Windows, Linux, and macOS on every push.
2. `tests/conformance/` runs every language feature through both engines and they agree.
3. Core pipeline tests are in the high hundreds, not 49.
4. `fn add(a: int, b: int) -> int` actually rejects `add("x", [1])` — at compile time,
   with a code, a span, and a message that says what to do about it.
5. `docs/LANGUAGE.md` and `docs/grammar.ebnf` are generated from or verified against the
   real parser, not written from memory.
6. Closures and `match` exist.
7. No benchmark in `benchmarks/baseline.json` is slower than it is today, and the ones
   that touch typed arithmetic are meaningfully faster.
