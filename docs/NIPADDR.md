# nipaddr — IPv4/IPv6 addresses & CIDR networks

IPv4/IPv6 addresses, CIDR networks, ranges, subnet math, and membership checks. Native Rust implementation (~Python `ipaddress` subset).

## Import

```niao
import "nipaddr"
```

Paths `import "std/nipaddr"` and `import "nipaddr"` are equivalent. Flat builtins (`nipaddr_address`, `nipaddr_contains`, …) are also available globally after import.

## Quick start

```niao
import "nipaddr"

let addr = nipaddr.address("192.168.1.42")
print(nipaddr.to_string(addr))          // 192.168.1.42
print(nipaddr.is_private(addr))         // true

let net = nipaddr.network("10.0.0.0/8")
print(nipaddr.contains(net, nipaddr.ipv4("10.1.2.3")))  // true

let iface = nipaddr.interface("192.168.1.1/24")
print(nipaddr.ip(iface))                // 192.168.1.1
print(nipaddr.network_of(iface))        // 192.168.1.0/24

let blocks = nipaddr.summarize_range(
    nipaddr.ipv4("192.168.0.0"),
    nipaddr.ipv4("192.168.0.255")
)
print(len(blocks))                      // 1

nipaddr.close(addr)
nipaddr.close(net)
nipaddr.close(iface)
```

## Constructors

| Method | Description |
|--------|-------------|
| `nipaddr.address(s)` | Parse IPv4 or IPv6 address (auto-detect). |
| `nipaddr.ipv4(s)` / `nipaddr.ipv6(s)` | Parse version-specific address. |
| `nipaddr.network(s, strict?)` | Parse CIDR network (`strict` default `true`). |
| `nipaddr.ipv4_network(s, strict?)` | IPv4 network only. |
| `nipaddr.ipv6_network(s, strict?)` | IPv6 network only. |
| `nipaddr.interface(s)` | Parse `addr/prefix` interface notation. |
| `nipaddr.valid_address(s)` | `true` when string is a valid address. |
| `nipaddr.valid_network(s, strict?)` | `true` when string is a valid network. |
| `nipaddr.valid_interface(s)` | `true` when string is a valid interface. |
| `nipaddr.close(handle)` | Free handle; returns `true` if it existed. |

Constructors return an integer **handle** on success, or a catchable `nipaddr_error` object on parse failure.

## Introspection

| Method | Description |
|--------|-------------|
| `nipaddr.kind(h)` | `"ipv4"`, `"ipv6"`, `"ipv4_network"`, `"ipv6_network"`, `"ipv4_interface"`, `"ipv6_interface"`. |
| `nipaddr.version(h)` | `4` or `6`. |
| `nipaddr.to_string(h)` | Canonical string form. |
| `nipaddr.packed(h)` | Address bytes as `byte[]` (4 or 16 bytes). |
| `nipaddr.exploded(h)` | IPv6 fully expanded notation. |
| `nipaddr.compressed(h)` | IPv6 compressed notation. |
| `nipaddr.reverse_ptr(h)` | DNS PTR name (`in-addr.arpa` / `ip6.arpa`). |
| `nipaddr.max_prefixlen(h)` | `32` (IPv4) or `128` (IPv6). |

## Address classification

| Method | Description |
|--------|-------------|
| `nipaddr.is_private(h)` | RFC 1918 / ULA (`fc00::/7`). |
| `nipaddr.is_global(h)` | Routable global unicast. |
| `nipaddr.is_link_local(h)` | `169.254.0.0/16` / `fe80::/10`. |
| `nipaddr.is_loopback(h)` | `127.0.0.0/8` / `::1`. |
| `nipaddr.is_multicast(h)` | Multicast ranges. |
| `nipaddr.is_reserved(h)` | IANA reserved / documentation ranges. |
| `nipaddr.is_unspecified(h)` | `0.0.0.0` / `::`. |
| `nipaddr.is_site_local(h)` | Deprecated IPv6 site-local (`fec0::/10`); always `false` for IPv4. |

## Arithmetic & comparison

| Method | Description |
|--------|-------------|
| `nipaddr.add(h, n)` | Add integer offset to address. |
| `nipaddr.compare(a, b)` | `-1`, `0`, or `1` (addresses, same version). |

## Network operations

| Method | Description |
|--------|-------------|
| `nipaddr.network_address(net)` | Network address. |
| `nipaddr.broadcast_address(net)` | IPv4 broadcast address. |
| `nipaddr.prefixlen(h)` | CIDR prefix length. |
| `nipaddr.netmask(h)` / `nipaddr.hostmask(h)` | IPv4 masks as address handles. |
| `nipaddr.num_addresses(net)` | Host count (int or string for huge IPv6). |
| `nipaddr.contains(net, other)` | Membership: network ⊇ address/network/interface. |
| `nipaddr.overlaps(a, b)` | `true` when ranges overlap. |
| `nipaddr.subnet_of(a, b)` | `true` when `a` ⊆ `b`. |
| `nipaddr.supernet_of(a, b)` | `true` when `a` ⊇ `b`. |
| `nipaddr.hosts(net, max?)` | Usable host addresses as handle array (default max 1_048_576). |
| `nipaddr.subnets(net, new_prefix)` | Split into finer subnets. |
| `nipaddr.supernet(net, prefix_diff?)` | Parent network (default shrink prefix by 1). |
| `nipaddr.with_prefixlen(h, n)` | Change prefix length. |
| `nipaddr.with_netmask(h, mask)` | IPv4: set prefix from netmask handle. |
| `nipaddr.with_hostmask(h, mask)` | IPv4: set prefix from hostmask handle. |
| `nipaddr.address_exclude(a, b)` | Networks in `a` not in `b`. |

## Interface helpers

| Method | Description |
|--------|-------------|
| `nipaddr.ip(iface)` | Host address handle. |
| `nipaddr.network_of(iface)` | Corresponding network handle. |

## Range utilities

| Method | Description |
|--------|-------------|
| `nipaddr.summarize_range(first, last)` | Minimal CIDR list covering an address range. |
| `nipaddr.collapse(networks)` | Merge adjacent/overlapping networks. |

## Batch membership (parallel)

| Method | Description |
|--------|-------------|
| `nipaddr.contains_many(net, handles)` | `bool[]` parallel membership test. |
| `nipaddr.filter_contains(net, handles)` | Subset of handles inside the network. |

## Errors

| Code | Meaning |
|------|---------|
| 3484 | Wrong argument count. |
| 3485 | Parse / operation failure (catchable `nipaddr_error`). |
| 3486 | Wrong argument type (hard error). |
| 3487 | Invalid or closed handle. |

## See also

- `net` — sockets and HTTP (operate on resolved endpoints).
- `nvalid` — schema validation including basic IPv4 string checks.
- `nurl` — URL parsing (hostnames, not CIDR).
