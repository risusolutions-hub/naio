# nfmt standard library

String templating and human-friendly number formatting: `{}` templates, thousands separators, fixed precision, hex/oct/bin, humanized bytes/durations/counts, ordinals.

## Import

```niao
import "nfmt"
```

Paths `import "std/nfmt"` and `import "nfmt"` are equivalent. Flat builtins (`nfmt_fmt`, `nfmt_number`, …) are also available globally after import.

## Quick start

```niao
import "nfmt"

print(nfmt.fmt("{} scored {} points", "vivek", 42))
print(nfmt.fmt("hi {name}!", {name: "Niao"}))
print(nfmt.number(1234567))          // 1,234,567
print(nfmt.bytes(1500000))           // 1.5 MB
print(nfmt.duration_ms(93784000))    // 1d 2h
```

## Templates — `nfmt.fmt(template, ...args)`

| Placeholder | Meaning |
|-------------|---------|
| `{}` | Next positional argument. |
| `{0}`, `{1}` | Indexed positional argument. |
| `{name}` | Key lookup in the **last** argument when it is an object. |
| `{{` / `}}` | Literal braces. |

Missing placeholders return a catchable `error` value.

## Numbers

| Method | Description |
|--------|-------------|
| `nfmt.number(x, decimals?, sep?)` | Thousands grouping; decimals default `0` for ints, `2` for floats; sep default `","`. |
| `nfmt.fixed(x, decimals)` | Fixed decimal places. |
| `nfmt.sci(x, decimals?)` | Scientific notation (default 3). |
| `nfmt.percent(x, decimals?)` | `0.425` → `42.5%` (default 1). |
| `nfmt.currency(x, symbol, decimals?)` | `-$1,234.50` style (default 2). |
| `nfmt.hex(n, width?)` / `oct(n, width?)` / `bin(n, width?)` | `0x`/`0o`/`0b` with optional zero-pad width. |
| `nfmt.ordinal(n)` | `1st`, `2nd`, `3rd`, `12th`, `23rd`. |

## Humanizers

| Method | Description |
|--------|-------------|
| `nfmt.bytes(n)` | Decimal units: `1.5 MB` (1 kB = 1000 B). |
| `nfmt.bytes_bin(n)` | Binary units: `1.5 KiB` (1 KiB = 1024 B). |
| `nfmt.count(n)` | `1.2k`, `3.4M`, `5.6B`, `7.8T`. |
| `nfmt.duration_ms(ms)` | `500ms`, `1m 3s`, `1d 2h` (largest two units). |

## Errors

| Code | Meaning |
|------|---------|
| 2630 | Wrong argument count. |
| 2631 | Formatting failed (unclosed `{`, missing placeholder value). |
| 2632 | Type mismatch. |
