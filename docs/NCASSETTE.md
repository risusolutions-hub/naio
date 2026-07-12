# ncassette standard library

VCR-style request/response cassette for **record**, **replay**, and **passthrough**. In-memory map keyed by stable request keys; durable save/load of **string responses only** as a minimal JSON object.

## Import

```niao
import "ncassette"
```

Paths `import "std/ncassette"` and `import "ncassette"` are equivalent. Flat builtins (`ncassette_get`, `ncassette_put`, …) are also available globally after import.

## Quick start

```niao
import "ncassette"

let tape = ncassette.new("record")
let k = ncassette.key("GET", "/api/users", "")
ncassette.put(tape, k, "{\"users\":[]}")
ncassette.save(tape, "fixtures/users.json")

let replay = ncassette.load("fixtures/users.json")   // mode = "replay"
print(ncassette.get(replay, k))
print(ncassette.mode(replay))                        // replay
```

## Creating cassettes

| Method | Description |
|--------|-------------|
| `ncassette.new(mode)` | Create a cassette. `mode` is `"record"`, `"replay"`, or `"passthrough"`. Returns an int handle. |
| `ncassette.load(path)` | Load a cassette file into a new handle in **replay** mode. |
| `ncassette.close(handle)` | Free the cassette; returns `true` if it existed. |

Mode is metadata for callers (e.g. HTTP wrappers); `put`/`get` work the same in every mode.

## Keys

| Method | Description |
|--------|-------------|
| `ncassette.key(method, url, body?)` | Stable key: uppercase `method`, then `\|url\|body`. Missing `body` is `""`. |

Example: `ncassette.key("get", "/x", "{}")` → `"GET|/x|{}"`.

## Operations

| Method | Description |
|--------|-------------|
| `ncassette.put(h, key, response)` | Store any Niao value under `key`. |
| `ncassette.get(h, key)` | Value or `nil`. |
| `ncassette.has(h, key)` | `true` if the key exists. |
| `ncassette.save(h, path)` | Persist **string** responses only as `{"key":"resp",...}` (hand-rolled JSON escapes). Returns `true` on success. Non-string values → catchable error. |
| `ncassette.len(h)` | Entry count. |
| `ncassette.keys(h)` | Sorted array of keys. |
| `ncassette.mode(h)` | `"record"`, `"replay"`, or `"passthrough"`. |
| `ncassette.clear(h)` | Drop all entries. |

## File format

`save` / `load` use a single JSON object with string keys and string values:

```json
{"GET|/api|":"{\"ok\":true}","POST|/t|x":"body"}
```

Keys are written in sorted order. Escapes: `\"`, `\\`, `\n`, `\r`, `\t`, and `\uXXXX` for other controls. Only string responses are supported on disk — use string bodies in demos and fixtures.

## Errors

| Code | Meaning |
|------|---------|
| 2960 | Wrong argument count. |
| 2961 | Operation failed (bad mode, I/O, non-string save, parse). Catchable `error`. |
| 2962 | Wrong argument type. |
| 2963 | Invalid or closed cassette handle (catchable `error`). |
