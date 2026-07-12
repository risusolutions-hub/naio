# nmarkdown standard library

Lightweight Markdown parsing: HTML conversion, plain-text stripping, and heading extraction. Std-only, line-based — no external crate.

## Import

```niao
import "nmarkdown"
```

Paths `import "std/nmarkdown"` and `import "nmarkdown"` are equivalent. Flat builtins (`nmarkdown_to_html`, `nmarkdown_strip`, …) are also available globally after import.

## Quick start

```niao
import "nmarkdown"

let md = "# Hello\n\n**bold** and [link](https://niao.dev)"
print(nmarkdown.to_html(md))
print(nmarkdown.strip(md))
print(nmarkdown.headings(md))
```

## Supported syntax

| Feature | Syntax |
|---------|--------|
| Headings | `#` … `######` (space after `#` required) |
| Bold | `**text**` |
| Italic | `*text*` |
| Inline code | `` `code` `` |
| Links | `[label](url)` |
| Unordered list | `- item` |
| Ordered list | `1. item` |
| Blockquote | `> text` |
| Fenced code | ` ``` ` … ` ``` ` |
| Paragraphs | Blank line separates blocks; consecutive lines join with `<br>` |

HTML output escapes `<`, `>`, `&`, quotes in text and attributes.

## Functions

| Method | Description |
|--------|-------------|
| `nmarkdown.to_html(text)` | Convert Markdown string to HTML. |
| `nmarkdown.strip(text)` | Remove markup; return plain text. |
| `nmarkdown.headings(text)` | Array of `{level, text}` objects for ATX headings (`#` … `######`). Inline markup in heading text is stripped. Fenced code blocks are skipped. |

## Errors

| Code | Meaning |
|------|---------|
| 2860 | Wrong argument count. |
| 2861 | Reserved for catchable semantic errors (unused in current subset). |
| 2862 | Type error — expected string argument. |
