//! IPv6 exploded / compressed formatting.

use std::net::Ipv6Addr;

/// Lowercase compressed form (RFC 5952).
pub fn ipv6_compressed(addr: &Ipv6Addr) -> String {
    addr.to_string()
}

/// Fully expanded eight-hextet form.
pub fn ipv6_exploded(addr: &Ipv6Addr) -> String {
    let segs = addr.segments();
    format!(
        "{:x}:{:x}:{:x}:{:x}:{:x}:{:x}:{:x}:{:x}",
        segs[0], segs[1], segs[2], segs[3], segs[4], segs[5], segs[6], segs[7]
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn exploded_loopback() {
        let a = Ipv6Addr::from_str("::1").unwrap();
        assert_eq!(ipv6_exploded(&a), "0:0:0:0:0:0:0:1");
    }
}
