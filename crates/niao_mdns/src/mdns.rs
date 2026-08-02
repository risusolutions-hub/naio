//! Multicast mDNS socket: browse, resolve, register, unregister.

use crate::dns::{
    build_query, decode_message, encode_message, names_equal, normalize_name, RecordType,
};
use crate::error::{MdnsError, MdnsResult};
use crate::service::{services_from_message, DiscoveredService, ServiceInfo};
use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

/// IPv4 mDNS multicast group.
pub const MDNS_GROUP_V4: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
/// mDNS UDP port.
pub const MDNS_PORT: u16 = 5353;

/// Active mDNS client / announcer.
pub struct MdnsClient {
    sock: UdpSocket,
    /// Bound to the classic mDNS port (better for announcements).
    pub bound_mdns_port: bool,
}

impl MdnsClient {
    /// Open a UDP socket suitable for mDNS queries and announcements.
    /// Prefers binding `0.0.0.0:5353` with reuse; falls back to an ephemeral port.
    pub fn open() -> MdnsResult<Self> {
        let bound_mdns_port = match try_bind_mdns() {
            Ok(sock) => {
                let _ = join_multicast(&sock);
                return Ok(Self {
                    sock,
                    bound_mdns_port: true,
                });
            }
            Err(_) => false,
        };
        let sock = UdpSocket::bind("0.0.0.0:0")?;
        sock.set_multicast_ttl_v4(1)?;
        let _ = join_multicast(&sock);
        Ok(Self {
            sock,
            bound_mdns_port,
        })
    }

    /// Send a UDP packet to the mDNS group.
    pub fn send_multicast(&self, data: &[u8]) -> MdnsResult<()> {
        let dest = SocketAddr::from((MDNS_GROUP_V4, MDNS_PORT));
        self.sock.send_to(data, dest)?;
        Ok(())
    }

    /// Browse for services of `service_type` for up to `timeout`.
    pub fn browse(
        &self,
        service_type: &str,
        timeout: Duration,
    ) -> MdnsResult<Vec<DiscoveredService>> {
        let ty = crate::service::normalize_service_type(service_type)?;
        let query = build_query(&ty, RecordType::Ptr)?;
        self.send_multicast(&query)?;
        // Second query shortly after (mDNS often sends duplicates).
        let _ = self.send_multicast(&query);

        let mut by_key: BTreeMap<String, DiscoveredService> = BTreeMap::new();
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            self.sock.set_read_timeout(Some(remaining))?;
            let mut buf = [0u8; 9000];
            match self.sock.recv_from(&mut buf) {
                Ok((n, _addr)) => {
                    if let Ok(msg) = decode_message(&buf[..n]) {
                        for svc in services_from_message(&msg, Some(&ty)) {
                            let key = svc.fullname().to_ascii_lowercase();
                            by_key
                                .entry(key)
                                .and_modify(|e| merge_service(e, &svc))
                                .or_insert(svc);
                        }
                    }
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    break;
                }
                Err(e) => return Err(MdnsError::Io(e.to_string())),
            }
        }
        Ok(by_key.into_values().collect())
    }

    /// Resolve one instance (`name` + `service_type`) within `timeout`.
    pub fn resolve(
        &self,
        name: &str,
        service_type: &str,
        timeout: Duration,
    ) -> MdnsResult<Option<DiscoveredService>> {
        let ty = crate::service::normalize_service_type(service_type)?;
        let full = if name.contains("._") {
            normalize_name(name)
        } else {
            normalize_name(&format!(
                "{}.{}",
                name.trim_end_matches('.'),
                ty.trim_start_matches('.')
            ))
        };
        // Query SRV (and ANY) for the instance + PTR for the type.
        let q1 = build_query(&full, RecordType::Any)?;
        let q2 = build_query(&ty, RecordType::Ptr)?;
        self.send_multicast(&q1)?;
        self.send_multicast(&q2)?;

        let mut found: Option<DiscoveredService> = None;
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            self.sock.set_read_timeout(Some(remaining))?;
            let mut buf = [0u8; 9000];
            match self.sock.recv_from(&mut buf) {
                Ok((n, _)) => {
                    if let Ok(msg) = decode_message(&buf[..n]) {
                        for svc in services_from_message(&msg, Some(&ty)) {
                            if names_equal(&svc.fullname(), &full) || names_equal(&svc.name, name) {
                                found = Some(match found.take() {
                                    Some(mut cur) => {
                                        merge_service(&mut cur, &svc);
                                        cur
                                    }
                                    None => svc,
                                });
                            }
                        }
                    }
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    break;
                }
                Err(e) => return Err(MdnsError::Io(e.to_string())),
            }
        }
        Ok(found)
    }

    /// Announce / register a service (multicast response).
    pub fn register(&self, service: &ServiceInfo) -> MdnsResult<()> {
        let msg = service.to_response_message(false)?;
        let wire = encode_message(&msg)?;
        self.send_multicast(&wire)?;
        // Probe/announce twice for reliability.
        self.send_multicast(&wire)?;
        Ok(())
    }

    /// Send goodbye records (TTL=0) to unregister.
    pub fn unregister(&self, service: &ServiceInfo) -> MdnsResult<()> {
        let msg = service.to_response_message(true)?;
        let wire = encode_message(&msg)?;
        self.send_multicast(&wire)?;
        Ok(())
    }

    /// Re-announce an updated service.
    pub fn update(&self, service: &ServiceInfo) -> MdnsResult<()> {
        self.register(service)
    }
}

