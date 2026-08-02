//! Network operations: contains, subnets, supernets, hosts.

use crate::addr::{ipv4_to_u32, ipv6_to_u128, u128_to_ipv6, u32_to_ipv4};
use crate::error::{IpError, IpResult};
use crate::parse::{prefix_to_hostmask_v4, prefix_to_netmask_v4, IpEntity};
use ipnet::{Ipv4Net, Ipv6Net};
use std::net::{Ipv4Addr, Ipv6Addr};

pub const DEFAULT_MAX_HOSTS: usize = 1_048_576;

pub fn entity_contains(container: &IpEntity, other: &IpEntity) -> IpResult<bool> {
    match (container, other) {
        (IpEntity::V4Net(net), IpEntity::V4Addr(addr)) => Ok(net.contains(addr)),
        (IpEntity::V6Net(net), IpEntity::V6Addr(addr)) => Ok(net.contains(addr)),
        (IpEntity::V4Net(a), IpEntity::V4Net(b)) => Ok(a.contains(b)),
        (IpEntity::V6Net(a), IpEntity::V6Net(b)) => Ok(a.contains(b)),
        (IpEntity::V4Net(net), IpEntity::V4Iface { addr, .. }) => Ok(net.contains(addr)),
        (IpEntity::V6Net(net), IpEntity::V6Iface { addr, .. }) => Ok(net.contains(addr)),
        (
            IpEntity::V4Iface {
                addr: ia,
                prefix: ip,
            },
            IpEntity::V4Addr(a),
        ) => {
            let net = Ipv4Net::new(*ia, *ip).map_err(|e| IpError::Parse(e.to_string()))?;
            Ok(net.contains(a))
        }
        (
            IpEntity::V6Iface {
                addr: ia,
                prefix: ip,
            },
            IpEntity::V6Addr(a),
        ) => {
            let net = Ipv6Net::new(*ia, *ip).map_err(|e| IpError::Parse(e.to_string()))?;
            Ok(net.contains(a))
        }
        _ => Err(IpError::VersionMismatch),
    }
}

pub fn entity_overlaps(a: &IpEntity, b: &IpEntity) -> IpResult<bool> {
    match (a, b) {
        (IpEntity::V4Net(x), IpEntity::V4Net(y)) => Ok(v4_overlaps(x, y)),
        (IpEntity::V6Net(x), IpEntity::V6Net(y)) => Ok(v6_overlaps(x, y)),
        (IpEntity::V4Net(x), IpEntity::V4Addr(y)) => Ok(x.contains(y)),
        (IpEntity::V6Net(x), IpEntity::V6Addr(y)) => Ok(x.contains(y)),
        (IpEntity::V4Addr(x), IpEntity::V4Net(y)) => Ok(y.contains(x)),
        (IpEntity::V6Addr(x), IpEntity::V6Net(y)) => Ok(y.contains(x)),
        _ => Err(IpError::VersionMismatch),
    }
}

fn v4_overlaps(a: &Ipv4Net, b: &Ipv4Net) -> bool {
    a.contains(&b.network()) || b.contains(&a.network())
}

fn v6_overlaps(a: &Ipv6Net, b: &Ipv6Net) -> bool {
    a.contains(&b.network()) || b.contains(&a.network())
}

pub fn subnet_of(a: &IpEntity, b: &IpEntity) -> IpResult<bool> {
    match (a, b) {
        (IpEntity::V4Net(x), IpEntity::V4Net(y)) => Ok(y.contains(x)),
        (IpEntity::V6Net(x), IpEntity::V6Net(y)) => Ok(y.contains(x)),
        _ => Err(IpError::NotNetwork),
    }
}

pub fn supernet_of(a: &IpEntity, b: &IpEntity) -> IpResult<bool> {
    subnet_of(b, a)
}

pub fn num_addresses_v4(net: &Ipv4Net) -> u128 {
    let host_bits = 32 - net.prefix_len();
    1u128 << host_bits
}

pub fn num_addresses_v6(net: &Ipv6Net) -> u128 {
    let host_bits = 128 - net.prefix_len();
    if host_bits >= 64 {
        u128::MAX
    } else {
        1u128 << host_bits
    }
}

pub fn broadcast_v4(net: &Ipv4Net) -> Ipv4Addr {
    let base = ipv4_to_u32(net.network());
    let host_bits = 32 - net.prefix_len();
    let broadcast = base | ((1u32 << host_bits) - 1);
    u32_to_ipv4(broadcast)
}

