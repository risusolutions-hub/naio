# nmqtt — MQTT 3.1.1 / 5 client

Synchronous MQTT client for IoT and edge fleets: QoS 0–2, TLS (rustls), last wills, optional reconnect, and topic wildcards. Surface mirrors the essentials of Eclipse Paho while staying idiomatic Niao (`nmqtt.connect(config)` + handle methods).

## Import

```niao
import "nmqtt"
```

Paths `import "std/nmqtt"` and `import "nmqtt"` are equivalent. Flat builtins (`nmqtt_connect`, `nmqtt_publish`, …) are also available globally after import.

## Quick start

```niao
import "nmqtt"

fn main() {
    let id = nmqtt.connect({
        host: "127.0.0.1",
        port: 1883,
        client_id: "sensor-1",
        will: { topic: "status/sensor-1", payload: "offline", qos: 1, retain: true }
    })
    nmqtt.subscribe(id, "commands/#", 1)
    nmqtt.publish(id, "telemetry/temp", "21.5", {qos: 1})
    let msg = nmqtt.recv(id, 5000)   // {topic, payload, qos, retain, dup} or nil
    print(msg)
    nmqtt.close(id)
}
```

Offline packet helpers (no broker required):

```niao
import "nmqtt"

fn main() {
    let pkt = nmqtt.encode_publish("fleet/edge-1/ping", "ok", {qos: 0})
    print(nmqtt.packet_type(pkt))          // PUBLISH
    print(nmqtt.topic_matches("fleet/+/ping", "fleet/edge-1/ping"))  // true
}
```

## Connect config

| Field | Type | Default | Notes |
|-------|------|---------|-------|
| `host` | string | required | Broker hostname |
| `port` | int | 1883 / 8883 | 8883 when `tls` is true |
| `client_id` | string | auto | Empty generates `niao-<id>` |
| `username` / `password` | string | — | Optional auth |
| `keepalive` | int | 60 | Seconds |
| `clean_session` / `clean_start` | bool | true | Session flag |
| `protocol` | `"3.1.1"` / `"5"` / `4` / `5` | `3.1.1` | MQTT version |
| `tls` | bool | false | rustls + system roots |
| `will` | object | — | `{topic, payload?, qos?, retain?}` |
| `reconnect` | bool / object | false | `{enabled, delay_ms, max_delay_ms}` |

## Functions

| Method | Description |
|--------|-------------|
| `nmqtt.connect(config)` | Open TCP/TLS and CONNECT. Returns handle int, or catchable `nmqtt_error`. |
| `nmqtt.publish(id, topic, payload, opts?)` | Publish. `payload` is string, bytearray, or `int[]`. `opts`: `{qos, retain}`. |
| `nmqtt.subscribe(id, topic\|topics, qos?)` | Subscribe (qos default 0). |
| `nmqtt.unsubscribe(id, topic\|topics)` | Unsubscribe. |
| `nmqtt.recv(id, timeout_ms?)` | Next message object, or `nil` on timeout. Blocks if timeout omitted. |
| `nmqtt.disconnect(id)` | Send DISCONNECT; keep handle (for reconnect). |
| `nmqtt.reconnect(id)` | Reconnect and restore subscriptions. |
| `nmqtt.is_connected(id)` | Connection flag. |
| `nmqtt.client_id(id)` | Session client id string. |
| `nmqtt.ping(id)` | PINGREQ / PINGRESP. |
| `nmqtt.close(id)` | Disconnect and free the handle. |
| `nmqtt.topic_matches(filter, topic)` | MQTT `+` / `#` matching. |
| `nmqtt.encode_connect(config)` | Encode CONNECT bytes (offline). |
| `nmqtt.encode_publish(topic, payload, opts?)` | Encode PUBLISH bytes. |
| `nmqtt.decode_packet(bytes)` | Decode one packet to an object. |
| `nmqtt.packet_type(bytes)` | Fixed-header type name (`"PUBLISH"`, …). |

## Message object

`recv` returns:

```
{ topic: string, payload: string|bytearray, qos: int, retain: bool, dup: bool }
```

UTF-8 payloads become strings; otherwise a byte array.

## Errors

| Code | Meaning |
|------|---------|
| 4130 | Wrong argument count. |
| 4131 | Connection / I/O / broker error (often catchable `nmqtt_error`). |
| 4132 | Wrong argument type or bad config. |
| 4133 | Invalid or closed handle. |
| 4134 | Protocol / decode / CONNACK refusal. |

## See also

- `nws` — WebSocket client
- `nsmtp` / `nmail` — outbound email
- `net` — general networking