fn try_bind_mdns() -> std::io::Result<UdpSocket> {
    // Prefer the classic mDNS port so announcements are well-formed; fall back in open().
    let sock = UdpSocket::bind(("0.0.0.0", MDNS_PORT))?;
    sock.set_multicast_ttl_v4(1)?;
    Ok(sock)
}

fn join_multicast(sock: &UdpSocket) -> std::io::Result<()> {
    sock.join_multicast_v4(&MDNS_GROUP_V4, &Ipv4Addr::UNSPECIFIED)?;
    Ok(())
}

fn merge_service(dst: &mut DiscoveredService, src: &DiscoveredService) {
    if dst.port == 0 {
        dst.port = src.port;
    }
    if dst.server.is_empty() {
        dst.server = src.server.clone();
    }
    if dst.service_type.is_empty() {
        dst.service_type = src.service_type.clone();
    }
    if dst.name.is_empty() {
        dst.name = src.name.clone();
    }
    for a in &src.addresses {
        if !dst.addresses.contains(a) {
            dst.addresses.push(*a);
        }
    }
    for (k, v) in &src.properties {
        dst.properties.entry(k.clone()).or_insert_with(|| v.clone());
    }
    if src.ttl > 0 {
        dst.ttl = src.ttl;
    }
    dst.priority = src.priority;
    dst.weight = src.weight;
}

/// Self-discovery loopback helper used by integration-style tests.
/// Registers `service`, browses for its type, returns matches (may be empty if multicast blocked).
pub fn announce_and_browse(
    service: &ServiceInfo,
    timeout: Duration,
) -> MdnsResult<Vec<DiscoveredService>> {
    let client = MdnsClient::open()?;
    client.register(service)?;
    let found = client.browse(&service.service_type, timeout)?;
    Ok(found
        .into_iter()
        .filter(|s| names_equal(&s.fullname(), &service.fullname()) || s.name == service.name)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;

    #[test]
    fn open_client() {
        let c = MdnsClient::open().expect("open mdns");
        // Just ensure send works (may be dropped by OS; should not error).
        let q = build_query("_nmdns-test._tcp.local.", RecordType::Ptr).unwrap();
        c.send_multicast(&q).unwrap();
    }

    #[test]
    fn self_announce_best_effort() {
        let mut props = BTreeMap::new();
        props.insert("test".into(), "1".into());
        let svc = ServiceInfo::new(
            "NmdnsSelfTest",
            "_nmdns-test._tcp",
            19090,
            Some("nmdns-test.local.".into()),
            vec![IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))],
            props,
            0,
            0,
            120,
        )
        .unwrap();
        // Short timeout; environment may block multicast — do not assert non-empty.
        let _ = announce_and_browse(&svc, Duration::from_millis(200));
    }
}
