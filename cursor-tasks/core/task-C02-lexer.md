# Task C02 — Lexer Correctness

**File:** `crates/niao_lexer/src/lib.rs` (561 lines, 7 tests).
**Goal:** fix six real bugs, then take the lexer from 7 tests to 100+.

**Read first:** `cursor-tasks/core/MASTER_PLAN.md`, `.cursor/rules/niao-core.mdc`.
**Prerequisite:** C01 green (you need CI before touching the front end).

This task is self-contained — it does not change parser, IR, or VM semantics. Every fix
below is a bug an actual user will hit.

---

## Bug 1 — String escapes silently swallow typos, and half of them are missing

`read_string()` currently does:

```rust
match escaped {
    'n'  => s.push('\n'),
    't'  => s.push('\t'),
    '\\' => s.push('\\'),
    '"'  => s.push('"'),
    other => s.push(other),      // <-- silently drops the backslash
}
```

Two problems. `"\q"` silently becomes `q` instead of erroring. And there is **no way to
write `\r`, a null byte, or any non-ASCII codepoint in a Niao string literal** — which
means HTTP header construction written in Niao cannot emit CRLF.

Fix:

- Add `\r`, `\0`, `\'`.
- Add `\xNN` — exactly two hex digits, value `0x00..=0x7F`. Error `E0003_INVALID_ESCAPE`
  on a bad digit; error on `> 0x7F` telling the user to use `\u{...}` for non-ASCII.
- Add `\u{...}` — 1 to 6 hex digits, validated through `char::from_u32`. Reject surrogates
  `D800..=DFFF` and anything above `10FFFF` with a message naming the offending value.
- Replace the `other =>` arm with `E0003_INVALID_ESCAPE`, span pointing at the escape
  itself (both chars), message listing the valid escapes.

Add `E0003_INVALID_ESCAPE` to `crates/niao_errors/src/codes.rs` in the `E0001`–`E0099`
lexer band, with a doc comment.

## Bug 2 — Number literals are missing most of the standard set

`read_number()` handles decimal and `0x` hex. Everything else is absent:

- `0b1010` binary → currently lexes as `Int(0)` then `Ident("b1010")`.
- `0o755` octal → same failure.
- `1_000_000` digit separators → lexes as `Int(1)` then `Ident("_000_000")`.
- `1e9`, `1.5e-3`, `2E+10` scientific notation → `1e9` lexes as `Int(1)` then `Ident("e9")`.

Every one of these produces a baffling downstream parse error instead of a lexer error.

Fix: support `0b`, `0o`, `_` separators in all radixes (never leading, never trailing,
never doubled), and exponents on both int-looking and float-looking mantissas (`1e9` is a
float). Add `E0004_INVALID_NUMBER` for malformed literals — bad digit for the radix, empty
digits after a prefix, a trailing `_`, an `e` with no exponent digits.

## Bug 3 — Integer overflow reports the wrong error

Both `parse::<i64>()` and `i64::from_str_radix` failures currently map to
`LexError::UnexpectedChar { ch: first, ... }`. A user who writes `99999999999999999999`
gets told there is an unexpected character `9`.

Fix: `E0005_INTEGER_OVERFLOW`, message stating the literal does not fit in a 64-bit signed
integer and giving the valid range. Same treatment for float literals that parse to
infinity — `E0006_FLOAT_OVERFLOW`.

## Bug 4 — `//` is heuristically comment-or-floor-division

`slash_slash_is_floor_div()` (line 442) guesses whether `//` starts a comment or is the
floor-division operator, based on the previous non-whitespace character and a lookahead
scan. It gets this wrong:

```niao
let x = 1 // (see note)      # parsed as FLOOR DIVISION, not a comment
let y = n // 2               # parsed as floor division  (intended)
// (a comment)               # parsed as a comment       (line start)
```

The first line is a silent miscompile. No heuristic will make this safe — the grammar is
genuinely ambiguous.

