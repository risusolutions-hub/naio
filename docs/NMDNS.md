# nmdns — mDNS / DNS-SD (zeroconf)

`nmdns` discovers and announces services on the local network with multicast DNS and DNS-SD (~Python `zeroconf`). Wire codec helpers work offline; browse/register need multicast.

Import with:

```niao
import "nmdns"
// or
import "std/nmdns"
```

---

## Quick start

```niao
import "nmdns"

let zc = nmdns.open()
let svc = nmdns.service("Demo", "_http._tcp", 8080, {
    properties: { path: "/" }
})
nmdns.register(zc, svc)
let found = nmdns.browse(zc, "_http._tcp", 300)
print(found)
nmdns.unregister(zc, svc)
nmdns.close(svc)
nmdns.close(zc)
```

Offline helpers (no multicast):

```niao
import "nmdns"

print(nmdns.service_type("_http._tcp"))   // _http._tcp.local.
let txt = nmdns.pack_txt({ path: "/" })
print(nmdns.unpack_txt(txt))
let q = nmdns.encode_query("_http._tcp.local.", "PTR")
print(nmdns.decode_message(q))
```

---

## Client & discovery

| Method | Description |
|--------|-------------|
| `nmdns.open()` | Create an mDNS client handle. |
| `nmdns.close(handle)` | Free client or service handle; returns `true` if it existed. |
| `nmdns.register(zc, svc)` | Announce a service on the LAN. |
| `nmdns.unregister(zc, svc)` | Send goodbye / stop announcing. |
| `nmdns.update(zc, svc)` | Re-announce after property/address changes. |
| `nmdns.browse(zc, type, timeout_ms?)` | Discover services (default timeout 1000 ms). Returns array of objects. |
| `nmdns.resolve(zc, name, type, timeout_ms?)` | Resolve one instance, or `nil`. |
| `nmdns.get_service_info(zc, type, name, timeout_ms?)` | Same as resolve with `(type, name)` order. |

---

## Service handles

| Method | Description |
|--------|-------------|
| `nmdns.service(name, type, port, opts?)` | Build a service handle. `opts`: `host`, `properties`/`props`, `addresses`/`addrs`, `priority`, `weight`, `ttl`. |
| `nmdns.info(svc)` | Snapshot object (`name`, `type`, `fullname`, `host`, `port`, …). |
| `nmdns.name(svc)` / `nmdns.type(svc)` / `nmdns.port(svc)` / `nmdns.host(svc)` / `nmdns.fullname(svc)` | Field accessors. |
| `nmdns.addresses(svc)` / `nmdns.properties(svc)` | Address list / TXT map. |
| `nmdns.set_property(svc, key, value)` | Set a TXT key (`string`/`int`/`bool`/`nil`). |
| `nmdns.add_address(svc, ip)` | Append an IPv4/IPv6 address string. |
| `nmdns.encode_response(svc)` | Encode announcement DNS message bytes. |

---

## Helpers (offline)

| Method | Description |
|--------|-------------|
| `nmdns.service_type(s)` | Normalize DNS-SD type to `._tcp.local.` / `._udp.local.` form. |
| `nmdns.is_mdns_type(s)` | `true` when `s` looks like a DNS-SD type. |
| `nmdns.localhost()` | Default host label for this machine. |
| `nmdns.mdns_group()` | Multicast group (`224.0.0.251`). |
| `nmdns.mdns_port()` | mDNS port (`5353`). |
| `nmdns.pack_txt(obj)` | Encode TXT key/values to bytes. |
| `nmdns.unpack_txt(bytes)` | Decode TXT bytes to object. |
| `nmdns.encode_query(name, type?)` | Build query bytes (`type` default `PTR`). |
| `nmdns.decode_message(bytes)` | Decode DNS message to object. |

---

## Errors

| Code | Meaning |
|------|---------|
| 3450 | Wrong argument count. |
| 3451 | mDNS / service error — catchable `nmdns_error`. |
| 3452 | Wrong argument type. |
| 3453 | Invalid or closed handle — catchable `nmdns_error`. |
| 3454 | Encode/decode failure — catchable `nmdns_error`. |
