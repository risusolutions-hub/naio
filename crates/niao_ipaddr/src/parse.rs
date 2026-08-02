//! Parsing addresses, networks, and interfaces.

use crate::addr::{ipv4_to_u32, ipv6_to_u128};
use crate::error::{IpError, IpResult};
use ipnet::{Ipv4Net, Ipv6Net};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

fn parse_v4_addr(s: &str) -> IpResult<Ipv4Addr> {
    Ipv4Addr::from_str(s).map_err(|e| IpError::Parse(format!("invalid IPv4 address '{s}': {e}")))
}

fn parse_v6_addr(s: &str) -> IpResult<Ipv6Addr> {
    Ipv6Addr::from_str(s).map_err(|e| IpError::Parse(format!("invalid IPv6 address '{s}': {e}")))
}

/// Auto-detect IPv4 vs IPv6 address.
///
/// >>> parse_address("192.168.0.1")
pub fn parse_address(s: &str) -> IpResult<IpEntity> {
    let s = s.trim();
    if s.contains(':') {
        Ok(IpEntity::V6Addr(parse_v6_addr(s)?))
    } else {
        Ok(IpEntity::V4Addr(parse_v4_addr(s)?))
    }
}

pub fn parse_ipv4_address(s: &str) -> IpResult<Ipv4Addr> {
    parse_v4_addr(s.trim())
}

pub fn parse_ipv6_address(s: &str) -> IpResult<Ipv6Addr> {
    parse_v6_addr(s.trim())
}

fn v4_host_bits(addr: Ipv4Addr, prefix: u8) -> u32 {
    let mask = if prefix == 0 {
        0
    } else {
        !0u32 << (32 - prefix)
    };
    ipv4_to_u32(addr) & !mask
}

fn v6_host_bits(addr: &Ipv6Addr, prefix: u8) -> u128 {
    let mask = if prefix == 0 {
        0
    } else {
        !0u128 << (128 - prefix)
    };
    ipv6_to_u128(addr) & !mask
}

fn parse_v4_prefix(s: &str) -> IpResult<(Ipv4Addr, u8)> {
    let (addr_s, plen_s) = s
        .split_once('/')
        .ok_or_else(|| IpError::Parse(format!("'{s}' does not appear to be an IPv4 network")))?;
    let addr = parse_v4_addr(addr_s.trim())?;
    if plen_s.contains('.') {
        let mask = parse_v4_addr(plen_s.trim())?;
        let prefix = mask_to_prefix_v4(mask)?;
        return Ok((addr, prefix));
    }
    let prefix: u8 = plen_s
        .trim()
        .parse()
        .map_err(|_| IpError::Parse(format!("invalid prefix '{plen_s}'")))?;
    if prefix > 32 {
        return Err(IpError::PrefixOutOfRange);
    }
    Ok((addr, prefix))
}

fn mask_to_prefix_v4(mask: Ipv4Addr) -> IpResult<u8> {
    let m = ipv4_to_u32(mask);
    if m == 0 {
        return Ok(0);
    }
    if m.count_ones() + m.trailing_zeros() != 32 || m.leading_zeros() == 32 {
        return Err(IpError::Parse("invalid netmask".into()));
    }
    Ok(m.count_ones() as u8)
}

fn u32_to_ipv4_addr(n: u32) -> Ipv4Addr {
    Ipv4Addr::from(n)
}

fn parse_v6_prefix(s: &str) -> IpResult<(Ipv6Addr, u8)> {
    let (addr_s, plen_s) = s
        .split_once('/')
        .ok_or_else(|| IpError::Parse(format!("'{s}' does not appear to be an IPv6 network")))?;
    let addr = parse_v6_addr(addr_s.trim())?;
    let prefix: u8 = plen_s
        .trim()
        .parse()
        .map_err(|_| IpError::Parse(format!("invalid prefix '{plen_s}'")))?;
    if prefix > 128 {
        return Err(IpError::PrefixOutOfRange);
    }
    Ok((addr, prefix))
}

/// Parse CIDR or address/prefix network notation.
///
/// >>> parse_network("192.168.0.0/24", true) // Ok(V4Net)
pub fn parse_network(s: &str, strict: bool) -> IpResult<IpEntity> {
    let s = s.trim();
    if s.contains(':') {
        let (addr, prefix) = parse_v6_prefix(s)?;
        if strict && v6_host_bits(&addr, prefix) != 0 {
            return Err(IpError::HostBitsSet);
        }
        let net = Ipv6Net::new(addr, prefix).map_err(|e| IpError::Parse(e.to_string()))?;
        Ok(IpEntity::V6Net(net))
    } else if s.contains('/') {
        let (addr, prefix) = parse_v4_prefix(s)?;
        if strict && v4_host_bits(addr, prefix) != 0 {
            return Err(IpError::HostBitsSet);
        }
        let net = Ipv4Net::new(addr, prefix).map_err(|e| IpError::Parse(e.to_string()))?;
        Ok(IpEntity::V4Net(net))
    } else {
        Err(IpError::Parse(format!("'{s}' missing '/' prefix length")))
    }
}

