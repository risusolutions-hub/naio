# nws — WebSocket client

Thin ergonomic wrapper over `net` WebSocket builtins. Uses the **same handle IDs** as `net.ws_connect` — connections opened with `nws` can be used with `net` and vice versa.

## Import

```niao
import "nws"
```

Paths `import "std/nws"` and `import "nws"` are equivalent. Flat builtins (`nws_connect`, `nws_send`, …) are also available globally after import.

## Quick start

```niao
import "nws"

fn main() {
    let id = nws.connect("ws://echo.websocket.events")
    nws.send(id, "hello")
    let msg = nws.recv(id)   // string or byte array
    print(msg)
    nws.close(id)            // true
}
```

## Functions

| Method | Description |
|--------|-------------|
| `nws.connect(url, opts?)` | Open a WebSocket client connection. Returns a positive integer handle. Optional `opts` object is reserved for future use (currently ignored). On failure returns catchable `nws_error`. |
| `nws.send(id, message)` | Send a text or binary frame. `message` may be a string or byte array (`int[]` 0..255). Returns `true` on success. |
| `nws.recv(id)` | Read the next frame. Returns a string (text), int array (binary), or `nil` on close. Blocks until a frame arrives. |
| `nws.close(id)` | Close the connection and release the handle. Returns `true` on success. |

## Message types

- **Text frames** — returned as strings from `recv`.
- **Binary frames** — returned as int arrays (each element 0..255).
- **Close frames** — `recv` returns `nil`.

`send` accepts strings (sent as text) or byte arrays (sent as text via UTF-8 lossy conversion, matching `net.ws_send` behavior).

## Handles

Handles are allocated from the shared `net` handle table. A handle opened with `nws.connect` is valid for `net.ws_send` / `net.ws_recv` / `net.ws_close`, and the reverse.

Always call `nws.close` (or `net.ws_close`) when finished to release the handle.

## Errors

| Code | Meaning |
|------|---------|
| 2870 | Wrong argument count. |
| 2871 | Connection or I/O error (catchable `nws_error` from `connect`; hard error from `send`/`recv`/`close`). |
| 2872 | Wrong argument type. |
| 2873 | Invalid or closed handle. |

## See also

- `net` — full networking library (`net.ws_connect`, HTTP, TCP, TLS, …).
- `nurl` — URL parse/build helpers for constructing WebSocket URLs.
