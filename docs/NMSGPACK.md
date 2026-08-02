# nmsgpack — MessagePack encode/decode

MessagePack binary serialization: pack/unpack, streaming packer/unpacker, extension types, timestamps, and file I/O. ~`msgpack` subset.

## Import

```niao
import "nmsgpack"
```

Paths `import "std/nmsgpack"` and `import "nmsgpack"` are equivalent. Flat builtins (`nmsgpack_pack`, `nmsgpack_unpack`, …) are also available globally after import.

## Quick start

```niao
import "nmsgpack"

// Pack / unpack
let data = {name: "neko", tags: ["fast", "binary"], n: 42}
let raw = nmsgpack.pack(data)
let back = nmsgpack.unpack(raw)
print(back.name)   // neko

// Streaming (multiple values in one byte stream)
let p = nmsgpack.packer()
nmsgpack.packer_pack(p.handle, 1)
nmsgpack.packer_pack(p.handle, {ok: true})
let blob = nmsgpack.packer_finish(p.handle)

let u = nmsgpack.unpacker(blob)
print(nmsgpack.unpacker_next(u.handle))   // 1
print(nmsgpack.unpacker_next(u.handle).ok) // true

// Extension types and timestamps
let ext = nmsgpack.ext(42, [0xCA, 0xFE])
let ts = nmsgpack.timestamp(1_600_000_000, 500)
let raw_ts = nmsgpack.pack(ts, {timestamp: true})

// Python msgpack aliases
let b = nmsgpack.packb(data)
let v = nmsgpack.unpackb(b)
```

## Functions

| Method | Description |
|--------|-------------|
| `nmsgpack.pack(value, opts?)` | Encode a Niao value to MessagePack bytes. |
| `nmsgpack.unpack(bytes, opts?)` | Decode one MessagePack value from bytes. |
| `nmsgpack.pack_all(values, opts?)` | Encode an array of values sequentially into one buffer. |
| `nmsgpack.unpack_all(bytes, opts?)` | Decode every top-level value in a buffer; returns an array. |
| `nmsgpack.valid(bytes)` | `true` when bytes contain at least one valid MessagePack value. |
| `nmsgpack.pack_file(path, value, opts?)` | Write packed bytes to a file; returns `true`. |
| `nmsgpack.unpack_file(path, opts?)` | Read and decode the first value from a file. |
| `nmsgpack.ext(code, data)` | Build an extension object `{code, data}` for packing. |
| `nmsgpack.timestamp(sec, nsec?)` | Build a timestamp object `{sec, nsec}` (packed as ext -1 when `timestamp: true`). |
| `nmsgpack.packer(opts?)` | Create a streaming packer; returns `{handle}`. |
| `nmsgpack.packer_pack(handle, value)` | Append one encoded value to a packer buffer. |
| `nmsgpack.packer_finish(handle)` | Finalize and return all packed bytes (destroys handle). |
| `nmsgpack.packer_bytes(handle)` | Peek at packed bytes without finishing. |
| `nmsgpack.packer_reset(handle)` | Clear the packer buffer. |
| `nmsgpack.unpacker(opts?, bytes?)` | Create a streaming unpacker; optional initial buffer. |
| `nmsgpack.unpacker_feed(handle, chunk)` | Append bytes to the unpacker buffer. |
| `nmsgpack.unpacker_next(handle)` | Decode the next complete value, or `nil` if more input is needed. |
| `nmsgpack.unpacker_tell(handle)` | Current consume offset in the buffered stream. |
| `nmsgpack.unpacker_reset(handle)` | Clear unpacker state. |
| `nmsgpack.packb` / `unpackb` | Aliases for `pack` / `unpack` (Python msgpack compat). |
| `nmsgpack.dumps` / `loads` | Aliases for `pack` / `unpack`. |

### Pack options

| Key | Default | Description |
|-----|---------|-------------|
| `use_bin_type` | `false` | Encode strings as MessagePack bin. Set `true` for Python 3 msgpack wire compat. |
| `use_single_float` | `false` | Prefer 32-bit floats when values fit exactly. |
| `timestamp` | `true` | Encode `{sec, nsec}` objects as timestamp extension (-1). |
| `bigint_as_string` | `true` | Serialize integers larger than 64 bits as decimal strings. |

### Unpack options

| Key | Default | Description |
|-----|---------|-------------|
| `strict_map_key` | `true` | Require string keys in maps (reject integer keys). |
| `raw` | `false` | Decode str format as raw bytes instead of UTF-8 strings. |
| `timestamp` | `true` | Decode timestamp ext (-1) to `{sec, nsec}` objects. |
| `bigint_as_string` | `true` | Parse decimal strings back to big integers when possible. |
| `max_depth` | `512` | Maximum nesting depth while decoding. |

### Value mapping

| MessagePack | Niao |
|-------------|------|
| `nil` | `nil` |
| `bool` | `bool` |
| `int` / `uint` | `int` (uint ≤ i64::MAX) or `bigint` |
| `float` | `float` or `int` when whole |
| `str` / `bin` | `string` or `byte[]` (`raw` / `use_bin_type` affect mapping) |
| `array` | `array` |
| `map` | `object` (string keys) |
| `ext` | `{code, data}` |
| timestamp ext (-1) | `{sec, nsec}` when `timestamp: true` |

### Input types

Byte-taking functions accept `byte[]` or `string` (interpreted as UTF-8 source bytes).

### Size limits

Inputs and outputs are capped at **64 MiB** per operation or stream buffer.

## Errors

| Code | Meaning |
|------|---------|
| 4320 | Wrong argument count. |
| 4321 | Encode/decode failure (catchable `nmsgpack_error`). |
| 4322 | Wrong argument type (hard error). |
| 4323 | Invalid streaming handle. |

## See also

- `json` — text JSON parse/stringify.
- `nbinary` — struct pack/unpack, varints, CRC (not self-describing).
- `codec` — Base64, hex helpers.
- `nyaml` — YAML text formats.
