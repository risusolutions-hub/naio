# nunicode — Unicode correctness

NFC/NFD/NFKC/NFKD normalization, extended grapheme clusters, UCD character properties, East-Asian-aware display width, and casefold. Complements `nstr` string editing with standards-grade Unicode semantics (~Python `unicodedata` + `grapheme` subset).

## Import

```niao
import "nunicode"
```

Paths `import "std/nunicode"` and `import "nunicode"` are equivalent. Flat builtins (`nunicode_nfc`, `nunicode_graphemes`, …) are also available globally after import.

## Quick start

```niao
import "nunicode"

// Normalization
print(nunicode.nfc("e\u{0301}"))           // é
print(nunicode.is_normalized("é"))         // true

// Grapheme clusters (user-perceived characters)
print(nunicode.grapheme_len("🇺🇸"))        // 1
print(nunicode.graphemes("café"))          // ["c","a","f","é"] or NFC-dependent splits

// Display width for terminals / tables
print(nunicode.display_width("你好"))       // 4
print(nunicode.truncate_width("你好世界", 6, ".."))  // 你好..

// unicodedata-style properties (single scalar strings)
print(nunicode.category("A"))              // Lu
print(nunicode.name("Σ"))                  // GREEK CAPITAL LETTER SIGMA
print(nunicode.script("你"))                // Hani
print(nunicode.casefold("Straße"))         // strasse
```

## Normalization

| Method | Description |
|--------|-------------|
| `nunicode.normalize(s, form?)` | Normalize to `NFC` (default), `NFD`, `NFKC`, or `NFKD`. |
| `nunicode.is_normalized(s, form?)` | `true` when already in the given form. |
| `nunicode.nfc(s)` / `nfd` / `nfkc` / `nfkd` | Convenience wrappers. |

Invalid form strings return a catchable `nunicode_error`.

## Graphemes & scalars

| Method | Description |
|--------|-------------|
| `nunicode.graphemes(s)` | Array of extended grapheme cluster strings. |
| `nunicode.grapheme_len(s)` | Count grapheme clusters. |
| `nunicode.grapheme_at(s, i)` | Cluster at index `i`, or `nil`. |
| `nunicode.grapheme_slice(s, start, end?)` | Half-open grapheme slice. |
| `nunicode.grapheme_offsets(s)` | Byte offset of each cluster start. |
| `nunicode.chars(s)` | Unicode scalar values as single-char strings. |
| `nunicode.char_len(s)` | Scalar count (not byte length). |

Grapheme results are capped at **16,777,216** clusters.

## Display width

| Method | Description |
|--------|-------------|
| `nunicode.display_width(s)` | East-Asian-aware terminal column count. |
| `nunicode.truncate_width(s, max, suffix?)` | Truncate by display width (default suffix `"..."`). |
| `nunicode.casefold(s)` | Full casefold mapping for case-insensitive compare. |

## Character properties (single scalar)

Pass a string containing exactly one Unicode scalar (not a multi-scalar grapheme like emoji ZWJ sequences).

| Method | Description |
|--------|-------------|
| `nunicode.category(c)` | General category (`Lu`, `Nd`, …). |
| `nunicode.categories(s)` | Per-scalar categories for a string. |
| `nunicode.name(c)` | Unicode character name, or `nil`. |
| `nunicode.lookup(name)` | Reverse name lookup → single-char string. |
| `nunicode.script(c)` | ISO 15924 script (`Latn`, `Hani`, …). |
| `nunicode.bidi(c)` | Bidirectional class (`L`, `R`, `NSM`, …). |
| `nunicode.combining(c)` | Canonical combining class (integer). |
| `nunicode.east_asian_width(c)` | EAW property (`N`, `W`, `Na`, …). |
| `nunicode.decimal(c)` | Decimal digit value, or `-1`. |
| `nunicode.digit(c, base?)` | Digit in `base` (default 10), or `-1`. |
| `nunicode.numeric(c)` | Numeric value (float) for fractions/Roman/etc., or `nil`. |
| `nunicode.mirrored(c)` | Bidi-mirrored property. |
| `nunicode.decomposition(c)` | Canonical decomposition hex mapping (`"0041 030A"`), or `""`. |
| `nunicode.is_alphabetic(c)` / `is_numeric` / `is_whitespace` / `is_control` | Fast property predicates. |

## Parallel batch helpers

| Method | Description |
|--------|-------------|
| `nunicode.parallel_normalize(arr, form?)` | Data-parallel NFC/NFD/NFKC/NFKD over a string array. |
| `nunicode.parallel_display_width(arr)` | Parallel width measurement. |
| `nunicode.parallel_casefold(arr)` | Parallel casefold. |

Uses all logical CPUs via `niao_parallel` (deterministic order preserved).

## Errors

| Code | Meaning |
|------|---------|
| 3490 | Wrong argument count. |
| 3491 | Invalid parameter or output too large (catchable `nunicode_error`). |
| 3492 | Wrong argument type (hard error). |
| 3493 | Reserved for invalid scalar / form (via 3491 today). |

## See also

- `nstr` — string editing, search, slugify, edit distance.
- `nvalid` — schema validation with pattern/email/url checks.
- `nencoding` — legacy byte encodings (UTF-8 lives in core strings).
