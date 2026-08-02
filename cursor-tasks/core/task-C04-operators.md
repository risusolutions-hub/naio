# Task C04 — Operators

**Goal:** close the operator gaps, and prove the C03 harness works by landing a real
language feature through it.

**Read first:** `cursor-tasks/core/MASTER_PLAN.md`, `.cursor/rules/niao-core.mdc`.
**Prerequisite:** C03 green — you need the conformance harness before changing semantics.

This is the first feature that lands in both engines under the new process. Treat the
workflow as part of the deliverable.

---

## Gap 1 — There are no bitwise operators at all

`BinOp` in `crates/niao_ast/src/lib.rs` has 14 variants and none of them are bitwise. In
`crates/niao_lexer/src/lib.rs`, a lone `&` or `|` is a **lex error** — `&&` and `||` are
the only uses of those characters in the entire language.

So `hash = hash ^ (hash >> 16)` cannot be written in Niao. You ship `niao_crypto`,
`niao_codec`, `niao_binary`, and `niao_bignum` as native Rust, and a user cannot write the
equivalent in your language at all.

Add, on `int`:

| Op | Token | Notes |
|---|---|---|
| `&` | `Amp` | bitwise AND |
| `\|` | `Pipe` | bitwise OR |
| `^` | `Caret` | bitwise XOR |
| `~` | `Tilde` | unary NOT (goes in `UnaryOp`) |
| `<<` | `Shl` | shift left |
| `>>` | `Shr` | **arithmetic** shift right (sign-extending) |
| `>>>` | `Ushr` | logical shift right (zero-fill) — include it; without it you cannot express unsigned hashing on a signed-int language |

Semantics, decide and document all of these in `docs/DECISIONS.md`:

- Operands must be `int`. `float` operand → typed error, not a silent truncation.
- Shift count `< 0` or `>= 64` → error, not UB and not a wrapped shift. Name the value.
- `<<` overflow wraps (document it) — do not panic in release and debug differently.

Precedence, slotted between comparison and the existing arithmetic tiers. Follow C/Rust so
nobody has to learn a new table:

```
||  <  &&  <  |  <  ^  <  &  <  == !=  <  < > <= >=  <  << >>  <  + -  <  * / % //  <  unary ! - ~
```

`&` binding tighter than `==` is the C mistake that has caused decades of bugs. **Do not
copy it** — the table above already puts the bitwise ops below equality, which is the Python
and Go choice. State this in `docs/DECISIONS.md` so it does not get "fixed" later.

## Gap 2 — Compound assignment is half-implemented

`AssignOp` has `Assign`, `AddAssign`, `SubAssign`. Add:

`*=` `/=` `%=` `//=` `&=` `|=` `^=` `<<=` `>>=` `>>>=`

Lower them the same way `+=` is lowered today. Verify the target is evaluated **once** for
member and index targets — `arr[f()] += 1` must call `f()` exactly once. Write that test
first; it is the classic bug here.

## Gap 3 — No increment/decrement

`++` / `--` do not exist. **Recommendation: do not add them.** They add parser complexity,
prefix/postfix confusion, and sequence-point questions, and `x += 1` already exists. Record
the decision in `docs/DECISIONS.md` and move on. If you disagree, add postfix only, and
only as a *statement*, never an expression.

---

## Where the changes land

Every one of these, in the same commit:

1. `crates/niao_lexer/src/lib.rs` — tokens. `&`/`|` no longer error alone.
2. `crates/niao_ast/src/lib.rs` — `BinOp`, `UnaryOp`, `AssignOp` variants.
3. `crates/niao_parser/src/lib.rs` — precedence tiers.
4. `crates/niao_ir/src/lib.rs` — lowering (exhaustive match; the compiler will tell you).
5. `crates/niao_bytecode/src/lib.rs` — opcodes. Keep the dispatch table dense.
6. `crates/niao_vm/src/lib.rs` — evaluation.
7. `crates/niao_interpreter/src/lib.rs` — evaluation. **Same semantics.**
8. `crates/niao_format/src/lib.rs` — the formatter must print the new operators with
   correct spacing and parenthesisation. It has **0 tests** — see below.
9. `crates/niao_lint/src/lib.rs` — at minimum, warn on `a & b == c`, where the new
   precedence is correct but probably not what the user meant.
10. `crates/niao_errors/src/codes.rs` — new codes for non-int operands and bad shift counts.
11. `docs/grammar.ebnf` — the precedence chain.
12. `vscode-niao/syntaxes/` — highlight the new operators.

## The formatter has zero tests

`niao_format` has **0 tests** and you are about to teach it new operators. A formatter with
no round-trip test corrupts source code silently. Before touching it, add:

- `parse(format(src)) == parse(src)` — formatting never changes meaning.
- `format(format(src)) == format(src)` — formatting is idempotent.

Run both over every file in `examples/` and `tests/`. Expect failures on the existing tree
— fix them or record them, but land the property tests in this task either way.

---

## Tests

- Unit tests per operator in the lexer, parser (precedence), VM, and interpreter.
- **Conformance cases in `tests/conformance/` for every new operator** — this is the point
  of the task. Both engines must agree.
- Precedence: for ~20 mixed expressions, assert the parse tree shape, not just the result.
  A wrong tree that happens to produce the right number on your test input is the failure
  mode here.
- Error cases: float operand, negative shift, shift ≥ 64, `~` on a string — assert code
  and position.
- Boundary values: `i64::MIN`, `i64::MAX`, `-1 >> 1` vs `-1 >>> 1`, `1 << 63`.
- `arr[f()] &= 3` evaluates `f()` exactly once.
- One `.niao` example under `examples/` — a real hash function (FNV-1a or xxhash-lite) that
  was impossible to write before this task. That is the proof the feature works.

---

## Acceptance

- [ ] All bitwise ops, `~`, all three shifts, and all compound assignments work in **both**
      engines with identical results.
- [ ] Precedence matches the documented table; ~20 tree-shape tests prove it.
- [ ] Non-int operands and bad shift counts are typed errors with spans.
- [ ] `niao_format` has round-trip + idempotence property tests, green over `examples/`
      and `tests/`.
- [ ] Conformance cases added for every new operator.
- [ ] `examples/hash_fnv.niao` (or similar) runs and produces known-correct values.
- [ ] `docs/grammar.ebnf` and `docs/DECISIONS.md` updated with the precedence rationale.
- [ ] `cargo check --workspace`, `cargo test --workspace`, `niao test tests` green.
- [ ] `CHANGELOG.md` updated.
- [ ] No regression in `benchmarks/baseline.json` — adding opcodes must not slow the
      dispatch loop. If the jump table grew past a cache line, say so with numbers.
