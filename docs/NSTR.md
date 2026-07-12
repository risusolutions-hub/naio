# nstr standard library

Fast, Unicode-correct string utilities: case conversions, trimming/padding, search, split/join, wrapping, slugify, and edit-distance helpers. Std-only native Rust — zero dependencies.

## Import

```niao
import "nstr"
```

Paths `import "std/nstr"` and `import "nstr"` are equivalent. Flat builtins (`nstr_upper`, `nstr_split`, …) are also available globally after import.

## Quick start

```niao
import "nstr"

print(nstr.snake("HelloWorldHTTP"))          // hello_world_http
print(nstr.slugify("Hello, World! 42"))      // hello-world-42
print(nstr.pad_start("7", 3, "0"))           // 007
print(nstr.similarity("kitten", "sitting"))  // 0.571...
let parts = nstr.split("a,b,c", ",")         // ["a", "b", "c"]
print(nstr.join(parts, " | "))               // a | b | c
```

## Case conversion

| Method | Description |
|--------|-------------|
| `nstr.upper(s)` / `nstr.lower(s)` | Full Unicode upper/lowercase. |
| `nstr.title(s)` | Uppercase first letter of every word. |
| `nstr.capitalize(s)` | Lowercase everything, uppercase first char. |
| `nstr.swap_case(s)` | Invert case of every letter. |
| `nstr.snake(s)` | `helloWorldHTTP` → `hello_world_http`. |
| `nstr.camel(s)` | `hello_world` → `helloWorld`. |
| `nstr.pascal(s)` | `hello world` → `HelloWorld`. |
| `nstr.kebab(s)` | `HTTPServerV2` → `http-server-v2`. |
| `nstr.constant(s)` | `helloWorld` → `HELLO_WORLD`. |

Word splitting understands spaces, `-`, `_`, `.` separators and camelCase / acronym boundaries.

## Trim, pad, shape

| Method | Description |
|--------|-------------|
| `nstr.trim(s)` / `trim_start(s)` / `trim_end(s)` | Whitespace trim. |
| `nstr.trim_chars(s, chars)` | Trim any of the given characters from both ends. |
| `nstr.pad_start(s, width, fill?)` / `pad_end(...)` | Pad to `width` chars (default fill `" "`). |
| `nstr.center(s, width, fill?)` | Center within `width`. |
| `nstr.repeat(s, n)` | Repeat (guarded against >64 MiB results). |
| `nstr.reverse(s)` | Reverse by chars. |
| `nstr.truncate(s, max, suffix?)` | Cut to `max` chars incl. suffix (default `"..."`). |
| `nstr.wrap(s, width)` | Greedy word wrap, returns text with `\n`. |
| `nstr.indent(s, prefix)` | Prefix every non-empty line. |
| `nstr.dedent(s)` | Remove common leading whitespace. |

## Split & join

| Method | Description |
|--------|-------------|
| `nstr.split(s, sep)` | Array of parts; empty `sep` splits into chars. |
| `nstr.split_n(s, sep, n)` | At most `n` parts. |
| `nstr.split_ws(s)` | Split on whitespace runs. |
| `nstr.join(arr, sep)` | Join array/StringArray of strings. |
| `nstr.lines(s)` | Split into lines (no trailing `\n`). |

## Search & replace

| Method | Description |
|--------|-------------|
| `nstr.contains(s, sub)` / `starts_with` / `ends_with` | Substring tests. |
| `nstr.index_of(s, sub)` / `last_index_of(s, sub)` | Char index or `-1`. |
| `nstr.count(s, sub)` | Non-overlapping occurrences. |
| `nstr.replace(s, from, to)` | Replace all. |
| `nstr.replace_n(s, from, to, n)` | Replace first `n`. |
| `nstr.remove_prefix(s, p)` / `remove_suffix(s, p)` | Strip if present. |

## Slicing & chars

| Method | Description |
|--------|-------------|
| `nstr.substring(s, start, end?)` | Char-based; negative indices count from the end; clamps. |
| `nstr.char_at(s, i)` | Char at index (negative ok); bounds `error` if outside. |
| `nstr.chars(s)` | Array of single-char strings. |
| `nstr.char_len(s)` | Unicode char count. |
| `nstr.byte_len(s)` | UTF-8 byte count. |
| `nstr.ord(s)` / `nstr.chr(n)` | First char code point / code point to string. |

## Text helpers

| Method | Description |
|--------|-------------|
| `nstr.slugify(s, sep?)` | URL slug (default `-`), keeps Unicode letters, lowercases. |
| `nstr.levenshtein(a, b)` | Edit distance (char-based, two-row DP). |
| `nstr.similarity(a, b)` | `1.0 - distance / max_len` in `0..=1`. |

## Checks

`nstr.is_blank(s)`, `is_digit(s)`, `is_alpha(s)`, `is_alnum(s)`, `is_upper(s)`, `is_lower(s)`, `is_ascii(s)` — all return `bool`. `is_digit`/`is_alpha`/`is_alnum` are false for empty strings.

## Errors

| Code | Meaning |
|------|---------|
| 2600 | Wrong argument count. |
| 2601 | Operation failed (e.g. `chr()` invalid code point, result too large). |
| 2602 | Type mismatch. |
| 2603 | Index out of bounds (returned as catchable `error` value). |
