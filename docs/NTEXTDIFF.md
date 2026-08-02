# ntextdiff standard library

Line/word text diff, unified patches, 3-way merge. Native Rust implementation (~difflib + diff-match-patch subset; beside `ndiff` structural).

## Import

```niao
import "ntextdiff"
```

Paths `import "std/ntextdiff"` and `import "ntextdiff"` are equivalent. Flat builtins (`ntextdiff_compare`, `ntextdiff_merge`, …) are also available globally after import.

## Quick start

```niao
import "ntextdiff"

let a = "hello\nworld\n"
let b = "hello\nthere\n"

print(ntextdiff.compare(a, b))           // ndiff-style lines
print(ntextdiff.unified(a, b, {join: true}))
print(ntextdiff.ratio(a, b))             // 0.0..1.0

let patch = ntextdiff.patch_make(a, b)
let applied = ntextdiff.patch_apply(a, patch)
print(applied.text)

let merged = ntextdiff.merge("base\nline\n", "base\nOURS\n", "base\nTHEIRS\n")
print(merged.merged)
print(len(merged.conflicts))

let m = ntextdiff.matcher(a, b)
print(ntextdiff.matcher_ratio(m))
ntextdiff.close(m)
```

## Line diff

| Method | Description |
|--------|-------------|
| `ntextdiff.compare(a, b, opts?)` | ndiff-style line compare (`  ` / `- ` / `+ ` prefixes). |
| `ntextdiff.unified(a, b, opts?)` | Unified diff lines (or string when `{join: true}`). |
| `ntextdiff.context(a, b, opts?)` | Context diff (difflib.context_diff subset). |
| `ntextdiff.line_changes(a, b, opts?)` | Structured changes `[{tag, value}, …]`. |
| `ntextdiff.splitlines(text, opts?)` | Split lines; `{keepends: true}` keeps terminators. |
| `ntextdiff.restore(which, lines)` | Reconstruct text 1 or 2 from compare output. |

## Similarity & opcodes

| Method | Description |
|--------|-------------|
| `ntextdiff.ratio(a, b, opts?)` | Similarity ratio 0..1 (SequenceMatcher.ratio). |
| `ntextdiff.quick_ratio(a, b, opts?)` | Upper-bound quick ratio. |
| `ntextdiff.real_quick_ratio(a, b, opts?)` | Cheaper upper bound. |
| `ntextdiff.opcodes(a, b, opts?)` | `[{tag, i1, i2, j1, j2}, …]`. |
| `ntextdiff.matching_blocks(a, b, opts?)` | `[{a, b, size}, …]`. |

## Word & character diff

| Method | Description |
|--------|-------------|
| `ntextdiff.word_diff(a, b, opts?)` | Unicode word-token changes. |
| `ntextdiff.word_diff_inline(a, b, opts?)` | Inline `{+added}` / `{-removed}` string. |
| `ntextdiff.char_diff(a, b, opts?)` | Character diff with semantic cleanup (`[{op, text}]`, op −1/0/1). |
| `ntextdiff.char_diff_raw(a, b, opts?)` | Raw character diff without cleanup. |
| `ntextdiff.levenshtein(a, b)` | Edit distance from char diff. |

## Patches & merge

| Method | Description |
|--------|-------------|
| `ntextdiff.patch_make(a, b, opts?)` | Unified patch text; `{dmp: true}` for diff-match-patch format. |
| `ntextdiff.patch_apply(text, patch, opts?)` | Returns `{text, applied: [bool, …]}`. Auto-detects unified vs DMP. |
| `ntextdiff.merge(base, ours, theirs, opts?)` | 3-way line merge → `{merged, conflicts}`. |

## Cached matcher

| Method | Description |
|--------|-------------|
| `ntextdiff.matcher(a, b, opts?)` | Compile reusable matcher → handle. |
| `ntextdiff.close(handle)` | Free handle. |
| `ntextdiff.matcher_ratio(handle)` | Ratio using cached sequences. |
| `ntextdiff.matcher_quick_ratio(handle)` | Quick ratio. |
| `ntextdiff.matcher_real_quick_ratio(handle)` | Real quick ratio. |
| `ntextdiff.matcher_opcodes(handle)` | Opcodes. |
| `ntextdiff.matcher_matching_blocks(handle)` | Matching blocks. |

## Parallel batch

| Method | Description |
|--------|-------------|
| `ntextdiff.parallel_diff(pairs, opts?)` | `[{unified, ratio}, …]` over `[{from, to}, …]`. |
| `ntextdiff.parallel_ratio(pairs, opts?)` | Ratios only. |
| `ntextdiff.parallel_unified(pairs, opts?)` | Unified diff line arrays. |

`opts.threads` defaults to CPU count.

## Options

| Option | Default | Description |
|--------|---------|-------------|
| `granularity` | `"line"` | `"line"`, `"word"`, `"char"`, `"unicode_word"`. |
| `algorithm` | `"myers"` | `"myers"` or `"patience"`. |
| `ignore_whitespace` | `false` | Collapse whitespace before compare. |
| `ignore_case` | `false` | Case-fold lines/tokens. |
| `context` | `3` | Context lines for unified/context diff. |
| `join` | `false` | Return string instead of line array. |
| `fromfile` / `tofile` | `""` | File labels in diff headers. |
| `lineterm` | `"\n"` | Line terminator when joining/applying. |
| `fuzz` | `0` | Fuzz factor for diff-match-patch apply. |
| `dmp` | `false` | Use diff-match-patch patch format in `patch_make`. |
| `marker_*` | git-style | Conflict markers for `merge`. |

## Limits & errors

- Max input size per string: `ntextdiff.max_input_bytes` (16 MiB).
- Errors return `ntextdiff_error` values with codes `e3563`–`e3566`.

## See also

- [`ndiff`](NDIFF.md) — structural value/object diff
- [`nstr`](NSTR.md) — string utilities
