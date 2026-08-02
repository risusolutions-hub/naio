# nrpc — JSON-RPC 2.0 client/server over stdio, TCP, HTTP

JSON-RPC 2.0 message codec, method dispatch, NDJSON / Content-Length framing, and sync transports. ~`jsonrpcserver` / `jsonrpcclient` subset.

## Import

```niao
import "nrpc"
```

Paths `import "std/nrpc"` and `import "nrpc"` are equivalent. Flat builtins (`nrpc_dispatch`, `nrpc_encode`, …) are also available globally after import.

## Quick start

```niao
import "nrpc"

fn add_handler(params) {
    return params[0] + params[1]
}

fn main() {
    let srv = nrpc.new_server()
    nrpc.method(srv, "add", "add_handler")

    let out = nrpc.dispatch(srv, "{\"jsonrpc\":\"2.0\",\"method\":\"add\",\"params\":[2,3],\"id\":1}")
    print(out.result)   // 5

    let client = nrpc.new_client()
    let req = nrpc.call(client, "add", [10, 20])
    let text = nrpc.encode(req)
    let again = nrpc.dispatch(srv, text)
    print(nrpc.parse_result(again).result)  // 30

    nrpc.close(srv)
    nrpc.close_client(client)
}
```

## Functions

### Message builders

| Method | Description |
|--------|-------------|
| `nrpc.request(method, params?, id?)` | Build a JSON-RPC request object (`id` defaults to `1`). |
| `nrpc.notify(method, params?)` | Build a notification (no `id`). |
| `nrpc.success(id, result)` | Build a success response. |
| `nrpc.failure(id, code, message, data?)` | Build an error response. |
| `nrpc.err(code, message, data?)` | Handler error marker (`__nrpc_error`); use inside methods. |
| `nrpc.ok(value)` | Pass-through success helper for handlers. |
| `nrpc.parse_error(id?)` / `invalid_request` / `method_not_found` / `invalid_params` / `internal_error` | Standard error responses. |

### Codec

| Method | Description |
|--------|-------------|
| `nrpc.encode(msg)` | Encode a request/response object to JSON text. |
| `nrpc.decode(text)` | Decode JSON text to a message object or batch array. |
| `nrpc.encode_batch(msgs)` | Encode a non-empty array of messages. |
| `nrpc.valid(text)` | `true` when text is a valid JSON-RPC message. |

### Inspection

| Method | Description |
|--------|-------------|
| `nrpc.is_request(msg)` | Has `method` and `id`. |
| `nrpc.is_notification(msg)` | Has `method`, no `id`. |
| `nrpc.is_response(msg)` | Has `result` or `error`. |
| `nrpc.is_error(msg)` | Has `error`. |
| `nrpc.is_batch(msg)` | Top-level array. |

### Server (jsonrpcserver-style)

| Method | Description |
|--------|-------------|
| `nrpc.new_server()` | Create a server handle (int). (`server` is a reserved keyword.) |
| `nrpc.method(server, name, handler)` | Register handler. `handler` is a callable **or** a global function name string. Return `nrpc.err(...)` for RPC errors. |
| `nrpc.methods(server)` | Sorted list of method names. |
| `nrpc.dispatch(server, request)` | Dispatch string/object; returns response object, batch array, or `nil` for notifications. |
| `nrpc.close(server)` | Free the server handle. |

### Client

| Method | Description |
|--------|-------------|
| `nrpc.new_client()` | Create a client with auto-incrementing ids. (`client` may be reserved in some contexts.) |
| `nrpc.call(client, method, params?)` | Build the next request. |
| `nrpc.notify_call(client, method, params?)` | Build a notification. |
| `nrpc.next_id(client)` | Peek the next id. |
| `nrpc.parse_result(response)` | `{ok, id, result}` or `{ok: false, id, error}`. |
| `nrpc.close_client(client)` | Free the client handle. |

### Framing

| Method | Description |
|--------|-------------|
| `nrpc.frame(msg, style?)` | Frame a message (`"ndjson"` default, or `"content-length"` / `"lsp"`). |
| `nrpc.unframe(buffer, style?)` | `{messages, rest}` from a growable stream buffer. |

### Transports

| Method | Description |
|--------|-------------|
| `nrpc.stdio_exchange(server, input, style?)` | One framed exchange (tests / stdio pipes). |
| `nrpc.handle_http_body(server, body)` | Dispatch a raw HTTP JSON body; returns response JSON (empty for notification). |
| `nrpc.tcp_serve_once(server, host, port, opts?)` | Accept one TCP client; NDJSON by default. |
| `nrpc.tcp_call(host, port, method, params?, opts?)` | One-shot TCP request/response. |
| `nrpc.http_serve_once(server, host, port, path?)` | Accept one HTTP POST; default path `/`. |
| `nrpc.http_call(url, method, params?)` | POST `http://host:port/path`. |

### Constants

| Method | Value |
|--------|-------|
| `nrpc.PARSE_ERROR()` | `-32700` |
| `nrpc.INVALID_REQUEST()` | `-32600` |
| `nrpc.METHOD_NOT_FOUND()` | `-32601` |
| `nrpc.INVALID_PARAMS()` | `-32602` |
| `nrpc.INTERNAL_ERROR()` | `-32603` |

### Transport options

| Key | Default | Description |
|-----|---------|-------------|
| `style` | `"ndjson"` | `"ndjson"` or `"content-length"`. |
| `timeout_ms` | `30000` | Socket timeout. |
| `max_requests` | `10000` | Cap for `tcp_serve_once`. |

## Errors

| Code | Meaning |
|------|---------|
| 4450 | Wrong argument count. |
| 4451 | Catchable `nrpc_error` (I/O, transport, limits). |
| 4452 | Wrong argument type (hard error). |
| 4453 | Parse / invalid JSON-RPC structure. |
| 4454 | Invalid server/client handle. |

## Limits

Payloads and framed messages are capped at **16 MiB**.

## See also

- [`net`](NET.md) — general TCP/HTTP servers
- [`json`](JSON.md) — JSON parse/stringify
- [`nbench`](NBENCH.md) / [`ntest`](NTEST.md)
