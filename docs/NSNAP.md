# nsnap standard library

Fast binary value snapshots with SHA-256 content fingerprints and staleness checks. Wire format magic `NSNP1`.

Supports scalars, arrays, objects, and packed typed arrays (`int[]`, `float[]`, `bool[]`, `byte[]`, `string[]`). Functions, native handles, and errors cannot be snapshotted.

## Import

```niao
import "nsnap"
```

Paths `import "std/nsnap"` and `import "nsnap"` are equivalent.

## Quick start

```niao
import "nsnap"

let state = {count: 42, tags: ["a", "b"]}
let snap = nsnap.capture(state)                 // byte[]

let restored = nsnap.restore(snap)              // deep copy of state
print(nsnap.stale(snap, state))                 // false — unchanged
state.count = 99
print(nsnap.stale(snap, state))                 // true — fingerprint differs

let fp = nsnap.fingerprint(state)
print(nsnap.stale_hash(snap, fp))               // true after mutation
print(nsnap.info(snap))                         // {magic, version, created_ms, fingerprint, payload_len}
```

## Capture & restore

| Method | Description |
|--------|-------------|
| `nsnap.capture(value)` | Pack a value into a `byte[]` snapshot (`NSNP1`). |
| `nsnap.restore(bytes)` | Decode snapshot back to a Niao value. |
| `nsnap.validate(bytes)` | `true` when bytes are a valid `NSNP1` snapshot. |
| `nsnap.info(bytes)` | Metadata object without full decode of payload. |

## Fingerprints & staleness

| Method | Description |
|--------|-------------|
| `nsnap.fingerprint(value)` | SHA-256 hex of the encoded payload (no header). |
| `nsnap.fingerprint_bytes(bytes)` | Fingerprint stored in a snapshot header. |
| `nsnap.stale(bytes, value)` | `true` when `value` fingerprint differs from snapshot. |
| `nsnap.stale_hash(bytes, hex)` | `true` when stored fingerprint ≠ expected hex. |
| `nsnap.stale_since(bytes, since_ms)` | `true` when snapshot `created_ms` < `since_ms`. |

## Wire format

```
NSNP1 | version:1 | created_ms:i64 | fingerprint:[32] | payload_len:u32 | payload
```

Payload uses tagged binary encoding for supported value types. Header fingerprint must match payload SHA-256.

## Errors

| Code | Meaning |
|------|---------|
| 3420 | Wrong argument count. |
| 3421 | Value cannot be snapshotted (catchable). |
| 3422 | Type mismatch (hard error). |
| 3423 | Invalid or corrupt snapshot format (catchable). |