pub fn collect_hosts_v4(net: &Ipv4Net, max: usize) -> IpResult<Vec<Ipv4Addr>> {
    let count = num_addresses_v4(net) as usize;
    let prefix = net.prefix_len();
    let mut hosts = Vec::new();
    let (start, end) = v4_host_range(net);
    let total = end.saturating_sub(start).saturating_add(1) as usize;
    if total > max {
        return Err(IpError::TooManyHosts);
    }
    let mut cur = start;
    while cur <= end {
        hosts.push(u32_to_ipv4(cur));
        cur = cur.saturating_add(1);
        if hosts.len() > max {
            return Err(IpError::TooManyHosts);
        }
    }
    let _ = count;
    let _ = prefix;
    Ok(hosts)
}

fn v4_host_range(net: &Ipv4Net) -> (u32, u32) {
    let base = ipv4_to_u32(net.network());
    let prefix = net.prefix_len();
    let host_bits = 32 - prefix;
    if prefix >= 31 {
        // RFC 3021: /31 and /32 include all addresses
        let end = base | ((1u32 << host_bits).saturating_sub(1));
        return (base, end);
    }
    let start = base + 1;
    let end = (base | ((1u32 << host_bits) - 1)).saturating_sub(1);
    (start, end)
}

pub fn collect_hosts_v6(net: &Ipv6Net, max: usize) -> IpResult<Vec<Ipv6Addr>> {
    let base = ipv6_to_u128(&net.network());
    let host_bits = 128 - net.prefix_len();
    if host_bits >= 64 {
        return Err(IpError::TooManyHosts);
    }
    let count = (1u128 << host_bits) as usize;
    if count > max {
        return Err(IpError::TooManyHosts);
    }
    let mut hosts = Vec::with_capacity(count);
    for i in 0..count {
        hosts.push(u128_to_ipv6(base + i as u128));
    }
    Ok(hosts)
}

pub fn subnets_v4(net: &Ipv4Net, new_prefix: u8) -> IpResult<Vec<Ipv4Net>> {
    if new_prefix < net.prefix_len() {
        return Err(IpError::PrefixOutOfRange);
    }
    Ok(net
        .subnets(new_prefix)
        .map_err(|e| IpError::Parse(e.to_string()))?
        .collect())
}

pub fn subnets_v6(net: &Ipv6Net, new_prefix: u8) -> IpResult<Vec<Ipv6Net>> {
    if new_prefix < net.prefix_len() {
        return Err(IpError::PrefixOutOfRange);
    }
    Ok(net
        .subnets(new_prefix)
        .map_err(|e| IpError::Parse(e.to_string()))?
        .collect())
}

pub fn supernet_v4(net: &Ipv4Net, prefix_diff: u8) -> IpResult<Ipv4Net> {
    let mut cur = *net;
    for _ in 0..prefix_diff {
        cur = cur.supernet().ok_or_else(|| IpError::PrefixOutOfRange)?;
    }
    Ok(cur)
}

pub fn supernet_v6(net: &Ipv6Net, prefix_diff: u8) -> IpResult<Ipv6Net> {
    let mut cur = *net;
    for _ in 0..prefix_diff {
        cur = cur.supernet().ok_or_else(|| IpError::PrefixOutOfRange)?;
    }
    Ok(cur)
}

pub fn with_prefixlen(entity: &IpEntity, new_prefix: u8) -> IpResult<IpEntity> {
    match entity {
        IpEntity::V4Net(n) => {
            let addr = n.network();
            let net = Ipv4Net::new(addr, new_prefix).map_err(|e| IpError::Parse(e.to_string()))?;
            Ok(IpEntity::V4Net(net))
        }
        IpEntity::V6Net(n) => {
            let addr = n.network();
            let net = Ipv6Net::new(addr, new_prefix).map_err(|e| IpError::Parse(e.to_string()))?;
            Ok(IpEntity::V6Net(net))
        }
        IpEntity::V4Iface { addr, .. } => Ok(IpEntity::V4Iface {
            addr: *addr,
            prefix: new_prefix,
        }),
        IpEntity::V6Iface { addr, .. } => Ok(IpEntity::V6Iface {
            addr: *addr,
            prefix: new_prefix,
        }),
        _ => Err(IpError::NotNetwork),
    }
}

pub fn with_netmask_v4(entity: &IpEntity, mask: Ipv4Addr) -> IpResult<IpEntity> {
    let prefix = crate::parse::netmask_to_prefix_v4(mask)?;
    with_prefixlen(entity, prefix)
}

pub fn with_hostmask_v4(entity: &IpEntity, hostmask: Ipv4Addr) -> IpResult<IpEntity> {
    let h = ipv4_to_u32(hostmask);
    let mask = u32_to_ipv4(!h);
    with_netmask_v4(entity, mask)
}

