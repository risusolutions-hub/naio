# `nredis` — Redis Client

> Import: `import "nredis"` or `import "std/nredis"`

`nredis` is a zero-dependency, RESP2-based Redis client built on top of
`niao_db::redis::Client`. Connections are held in a thread-local handle
registry (same design as `npg`/`ncache`). All network and command errors
are returned as recoverable `Error` values (code **E2781**); arity and type
mistakes raise hard runtime errors (E2780 / E2782 / E2783).

---

## Connection

### `nredis.connect(url) -> handle_id | error`

Opens a TCP connection to Redis and returns an integer handle.

```niao
import "nredis"

let r = nredis.connect("redis://127.0.0.1:6379")
if r is error { print("connect failed:", r.message) }
```

**URL format** (all parts optional):

```
redis://:password@host:port/db
```

- Default host: `127.0.0.1`
- Default port: `6379`
- Default db: `0`

### `nredis.close(id) -> true | error`

Drops the connection. Using the handle after `close` returns a recoverable
`Error(E2783)`.

```niao
nredis.close(r)
```

---

## String commands

### `nredis.ping(id) -> "PONG"`

```niao
let pong = nredis.ping(r)   // "PONG"
```

### `nredis.get(id, key) -> string | nil`

Returns `nil` when the key does not exist.

```niao
let val = nredis.get(r, "greeting")
if val == nil { print("not found") }
```

### `nredis.set(id, key, value) -> true`

```niao
nredis.set(r, "greeting", "hello")
```

### `nredis.del(id, key) -> true`

```niao
nredis.del(r, "greeting")
```

### `nredis.incr(id, key, by?) -> int`

Atomically increments `key` by `by` (default **1**). Creates the key with
value `0` if it does not exist, then increments.

```niao
nredis.set(r, "visits", "0")
nredis.incr(r, "visits")        // 1
nredis.incr(r, "visits", 10)    // 11
```

### `nredis.expire(id, key, secs) -> bool`

Sets a TTL of `secs` seconds on `key`. Returns `true` if the key exists,
`false` otherwise.

```niao
nredis.expire(r, "session", 3600)
```

---

## Multi-key commands

### `nredis.mget(id, keys[]) -> array`

Fetches multiple keys in one round-trip. Missing keys appear as `nil` at
the corresponding position.

```niao
let vals = nredis.mget(r, ["a", "b", "c"])
// vals[0] == "hello", vals[1] == nil, ...
```

### `nredis.mset(id, pairs{}) -> true`

Sets multiple key-value pairs atomically using `MSET`.

```niao
nredis.mset(r, {
    "name":  "Alice",
    "score": "100",
})
```

Numeric values are automatically stringified. All values must be strings or
numbers; other types raise a type error (E2782).

---

## Hash commands

### `nredis.hget(id, key, field) -> string | nil`

```niao
let name = nredis.hget(r, "user:1", "name")
```

### `nredis.hset(id, key, field, value) -> true`

Uses Redis `HSET` (Redis ≥ 4 syntax with explicit field-value pair).

```niao
nredis.hset(r, "user:1", "name", "Alice")
nredis.hset(r, "user:1", "age",  "30")
```

### `nredis.hdel(id, key, field) -> true`

```niao
nredis.hdel(r, "user:1", "age")
```

### `nredis.hgetall(id, key) -> object`

Returns all field-value pairs of the hash as a Niao object.

```niao
let user = nredis.hgetall(r, "user:1")
print(user.name)   // "Alice"
```

---

## Raw command

### `nredis.cmd(id, parts[]) -> value`

Sends an arbitrary RESP command. `parts` is an array of strings where the
first element is the Redis command name and the rest are arguments.

The return value is the raw RESP reply converted to a Niao value:

| RESP type | Niao type |
|-----------|-----------|
| Simple string `+OK` | `"OK"` |
| Bulk string `$…` | `string` |
| Null bulk / Null | `nil` |
| Integer `:42` | `42` |
| Array `*n` | `array` |
| Error `-ERR …` | `"ERR …"` (string) |

```niao
let ttl = nredis.cmd(r, ["TTL", "session"])
let info = nredis.cmd(r, ["INFO", "server"])
nredis.cmd(r, ["LPUSH", "mylist", "a", "b", "c"])
```

---

## Error codes

| Code | Constant | Meaning |
|------|----------|---------|
| 2780 | `E2780_NREDIS_ARITY` | Wrong number of arguments |
| 2781 | `E2781_NREDIS_ERROR` | Redis or network error |
| 2782 | `E2782_NREDIS_TYPE` | Wrong argument type |
| 2783 | `E2783_NREDIS_INVALID_HANDLE` | Invalid or closed connection handle |

---

## Complete example

```niao
import "nredis"

let r = nredis.connect("redis://127.0.0.1:6379")
if r is error {
    print("cannot connect:", r.message)
    return
}

nredis.set(r, "x", "10")
print(nredis.get(r, "x"))          // "10"
print(nredis.incr(r, "x", 5))      // 15
nredis.expire(r, "x", 60)

nredis.mset(r, { "a": "1", "b": "2", "c": "3" })
let vals = nredis.mget(r, ["a", "b", "c", "d"])
print(vals)                         // ["1", "2", "3", nil]

nredis.hset(r, "user:42", "name", "Bob")
nredis.hset(r, "user:42", "role", "admin")
let u = nredis.hgetall(r, "user:42")
print(u.name, u.role)

print(nredis.cmd(r, ["DBSIZE"]))   // integer

nredis.close(r)
```
