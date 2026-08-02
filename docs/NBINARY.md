# nbinary — binary struct, bitstrings, varints, CRC

High-performance binary primitives: Python `struct`-compatible pack/unpack, mutable bit buffers, protobuf varints, and CRC32/CRC64 checksums.

## Import

```niao
import "nbinary"
```

Paths `import "std/nbinary"` and `import "nbinary"` are equivalent. Flat builtins (`nbinary_pack`, `nbinary_crc32`, …) are also available globally after import.

## Quick start

```niao
import "nbinary"

// struct pack/unpack (big-endian uint32)
let buf = nbinary.pack(">I", 16909060)
print(nbinary.unpack(">I", buf))           // [16909060]

// compiled struct object
let hdr = nbinary.struct_format(">HH")
print(hdr.size)                            // 4
print(hdr.pack(1, 2))

// protobuf varints
let enc = nbinary.uvarint_encode(300)
print(nbinary.uvarint_decode(enc))         // {value: 300, offset: 2}

// CRC32 (IEEE / gzip polynomial)
print(nbinary.crc32("123456789"))

// bit buffer (handle API)
let bs = nbinary.bits(8)
nbinary.write_bits(bs, 4, 10)
nbinary.seek_bits(bs, 0)
print(nbinary.read_bits(bs, 4))           // 10
nbinary.release_bits(bs)
```

## Struct format (`struct` subset)

Format strings follow [Python `struct`](https://docs.python.org/3/library/struct.html) conventions:

| Prefix | Meaning |
|--------|---------|
| `@` | Native endian with alignment |
| `=` | Native endian, standard sizes, no padding |
| `<` | Little-endian, no padding |
| `>` / `!` | Big-endian (network), with alignment |

Common type codes: `b/B` (i8/u8), `h/H` (i16/u16), `i/I`/`l/L` (i32/u32), `q/Q` (i64/u64), `f`/`d` (f32/f64), `e` (f16), `?` (bool), `s`/`p` (fixed / Pascal strings), `x` (pad), `P` (pointer u64).

| Method | Description |
|--------|-------------|
| `nbinary.pack(fmt, ...values)` | Pack values to `byte_array`. |
| `nbinary.unpack(fmt, data, offset?)` | Unpack to array (default offset `0`). |
| `nbinary.calcsize(fmt)` | Record size in bytes. |
| `nbinary.pack_into(fmt, buf, offset, ...values)` | Pack into existing buffer; returns end offset. |
| `nbinary.unpack_from(fmt, data, offset)` | `{values, offset}` after one record. |
| `nbinary.iter_unpack(fmt, data)` | Array of `{values, offset}` per record. |
| `nbinary.struct_format(fmt)` | Compiled struct `{format, size, endian, pack, unpack}`. |
| `nbinary.endian()` | Endian marker strings (`little`, `big`, `native`, …). |

## Varints (protobuf subset)

| Method | Description |
|--------|-------------|
| `nbinary.uvarint_encode(n)` | Unsigned varint bytes. |
| `nbinary.uvarint_decode(data, offset?)` | `{value, offset}`. |
| `nbinary.varint_encode(n)` | Signed varint (zigzag + uvarint). |
| `nbinary.varint_decode(data, offset?)` | `{value, offset}`. |
| `nbinary.zigzag_encode(n)` / `zigzag_decode(n)` | Zigzag helpers. |

## CRC

| Method | Description |
|--------|-------------|
| `nbinary.crc32(data)` | IEEE CRC-32 (`crc32fast`, SIMD-accelerated). |
| `nbinary.crc32_update(crc, data)` | Incremental CRC-32. |
| `nbinary.crc64(data)` | ECMA-182 CRC-64 (slice-by-8). |
| `nbinary.crc64_update(crc, data)` | Incremental CRC-64. |

## Bit buffers (`bitstring` subset)

Bit handles are opaque ints; call `release_bits` when done.

| Method | Description |
|--------|-------------|
| `nbinary.bits(bit_len, initial_bytes?)` | New mutable bit buffer handle. |
| `nbinary.from_bytes(data, bit_len?)` | Handle from bytes. |
| `nbinary.bit_len(h)` | Logical bit length. |
| `nbinary.get_bit(h, pos)` / `set_bit(h, pos, val)` | Single-bit access. |
| `nbinary.read_bits(h, n)` / `write_bits(h, n, val)` | Sequential MSB-first fields at cursor. |
| `nbinary.seek_bits(h, pos)` | Reset read/write cursor. |
| `nbinary.to_bytes(h, pad?)` | Materialize bytes (default pad to byte boundary). |
| `nbinary.bits_hex(h)` | Lowercase hex of padded bytes. |
| `nbinary.release_bits(h)` | Drop handle. |

## Errors

| Code | Meaning |
|------|---------|
| 3460 | Wrong argument count. |
| 3461 | Format/bounds semantic error (catchable `nbinary_error`). |
| 3462 | Wrong argument type (hard error). |
| 3463 | Invalid or released bits handle. |

## See also

- `codec` — hex/base64 text encoding.
- `nsnap` — structured value snapshots (different wire format).
- `ncolumnar` — columnar binary tables.