pub fn parse_ipv4_network(s: &str, strict: bool) -> IpResult<Ipv4Net> {
    match parse_network(s, strict)? {
        IpEntity::V4Net(n) => Ok(n),
        _ => Err(IpError::Parse("expected IPv4 network".into())),
    }
}

pub fn parse_ipv6_network(s: &str, strict: bool) -> IpResult<Ipv6Net> {
    match parse_network(s, strict)? {
        IpEntity::V6Net(n) => Ok(n),
        _ => Err(IpError::Parse("expected IPv6 network".into())),
    }
}

/// Parse `addr/prefix` interface notation.
///
/// >>> parse_interface("10.0.0.1/8") // Ok(V4Iface)
pub fn parse_interface(s: &str) -> IpResult<IpEntity> {
    let s = s.trim();
    if !s.contains('/') {
        return Err(IpError::Parse(format!(
            "'{s}' missing '/' prefix length for interface"
        )));
    }
    if s.contains(':') {
        let (addr, prefix) = parse_v6_prefix(s)?;
        Ok(IpEntity::V6Iface { addr, prefix })
    } else {
        let (addr, prefix) = parse_v4_prefix(s)?;
        Ok(IpEntity::V4Iface { addr, prefix })
    }
}

pub fn valid_address(s: &str) -> bool {
    parse_address(s).is_ok()
}

pub fn valid_network(s: &str, strict: bool) -> bool {
    parse_network(s, strict).is_ok()
}

pub fn valid_interface(s: &str) -> bool {
    parse_interface(s).is_ok()
}

/// Unified IP entity stored behind runtime handles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IpEntity {
    V4Addr(Ipv4Addr),
    V6Addr(Ipv6Addr),
    V4Net(Ipv4Net),
    V6Net(Ipv6Net),
    V4Iface { addr: Ipv4Addr, prefix: u8 },
    V6Iface { addr: Ipv6Addr, prefix: u8 },
}

impl IpEntity {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::V4Addr(_) => "ipv4",
            Self::V6Addr(_) => "ipv6",
            Self::V4Net(_) => "ipv4_network",
            Self::V6Net(_) => "ipv6_network",
            Self::V4Iface { .. } => "ipv4_interface",
            Self::V6Iface { .. } => "ipv6_interface",
        }
    }

    pub fn version(&self) -> u8 {
        match self {
            Self::V4Addr(_) | Self::V4Net(_) | Self::V4Iface { .. } => 4,
            Self::V6Addr(_) | Self::V6Net(_) | Self::V6Iface { .. } => 6,
        }
    }

    pub fn display(&self) -> String {
        match self {
            Self::V4Addr(a) => a.to_string(),
            Self::V6Addr(a) => a.to_string(),
            Self::V4Net(n) => n.to_string(),
            Self::V6Net(n) => n.to_string(),
            Self::V4Iface { addr, prefix } => format!("{addr}/{prefix}"),
            Self::V6Iface { addr, prefix } => format!("{addr}/{prefix}"),
        }
    }

    pub fn packed(&self) -> Vec<u8> {
        match self {
            Self::V4Addr(a) => a.octets().to_vec(),
            Self::V6Addr(a) => a.octets().to_vec(),
            Self::V4Net(n) => n.network().octets().to_vec(),
            Self::V6Net(n) => n.network().octets().to_vec(),
            Self::V4Iface { addr, .. } => addr.octets().to_vec(),
            Self::V6Iface { addr, .. } => addr.octets().to_vec(),
        }
    }
}

pub fn prefix_to_netmask_v4(prefix: u8) -> Ipv4Addr {
    if prefix == 0 {
        Ipv4Addr::new(0, 0, 0, 0)
    } else {
        u32_to_ipv4_addr(!0u32 << (32 - prefix))
    }
}

pub fn prefix_to_hostmask_v4(prefix: u8) -> Ipv4Addr {
    if prefix == 32 {
        Ipv4Addr::new(0, 0, 0, 0)
    } else {
        u32_to_ipv4_addr((1u32 << (32 - prefix)) - 1)
    }
}

pub fn netmask_to_prefix_v4(mask: Ipv4Addr) -> IpResult<u8> {
    mask_to_prefix_v4(mask)
}

pub fn hostmask_to_prefix_v4(hostmask: Ipv4Addr) -> IpResult<u8> {
    let h = ipv4_to_u32(hostmask);
    mask_to_prefix_v4(u32_to_ipv4_addr(!h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_v4_net_strict() {
        assert!(parse_ipv4_network("192.168.1.0/24", true).is_ok());
        assert!(parse_ipv4_network("192.168.1.1/24", true).is_err());
        assert!(parse_ipv4_network("192.168.1.1/24", false).is_ok());
    }

    #[test]
    fn parse_v4_netmask() {
        let n = parse_ipv4_network("10.0.0.0/255.0.0.0", true).unwrap();
        assert_eq!(n.prefix_len(), 8);
    }

    #[test]
    fn interface_roundtrip() {
        let e = parse_interface("10.0.0.1/8").unwrap();
        assert_eq!(e.kind(), "ipv4_interface");
    }
}