**Decide and commit.** Recommended: `//` is **always** a line comment (matches C, Rust,
JS, Go, Java — every language a newcomer arrives from), and floor division moves to a
builtin `floordiv(a, b)` plus, optionally, a `%/` operator. This is a **breaking change**:

- Grep `examples/`, `tests/`, `benchmarks/`, `niao_libs/`, and `docs/` for `//` used as
  division and migrate every site.
- Delete `slash_slash_is_floor_div`, `slash_slash_at_line_start`,
  `prev_non_whitespace_before`, and `skip_spaces_on_line` if they become dead.
- Keep `TokenKind::FloorDiv` and `BinOp::FloorDiv` — only the spelling changes.
- Note it in `CHANGELOG.md` under a **Breaking** heading.

If you choose the other direction instead, write the reasoning into
`docs/DECISIONS.md` — but do not leave the heuristic in place.

## Bug 5 — Two comment syntaxes, no block comments

`#` is handled in `skip_whitespace_and_comments()`; `//` is handled in `next_token()`.
Having both is unusual and neither is documented in `docs/grammar.ebnf`.

- Add `/* ... */` block comments, **nested** (so commenting out a region containing a
  comment works). Unterminated → `E0007_UNTERMINATED_COMMENT` with the span of the opener.
- Keep `#` (shebang-friendly, already used across `examples/`). Handle a leading
  `#!/usr/bin/env niao` line explicitly so scripts are executable on Unix.
- Move all comment handling into `skip_whitespace_and_comments()` so there is one place
  that decides what a comment is.

## Bug 6 — ASCII-only identifiers

`read_ident()` gates on `is_ascii_alphabetic()` / `is_ascii_alphanumeric()`.

Fix: use `unicode_ident`-equivalent rules implemented by hand against `std` — start =
`XID_Start` or `_`, continue = `XID_Continue`. `char::is_alphabetic()` /
`char::is_alphanumeric()` from `std` is an acceptable approximation; state the choice in a
comment. **No new crates.**

## Also: reserved type keywords

`int`, `float`, `string`, `bool`, `void`, `array` are unconditionally keywords, so no user
can name a variable or struct field `string` or `array` — both of which are common.

Do **not** fix this here; it needs parser context-sensitivity and belongs with C05.
Open `cursor-tasks/core/task-C05b-contextual-keywords.md` describing the problem so it is
not lost.

---

## Tests

The lexer has 7 tests. That is the root cause of every bug above. Take it past 100:

- One test per escape, valid and invalid, asserting the **error code and the (line, col)**.
- One per numeric form: each radix, separators in each radix, every exponent shape,
  overflow at the i64 boundary, and each malformed variant.
- Comments: `#`, `//`, `/* */`, nested block, unterminated block, comment at EOF with no
  trailing newline, shebang.
- Strings: empty, unterminated, newline inside, all escapes, multi-byte UTF-8 content.
- Identifiers: leading `_`, digits inside, non-ASCII, and each reserved word confirmed to
  lex as its keyword token.
- Spans: for a known input, assert exact `start`/`end`/`line`/`col` on every token,
  including after a multi-byte character. **Span arithmetic is byte-based while column
  counting is char-based — write a test that would catch it if those disagree.**
- Property test (hand-rolled, no `proptest`): for a table of source strings, `tokenize()`
  never panics and either returns tokens or a `LexError`.

---

## Acceptance

- [ ] All six bugs fixed, each with tests asserting code + position.
- [ ] `crates/niao_lexer` has 100+ tests, all green.
- [ ] New codes `E0003`–`E0007` added to `codes.rs` with doc comments, in the lexer band.
- [ ] No `unwrap`/`expect`/`panic!` on any path reachable from source text.
- [ ] `//` ambiguity resolved one way, all call sites migrated, decision recorded.
- [ ] `cargo check --workspace`, `cargo test --workspace`, `niao test tests` green.
- [ ] `CHANGELOG.md` updated, with a **Breaking** section if `//` changed meaning.
- [ ] No regression in `benchmarks/baseline.json`. The lexer is on the compile path — if
      Unicode identifiers cost measurable throughput, keep an ASCII fast path.
