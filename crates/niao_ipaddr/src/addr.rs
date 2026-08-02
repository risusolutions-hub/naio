//! Address classification and arithmetic.

use crate::error::{IpError, IpResult};
use ipnet::{Ipv4Net, Ipv6Net};
use std::cmp::Ordering;
use std::net::{Ipv4Addr, Ipv6Addr};

pub fn ipv4_to_u32(addr: Ipv4Addr) -> u32 {
    u32::from_be_bytes(addr.octets())
}

pub fn u32_to_ipv4(n: u32) -> Ipv4Addr {
    Ipv4Addr::from(n)
}

pub fn ipv6_to_u128(addr: &Ipv6Addr) -> u128 {
    u128::from_be_bytes(addr.octets())
}

pub fn u128_to_ipv6(n: u128) -> Ipv6Addr {
    Ipv6Addr::from(n)
}

/// >>> reverse_ptr_v4(Ipv4Addr::new(1, 2, 3, 4))
/// => "4.3.2.1.in-addr.arpa"
pub fn reverse_ptr_v4(addr: &Ipv4Addr) -> String {
    let o = addr.octets();
    format!("{}.{}.{}.{}.in-addr.arpa", o[3], o[2], o[1], o[0])
}

/// >>> reverse_ptr_v6 from ::1
pub fn reverse_ptr_v6(addr: &Ipv6Addr) -> String {
    let bytes = addr.octets();
    let mut nibbles = String::with_capacity(63);
    for b in bytes.iter().rev() {
        nibbles.push(hex_digit(b & 0x0f));
        nibbles.push('.');
        nibbles.push(hex_digit(b >> 4));
        nibbles.push('.');
    }
    nibbles.push_str("ip6.arpa");
    nibbles
}

fn hex_digit(n: u8) -> char {
    char::from(b"0123456789abcdef"[n as usize])
}

pub fn compare_v4(a: &Ipv4Addr, b: &Ipv4Addr) -> Ordering {
    ipv4_to_u32(*a).cmp(&ipv4_to_u32(*b))
}

pub fn compare_v6(a: &Ipv6Addr, b: &Ipv6Addr) -> Ordering {
    ipv6_to_u128(a).cmp(&ipv6_to_u128(b))
}

pub fn add_v4(addr: Ipv4Addr, delta: i64) -> IpResult<Ipv4Addr> {
    let base = ipv4_to_u32(addr) as i64;
    let n = base.checked_add(delta).ok_or(IpError::Overflow)?;
    if n < 0 || n > u32::MAX as i64 {
        return Err(IpError::Overflow);
    }
    Ok(u32_to_ipv4(n as u32))
}

pub fn add_v6(addr: Ipv6Addr, delta: i128) -> IpResult<Ipv6Addr> {
    let base = ipv6_to_u128(&addr) as i128;
    let n = base.checked_add(delta).ok_or(IpError::Overflow)?;
    if n < 0 {
        return Err(IpError::Overflow);
    }
    Ok(u128_to_ipv6(n as u128))
}

// --- IPv4 classification (Python ipaddress parity) ---

pub fn v4_is_private(addr: &Ipv4Addr) -> bool {
    Ipv4Net::new(Ipv4Addr::new(10, 0, 0, 0), 8)
        .unwrap()
        .contains(addr)
        || Ipv4Net::new(Ipv4Addr::new(172, 16, 0, 0), 12)
            .unwrap()
            .contains(addr)
        || Ipv4Net::new(Ipv4Addr::new(192, 168, 0, 0), 16)
            .unwrap()
            .contains(addr)
}

pub fn v4_is_loopback(addr: &Ipv4Addr) -> bool {
    Ipv4Net::new(Ipv4Addr::new(127, 0, 0, 0), 8)
        .unwrap()
        .contains(addr)
}

pub fn v4_is_link_local(addr: &Ipv4Addr) -> bool {
    Ipv4Net::new(Ipv4Addr::new(169, 254, 0, 0), 16)
        .unwrap()
        .contains(addr)
}

pub fn v4_is_multicast(addr: &Ipv4Addr) -> bool {
    Ipv4Net::new(Ipv4Addr::new(224, 0, 0, 0), 4)
        .unwrap()
        .contains(addr)
}

pub fn v4_is_unspecified(addr: &Ipv4Addr) -> bool {
    *addr == Ipv4Addr::UNSPECIFIED
}