pub fn iface_network(entity: &IpEntity) -> IpResult<IpEntity> {
    match entity {
        IpEntity::V4Iface { addr, prefix } => {
            let net = Ipv4Net::new(*addr, *prefix).map_err(|e| IpError::Parse(e.to_string()))?;
            Ok(IpEntity::V4Net(net.trunc()))
        }
        IpEntity::V6Iface { addr, prefix } => {
            let net = Ipv6Net::new(*addr, *prefix).map_err(|e| IpError::Parse(e.to_string()))?;
            Ok(IpEntity::V6Net(net.trunc()))
        }
        _ => Err(IpError::NotInterface),
    }
}

pub fn iface_ip(entity: &IpEntity) -> IpResult<IpEntity> {
    match entity {
        IpEntity::V4Iface { addr, .. } => Ok(IpEntity::V4Addr(*addr)),
        IpEntity::V6Iface { addr, .. } => Ok(IpEntity::V6Addr(*addr)),
        _ => Err(IpError::NotInterface),
    }
}

pub fn netmask_v4(net: &Ipv4Net) -> Ipv4Addr {
    prefix_to_netmask_v4(net.prefix_len())
}

pub fn hostmask_v4(net: &Ipv4Net) -> Ipv4Addr {
    prefix_to_hostmask_v4(net.prefix_len())
}

pub fn address_exclude_v4(a: &Ipv4Net, b: &Ipv4Net) -> IpResult<Vec<Ipv4Net>> {
    if !a.contains(b) {
        return Err(IpError::Parse(
            "second network not contained in first".into(),
        ));
    }
    if a == b {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let a_start = ipv4_to_u32(a.network());
    let a_end = ipv4_to_u32(broadcast_v4(a));
    let b_start = ipv4_to_u32(b.network());
    let b_end = ipv4_to_u32(broadcast_v4(b));
    if b_start > a_start {
        let left_end = b_start.saturating_sub(1);
        out.extend(summarize_v4_range(a_start, left_end)?);
    }
    if b_end < a_end {
        let right_start = b_end.saturating_add(1);
        out.extend(summarize_v4_range(right_start, a_end)?);
    }
    Ok(out)
}

fn summarize_v4_range(start: u32, end: u32) -> IpResult<Vec<Ipv4Net>> {
    if start > end {
        return Ok(Vec::new());
    }
    let mut nets = Vec::new();
    let mut cur = start;
    while cur <= end {
        let max_size = cur & (!cur + 1);
        let mut size = max_size.max(1);
        while cur + size - 1 > end {
            size >>= 1;
        }
        let prefix = 32 - (size.trailing_zeros() as u8);
        let net =
            Ipv4Net::new(u32_to_ipv4(cur), prefix).map_err(|e| IpError::Parse(e.to_string()))?;
        nets.push(net);
        cur = cur.saturating_add(size);
    }
    Ok(nets)
}

pub fn address_exclude_v6(a: &Ipv6Net, b: &Ipv6Net) -> IpResult<Vec<Ipv6Net>> {
    if !a.contains(b) {
        return Err(IpError::Parse(
            "second network not contained in first".into(),
        ));
    }
    if a == b {
        return Ok(Vec::new());
    }
    let a_start = ipv6_to_u128(&a.network());
    let a_end = ipv6_to_u128(&a.broadcast());
    let b_start = ipv6_to_u128(&b.network());
    let b_end = ipv6_to_u128(&b.broadcast());
    let mut out = Vec::new();
    if b_start > a_start {
        out.extend(summarize_v6_range(a_start, b_start - 1)?);
    }
    if b_end < a_end {
        out.extend(summarize_v6_range(b_end + 1, a_end)?);
    }
    Ok(out)
}

fn summarize_v6_range(start: u128, end: u128) -> IpResult<Vec<Ipv6Net>> {
    if start > end {
        return Ok(Vec::new());
    }
    let mut nets = Vec::new();
    let mut cur = start;
    while cur <= end {
        let max_size = cur & (!cur + 1);
        let mut size = max_size.max(1);
        while cur.saturating_add(size - 1) > end {
            size >>= 1;
        }
        let prefix = 128 - (size.trailing_zeros() as u8);
        let net =
            Ipv6Net::new(u128_to_ipv6(cur), prefix).map_err(|e| IpError::Parse(e.to_string()))?;
        nets.push(net);
        cur = cur.saturating_add(size);
    }
    Ok(nets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn contains_v4() {
        let net = Ipv4Net::from_str("192.168.0.0/24").unwrap();
        let addr = Ipv4Addr::new(192, 168, 0, 5);
        assert!(entity_contains(&IpEntity::V4Net(net), &IpEntity::V4Addr(addr)).unwrap());
    }

    #[test]
    fn hosts_slash_24() {
        let net = Ipv4Net::from_str("192.168.1.0/24").unwrap();
        let hosts = collect_hosts_v4(&net, 300).unwrap();
        assert_eq!(hosts.len(), 254);
    }
}
