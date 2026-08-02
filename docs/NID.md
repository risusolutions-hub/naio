# nid — ID generation (ULID, UUID, nanoid, snowflake, hashids)

High-performance native ID utilities (~`uuid6`, `ulid-py`, `nanoid`, Twitter snowflake, `hashids` subsets). Extends `codec` UUID with v6, timestamps, ULID, and obfuscated integer hashes.

## Import

```niao
import "nid"
```

Paths `import "std/nid"` and `import "nid"` are equivalent. Flat builtins (`nid_uuid4`, `nid_ulid`, …) are also available globally after import.

## Quick start

```niao
import "nid"

let user_id = nid.ulid()
let order_id = nid.uuid7()
let slug = nid.nanoid()
let tweet_id = nid.snowflake(3, 1)

let h = nid.hashids("my-salt", 8)
let public_id = h.encode(42, 99)
let nums = h.decode(public_id)
```

## UUID (extends `codec`)

| Method | Description |
|--------|-------------|
| `nid.uuid4()` | Random UUID v4 (delegates to `codec` implementation). |
| `nid.uuid7()` | Timestamp-ordered UUID v7 (RFC 9562). |
| `nid.uuid6(ts_ms?)` | UUID v6; optional Unix-ms timestamp (default: now). |
| `nid.uuid_parse(s)` | `{ok, value, version}` or `{ok: false, error}`. |
| `nid.uuid_is_valid(s)` | Fast boolean check. |
| `nid.uuid_version(s)` | Version nibble (4/6/7/…) or catchable `nid_error`. |
| `nid.uuid_bytes(s)` | 16-byte array (0..255 ints). |
| `nid.uuid_from_bytes(bytes)` | Build canonical UUID string from byte array. |
| `nid.uuid_timestamp(s)` | Unix ms for v6/v7; `nil` for non-time UUIDs. |

Use `codec.uuid4()` / `codec.uuid7()` for basic needs; `nid` adds v6, parsing helpers, and byte/timestamp introspection.

## ULID

| Method | Description |
|--------|-------------|
| `nid.ulid()` | New 26-char Crockford-base32 ULID (48-bit ms + 80 random bits). |
| `nid.ulid_parse(s)` | `{ok, value, timestamp}` or error object. |
| `nid.ulid_is_valid(s)` | Validate charset and length. |
| `nid.ulid_timestamp(s)` | Extract Unix milliseconds. |
| `nid.ulid_maker()` | Monotonic generator handle `{id, kind, next()}`. |

`ulid_maker.next()` guarantees strictly increasing IDs within the process (useful for DB primary keys).

## Nanoid

| Method | Description |
|--------|-------------|
| `nid.nanoid(size?, alphabet?)` | URL-safe ID (default size 21). |
| `nid.nanoid_bulk(count, size?, alphabet?)` | Batch-generate an array of IDs. |
| `nid.NANOID_ALPHABET` | Default 64-char alphabet. |
| `nid.NANOID_SIZE` | Default length (`21`). |

## Snowflake

| Method | Description |
|--------|-------------|
| `nid.snowflake(worker_id?, datacenter_id?)` | One 64-bit ID (defaults `0`, `0`). |
| `nid.snowflake_maker(worker?, dc?, epoch_ms?)` | Reusable handle with `.next()`. |
| `nid.snowflake_parse(id, epoch_ms?)` | `{timestamp, datacenter_id, worker_id, sequence}`. |
| `nid.SNOWFLAKE_EPOCH` | Default Twitter epoch (`1288834974657`). |
| `nid.MAX_WORKER_ID` / `MAX_DATACENTER_ID` | `31` each. |

Generators are thread-safe; sequence spins within the same millisecond.

## Hashids

| Method | Description |
|--------|-------------|
| `nid.hashids(salt?, min_length?, alphabet?)` | Encoder handle with `.encode`, `.decode`, `.encode_hex`, `.decode_hex`. |
| `nid.HASHIDS_ALPHABET` | Default 62-char alphabet. |

```niao
let enc = nid.hashids("pepper", 6)
let hash = enc.encode(1001, 42)
enc.decode(hash)          // int array
enc.encode_hex("deadbeef")
```

## Errors

| Code | Meaning |
|------|---------|
| 3534 | Wrong argument count. |
| 3535 | Parse / range / encode error (catchable `nid_error`). |
| 3536 | Wrong argument type (hard error). |
| 3537 | Invalid or closed handle. |

## See also

- `codec` — base64, hex, UUID v4/v7, dotenv.
- `nrand` — PRNG and statistical sampling (not ID-shaped).
- `nvalid` — `is_uuid` validation helper.