pub fn v4_is_reserved(addr: &Ipv4Addr) -> bool {
    if v4_is_private(addr)
        || v4_is_loopback(addr)
        || v4_is_link_local(addr)
        || v4_is_multicast(addr)
    {
        return false;
    }
    // 0.0.0.0/8, 100.64.0.0/10, 192.0.0.0/24, 198.18.0.0/15, 240.0.0.0/4
    Ipv4Net::new(Ipv4Addr::new(0, 0, 0, 0), 8)
        .unwrap()
        .contains(addr)
        || Ipv4Net::new(Ipv4Addr::new(100, 64, 0, 0), 10)
            .unwrap()
            .contains(addr)
        || Ipv4Net::new(Ipv4Addr::new(192, 0, 0, 0), 24)
            .unwrap()
            .contains(addr)
        || Ipv4Net::new(Ipv4Addr::new(198, 18, 0, 0), 15)
            .unwrap()
            .contains(addr)
        || Ipv4Net::new(Ipv4Addr::new(240, 0, 0, 0), 4)
            .unwrap()
            .contains(addr)
}

pub fn v4_is_global(addr: &Ipv4Addr) -> bool {
    !v4_is_private(addr)
        && !v4_is_loopback(addr)
        && !v4_is_link_local(addr)
        && !v4_is_multicast(addr)
        && !v4_is_reserved(addr)
        && !v4_is_unspecified(addr)
}

// --- IPv6 classification ---

pub fn v6_is_private(addr: &Ipv6Addr) -> bool {
    Ipv6Net::new(Ipv6Addr::new(0xfc00, 0, 0, 0, 0, 0, 0, 0), 7)
        .unwrap()
        .contains(addr)
}

pub fn v6_is_loopback(addr: &Ipv6Addr) -> bool {
    *addr == Ipv6Addr::LOCALHOST
}

pub fn v6_is_link_local(addr: &Ipv6Addr) -> bool {
    Ipv6Net::new(Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 0), 10)
        .unwrap()
        .contains(addr)
}

pub fn v6_is_multicast(addr: &Ipv6Addr) -> bool {
    Ipv6Net::new(Ipv6Addr::new(0xff00, 0, 0, 0, 0, 0, 0, 0), 8)
        .unwrap()
        .contains(addr)
}

pub fn v6_is_unspecified(addr: &Ipv6Addr) -> bool {
    *addr == Ipv6Addr::UNSPECIFIED
}

pub fn v6_is_reserved(addr: &Ipv6Addr) -> bool {
    if v6_is_private(addr)
        || v6_is_loopback(addr)
        || v6_is_link_local(addr)
        || v6_is_multicast(addr)
        || v6_is_unspecified(addr)
    {
        return false;
    }
    // ::/128, ::1/128 handled above; 100::/64, 2001:db8::/32, 2001:10::/28
    Ipv6Net::new(Ipv6Addr::new(0x0100, 0, 0, 0, 0, 0, 0, 0), 64)
        .unwrap()
        .contains(addr)
        || Ipv6Net::new(Ipv6Addr::new(0x2001, 0x0db8, 0, 0, 0, 0, 0, 0), 32)
            .unwrap()
            .contains(addr)
        || Ipv6Net::new(Ipv6Addr::new(0x2001, 0x0010, 0, 0, 0, 0, 0, 0), 28)
            .unwrap()
            .contains(addr)
}

pub fn v6_is_site_local(addr: &Ipv6Addr) -> bool {
    Ipv6Net::new(Ipv6Addr::new(0xfec0, 0, 0, 0, 0, 0, 0, 0), 10)
        .unwrap()
        .contains(addr)
}

pub fn v6_is_global(addr: &Ipv6Addr) -> bool {
    !v6_is_private(addr)
        && !v6_is_loopback(addr)
        && !v6_is_link_local(addr)
        && !v6_is_multicast(addr)
        && !v6_is_reserved(addr)
        && !v6_is_unspecified(addr)
        && !v6_is_site_local(addr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn private_v4() {
        let a = Ipv4Addr::new(192, 168, 1, 1);
        assert!(v4_is_private(&a));
        assert!(!v4_is_global(&a));
    }

    #[test]
    fn add_v4_wrap() {
        let a = Ipv4Addr::new(255, 255, 255, 255);
        assert!(add_v4(a, 1).is_err());
    }

    #[test]
    fn reverse_v4() {
        let a = Ipv4Addr::new(1, 2, 3, 4);
        assert_eq!(reverse_ptr_v4(&a), "4.3.2.1.in-addr.arpa");
    }

    #[test]
    fn reverse_v6() {
        let a = Ipv6Addr::from_str("::1").unwrap();
        assert!(reverse_ptr_v6(&a).ends_with("ip6.arpa"));
    }
}
