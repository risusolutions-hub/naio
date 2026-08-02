//! Address range summarization and network collapse.

use crate::addr::{ipv4_to_u32, ipv6_to_u128, u128_to_ipv6, u32_to_ipv4};
use crate::error::{IpError, IpResult};
use crate::parse::IpEntity;
use ipnet::{Ipv4Net, Ipv6Net};

/// Summarize contiguous address range into minimal CIDR blocks.
///
/// >>> summarize_range("192.168.0.0", "192.168.0.255")
pub fn summarize_range(first: &IpEntity, last: &IpEntity) -> IpResult<Vec<IpEntity>> {
    match (first, last) {
        (IpEntity::V4Addr(a), IpEntity::V4Addr(b)) => {
            let start = ipv4_to_u32(*a);
            let end = ipv4_to_u32(*b);
            if start > end {
                return Err(IpError::EmptyRange);
            }
            Ok(summarize_v4(start, end)?
                .into_iter()
                .map(IpEntity::V4Net)
                .collect())
        }
        (IpEntity::V6Addr(a), IpEntity::V6Addr(b)) => {
            let start = ipv6_to_u128(a);
            let end = ipv6_to_u128(b);
            if start > end {
                return Err(IpError::EmptyRange);
            }
            Ok(summarize_v6(start, end)?
                .into_iter()
                .map(IpEntity::V6Net)
                .collect())
        }
        _ => Err(IpError::VersionMismatch),
    }
}

fn summarize_v4(start: u32, end: u32) -> IpResult<Vec<Ipv4Net>> {
    let mut nets = Vec::new();
    let mut cur = start;
    while cur <= end {
        let max_size = cur & (!cur + 1);
        let mut size = max_size.max(1);
        while cur.saturating_add(size - 1) > end {
            size >>= 1;
        }
        let prefix = 32 - size.trailing_zeros() as u8;
        let net =
            Ipv4Net::new(u32_to_ipv4(cur), prefix).map_err(|e| IpError::Parse(e.to_string()))?;
        nets.push(net);
        cur = cur.saturating_add(size);
    }
    Ok(nets)
}

fn summarize_v6(start: u128, end: u128) -> IpResult<Vec<Ipv6Net>> {
    let mut nets = Vec::new();
    let mut cur = start;
    while cur <= end {
        let max_size = cur & (!cur + 1);
        let mut size = max_size.max(1);
        while cur.saturating_add(size - 1) > end {
            size >>= 1;
        }
        let prefix = 128 - size.trailing_zeros() as u8;
        let net =
            Ipv6Net::new(u128_to_ipv6(cur), prefix).map_err(|e| IpError::Parse(e.to_string()))?;
        nets.push(net);
        cur = cur.saturating_add(size);
    }
    Ok(nets)
}

/// Merge adjacent/overlapping networks into minimal covering set.
pub fn collapse_networks(networks: &[IpEntity]) -> IpResult<Vec<IpEntity>> {
    if networks.is_empty() {
        return Ok(Vec::new());
    }
    let v4: Vec<Ipv4Net> = networks
        .iter()
        .filter_map(|e| match e {
            IpEntity::V4Net(n) => Some(*n),
            _ => None,
        })
        .collect();
    let v6: Vec<Ipv6Net> = networks
        .iter()
        .filter_map(|e| match e {
            IpEntity::V6Net(n) => Some(*n),
            _ => None,
        })
        .collect();
    if !v4.is_empty() && !v6.is_empty() {
        return Err(IpError::VersionMismatch);
    }
    if !v4.is_empty() {
        return Ok(collapse_v4(v4)?.into_iter().map(IpEntity::V4Net).collect());
    }
    Ok(collapse_v6(v6)?.into_iter().map(IpEntity::V6Net).collect())
}

fn collapse_v4(mut nets: Vec<Ipv4Net>) -> IpResult<Vec<Ipv4Net>> {
    nets.sort_by_key(|n| (ipv4_to_u32(n.network()), n.prefix_len()));
    nets.dedup();
    let mut changed = true;
    while changed {
        changed = false;
        let mut merged: Vec<Ipv4Net> = Vec::new();
        let mut i = 0;
        while i < nets.len() {
            if i + 1 < nets.len() {
                let a = nets[i];
                let b = nets[i + 1];
                if let Some(parent) = try_merge_v4(a, b) {
                    merged.push(parent);
                    i += 2;
                    changed = true;
                    continue;
                }
            }
            merged.push(nets[i]);
            i += 1;
        }
        nets = merged;
    }
    Ok(nets)
}

fn try_merge_v4(a: Ipv4Net, b: Ipv4Net) -> Option<Ipv4Net> {
    if a.prefix_len() != b.prefix_len() {
        return None;
    }
    let plen = a.prefix_len();
    if plen == 0 {
        return None;
    }
    let parent_plen = plen - 1;
    let pa = Ipv4Net::new(a.network(), parent_plen).ok()?;
    if pa.contains(&b) && pa.contains(&a) {
        return Some(pa);
    }
    None
}

fn collapse_v6(mut nets: Vec<Ipv6Net>) -> IpResult<Vec<Ipv6Net>> {
    nets.sort_by_key(|n| (ipv6_to_u128(&n.network()), n.prefix_len()));
    nets.dedup();
    let mut changed = true;
    while changed {
        changed = false;
        let mut merged: Vec<Ipv6Net> = Vec::new();
        let mut i = 0;
        while i < nets.len() {
            if i + 1 < nets.len() {
                let a = nets[i];
                let b = nets[i + 1];
                if let Some(parent) = try_merge_v6(a, b) {
                    merged.push(parent);
                    i += 2;
                    changed = true;
                    continue;
                }
            }
            merged.push(nets[i]);
            i += 1;
        }
        nets = merged;
    }
    Ok(nets)
}

fn try_merge_v6(a: Ipv6Net, b: Ipv6Net) -> Option<Ipv6Net> {
    if a.prefix_len() != b.prefix_len() {
        return None;
    }
    let plen = a.prefix_len();
    if plen == 0 {
        return None;
    }
    let parent_plen = plen - 1;
    let pa = Ipv6Net::new(a.network(), parent_plen).ok()?;
    if pa.contains(&b) && pa.contains(&a) {
        return Some(pa);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{parse_address, parse_network};
    use std::str::FromStr;

    #[test]
    fn summarize_slash_24() {
        let first = parse_address("192.168.0.0").unwrap();
        let last = parse_address("192.168.0.255").unwrap();
        let nets = summarize_range(&first, &last).unwrap();
        assert_eq!(nets.len(), 1);
        match &nets[0] {
            IpEntity::V4Net(n) => assert_eq!(n.to_string(), "192.168.0.0/24"),
            _ => panic!("expected v4 net"),
        }
    }

    #[test]
    fn collapse_adjacent() {
        let a = parse_network("192.168.0.0/25", true).unwrap();
        let b = parse_network("192.168.0.128/25", true).unwrap();
        let out = collapse_networks(&[a, b]).unwrap();
        assert_eq!(out.len(), 1);
    }
}
