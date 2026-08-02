# ncbor — CBOR encode/decode

RFC 8949 CBOR encode and decode with semantic tags, canonical (COSE-friendly) mode, indefinite-length support, and tag hooks. ~cbor2 subset.

## Import

```niao
import "ncbor"
```

Paths `import "std/ncbor"` and `import "ncbor"` are equivalent. Flat builtins (`ncbor_encode`, `ncbor_decode`, …) are also available globally after import.

## Quick start

```niao
import "ncbor"

// Encode / decode maps, arrays, bytes, tags
let data = ncbor.encode({device: "sensor-1", reading: 23.5, raw: byte_array[0xDE, 0xAD]})
let doc = ncbor.decode(data)
print(doc.device, doc.reading)

// cbor2-style aliases
let v = ncbor.loads(data)
let bytes = ncbor.dumps({ok: true})

// COSE / deterministic encoding (canonical key order + minimal integers)
let cose = ncbor.encode_canonical({alg: 1, kid: byte_array[1, 2, 3]})

// Semantic tags
let tagged = ncbor.tag(ncbor.tags.DATETIME_STRING, "2026-07-13T12:00:00Z")
let enc = ncbor.encode(tagged)

// Multi-item CBOR sequences
let seq = ncbor.decode_all(concat(ncbor.dumps(1), ncbor.dumps(2)))
print(len(seq))   // 2
```

## Functions

| Method | Description |
|--------|-------------|
| `ncbor.encode(value, opts?)` | Serialize a Niao value to `byte[]`. |
| `ncbor.decode(bytes, opts?)` | Parse CBOR bytes into a Niao value. |
| `ncbor.loads(bytes, opts?)` | Alias for `decode` (cbor2 compat). |
| `ncbor.dumps(value, opts?)` | Alias for `encode` (cbor2 compat). |
| `ncbor.valid(bytes)` | `true` when `bytes` is valid CBOR. |
| `ncbor.decode_all(bytes, opts?)` | Parse concatenated CBOR data items; returns an array. |
| `ncbor.encode_canonical(value)` | Canonical CBOR (sorted map keys, minimal encodings). |
| `ncbor.tag(n, value)` | Build `{__tag: n, value: …}` for encoding. |
| `ncbor.decode_file(path, opts?)` | Read a file and decode the first CBOR item. |
| `ncbor.encode_file(path, value, opts?)` | Encode and write to a file; returns `true`. |

### `ncbor.tags`

Object of well-known semantic tag numbers: `DATETIME_STRING` (0), `DATETIME_EPOCH` (1), `BIGNUM_POS` (2), `BIGNUM_NEG` (3), `DECIMAL_FRACTION` (4), `BIGFLOAT` (5), `UUID` (37), `SELF_DESCRIBE` (55799), and others.

### Encode options

| Key | Default | Description |
|-----|---------|-------------|
| `canonical` | `false` | RFC 8949 canonical CBOR (implies sorted keys). |
| `sort_keys` | `false` | Sort map keys by canonical byte order. |
| `auto_datetime_tag` | `false` | Tag ISO-8601 strings with tag 0 on encode. |
| `datetime_timestamp` | `false` | Prefer epoch tagging (reserved for future use). |
| `indefinite_length` | `false` | Use indefinite-length strings/arrays/maps/bytes when large. |
| `fractional_floats` | `false` | Encode floats as tag 5 bigfloat. |
| `self_describe` | `false` | Wrap output in self-describe tag 55799. |
| `max_bytes` | 64 MiB | Output size cap. |
| `max_depth` | 512 | Nesting depth cap. |

### Decode options

| Key | Default | Description |
|-----|---------|-------------|
| `tag_hook` | `true` | Decode tags 0/1/2/3/4/37 to native types; others stay tagged. |
| `allow_indefinite` | `true` | Accept indefinite-length collections. |
| `reject_trailing` | `false` | Error when extra bytes follow the first item. |
| `reject_duplicate_keys` | `false` | Error on duplicate map keys. |
| `max_bytes` | 64 MiB | Input size cap. |
| `max_depth` | 512 | Nesting depth cap. |
| `max_items` | 1_000_000 | Max array/map entries. |

### Value mapping

| CBOR | Niao |
|------|------|
| null | `nil` |
| undefined | `{__cbor_undefined: true}` |
| true / false | `bool` |
| integers | `int` (or `bigint` when out of range) |
| floats | `float` |
| text | `string` |
| bytes | `byte[]` |
| arrays | `array` |
| maps | `object` (non-string keys stringified) |
| semantic tags (hooked) | native datetime string, float epoch, bigint, UUID string, decimal→float |
| other tags | `{__tag: n, value: …}` |
| simple values (24–255) | `{__simple: n}` |

## Size limits

Inputs and outputs are capped at **64 MiB** per operation by default.

## Errors

| Code | Meaning |
|------|---------|
| 3546 | Wrong argument count. |
| 3547 | Encode/decode or I/O failure (catchable `ncbor_error`). |
| 3548 | Wrong argument type (hard error). |
| 3549 | CBOR parse/decode failure (catchable `ncbor_error`). |

## See also

- `json` — JSON text parse/stringify.
- `codec` — Base64, hex helpers.
- `nbinary` — struct packing and low-level binary ops.
- `crypto` — COSE/JWS workflows (pair with `encode_canonical`).
