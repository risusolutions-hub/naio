# nencoding — charset detection & transcoding

Charset detection and transcoding for byte sequences: UTF-8/16, Shift-JIS, GBK, Latin-1, BOM handling, and Unicode normalization. ~`codecs` + `charset-normalizer` subset.

## Import

```niao
import "nencoding"
```

Paths `import "std/nencoding"` and `import "nencoding"` are equivalent. Flat builtins (`nencoding_detect`, `nencoding_decode`, …) are also available globally after import.

## Quick start

```niao
import "nencoding"

// Detect charset from raw bytes
let raw = nencoding.encode("日本語", "shift_jis")
let guess = nencoding.detect(raw)
print(guess.encoding, guess.confidence)   // shift_jis, ~0.95

// Decode / encode
let text = nencoding.decode(raw, "shift_jis")
let utf8 = nencoding.encode(text, "utf-8", true)   // with BOM

// Auto-decode (detect + decode)
let text2 = nencoding.guess_decode(raw)

// Transcode between encodings
let gbk_bytes = nencoding.transcode(raw, "gbk", "shift_jis")
```

## Functions

| Method | Description |
|--------|-------------|
| `nencoding.detect(bytes)` | Best charset guess → `{encoding, confidence, bom_encoding?, language?}`. |
| `nencoding.detect_all(bytes, top?)` | Up to `top` candidates (default 5), sorted by confidence. |
| `nencoding.decode(bytes, encoding?, errors?)` | Decode to UTF-8 string. Auto-detect when `encoding` omitted. `errors`: `strict` (default), `replace`, `ignore`. |
| `nencoding.encode(text, encoding?, bom?)` | Encode string to bytes (`encoding` default `utf-8`; `bom` default `false`). |
| `nencoding.guess_decode(bytes, errors?)` | Shorthand: detect + decode in one call. |
| `nencoding.transcode(bytes, to, from?, errors?)` | Re-encode bytes to target charset (auto-detect source when `from` omitted). |
| `nencoding.bom(encoding)` | BOM prefix bytes for an encoding (empty array when none). |
| `nencoding.strip_bom(bytes)` | `{bytes, encoding?}` with leading BOM removed. |
| `nencoding.list()` | Array of `{name, aliases, has_bom}` for supported encodings. |
| `nencoding.lookup(label)` | Resolve alias → encoding info object. |
| `nencoding.is_valid(bytes, encoding)` | `true` when bytes are valid in the given encoding (strict). |
| `nencoding.normalize(text, form?)` | Unicode normalization (`NFC` default; also `NFD`, `NFKC`, `NFKD`). |
| `nencoding.same_encoding(a, b)` | `true` when two labels resolve to the same charset. |

### Input types

Byte-taking functions accept `byte[]` or `string` (interpreted as UTF-8 source bytes for convenience in tests).

### Supported encodings

`utf-8`, `utf-8-sig`, `utf-16-le`, `utf-16-be`, `shift_jis` (aliases: `shift-jis`, `sjis`, `cp932`), `euc-jp`, `iso-2022-jp`, `gbk`, `gb18030`, `big5`, `euc-kr`, `iso-8859-1` / `latin-1`, `windows-1252`, `ascii`, `koi8-r`, `iso-8859-5`, `windows-1251`.

### Size limits

Inputs and outputs are capped at **64 MiB** per operation.

## Errors

| Code | Meaning |
|------|---------|
| 3470 | Wrong argument count. |
| 3471 | Encoding/transcoding failure (catchable `nencoding_error`). |
| 3472 | Wrong argument type (hard error). |

## See also

- `codec` — Base64, hex, UUID helpers (not charset transcoding).
- `io` — file read/write; pair with `nencoding.decode` for legacy text files.
- `nstr` — UTF-8 string utilities after decoding.
