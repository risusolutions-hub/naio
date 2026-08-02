//! DNS-SD ServiceInfo and naming helpers.

use crate::dns::{
    encode_a_rdata, encode_aaaa_rdata, encode_ptr_rdata, encode_srv_rdata, names_equal,
    normalize_name, DnsMessage, RecordType, ResourceRecord, CLASS_CACHE_FLUSH, CLASS_IN,
};
use crate::error::{MdnsError, MdnsResult};
use crate::txt::{pack_txt, unpack_txt};
use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};

/// Default RR TTL for announced services (seconds).
pub const DEFAULT_TTL: u32 = 120;

/// DNS-SD service description (Python `zeroconf.ServiceInfo` analog).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceInfo {
    /// Instance name (e.g. `My Printer`) — without type suffix.
    pub name: String,
    /// Service type (e.g. `_http._tcp.local.`).
    pub service_type: String,
    /// Port number.
    pub port: u16,
    /// Host / server FQDN (e.g. `hostname.local.`).
    pub host: String,
    /// IPv4 / IPv6 addresses.
    pub addresses: Vec<IpAddr>,
    /// TXT properties.
    pub properties: BTreeMap<String, String>,
    pub priority: u16,
    pub weight: u16,
    pub ttl: u32,
}

impl ServiceInfo {
    /// Build a new service; normalizes type and host.
    pub fn new(
        name: impl Into<String>,
        service_type: impl Into<String>,
        port: u16,
        host: Option<String>,
        addresses: Vec<IpAddr>,
        properties: BTreeMap<String, String>,
        priority: u16,
        weight: u16,
        ttl: u32,
    ) -> MdnsResult<Self> {
        let name = name.into();
        let service_type_raw = service_type.into();
        if name.trim().is_empty() {
            return Err(MdnsError::Invalid("service name must not be empty".into()));
        }
        if name.contains('.') {
            // Allow full name form: "Instance._type._tcp.local."
            let full = normalize_name(&name);
            let st = normalize_service_type(&service_type_raw)?;
            // If name already includes type, extract instance.
            let type_norm = normalize_name(&st);
            if full
                .to_ascii_lowercase()
                .ends_with(&type_norm.to_ascii_lowercase())
            {
                let inst_len = full.len().saturating_sub(type_norm.len());
                let inst = full[..inst_len].trim_end_matches('.').to_string();
                if inst.is_empty() {
                    return Err(MdnsError::Invalid("empty instance name".into()));
                }
                let host = host
                    .map(|h| normalize_name(&h))
                    .unwrap_or_else(default_host);
                return Ok(Self {
                    name: inst,
                    service_type: type_norm,
                    port,
                    host,
                    addresses,
                    properties,
                    priority,
                    weight,
                    ttl: if ttl == 0 { DEFAULT_TTL } else { ttl },
                });
            }
        }
        let service_type = normalize_service_type(&service_type_raw)?;
        let host = host
            .map(|h| normalize_name(&h))
            .unwrap_or_else(default_host);
        Ok(Self {
            name,
            service_type,
            port,
            host,
            addresses,
            properties,
            priority,
            weight,
            ttl: if ttl == 0 { DEFAULT_TTL } else { ttl },
        })
    }

    /// Fully-qualified service instance name (preserves instance label case).
    pub fn fullname(&self) -> String {
        let inst = self.name.trim_end_matches('.');
        let ty = self
            .service_type
            .trim_start_matches('.')
            .trim_end_matches('.');
        format!("{inst}.{ty}.")
    }

    pub fn set_property(
        &mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> MdnsResult<()> {
        let key = key.into();
        if key.is_empty() || key.contains('=') {
            return Err(MdnsError::Invalid(format!("invalid TXT key '{key}'")));
        }
        self.properties.insert(key, value.into());
        Ok(())
    }

    pub fn add_address(&mut self, ip: IpAddr) {
        if !self.addresses.contains(&ip) {
            self.addresses.push(ip);
        }
    }

    /// Resource records for announcement (PTR + SRV + TXT + A/AAAA).
    pub fn to_records(&self, goodbye: bool) -> MdnsResult<Vec<ResourceRecord>> {
        let ttl = if goodbye { 0 } else { self.ttl };
        let class = CLASS_IN | CLASS_CACHE_FLUSH;
        let full = self.fullname();
        let mut records = Vec::new();

        records.push(ResourceRecord {
            name: self.service_type.clone(),
            rtype: RecordType::Ptr,
            class: CLASS_IN, // PTR should not set cache-flush (RFC 6762)
            ttl,
            rdata: encode_ptr_rdata(&full)?,
        });

        records.push(ResourceRecord {
            name: full.clone(),
            rtype: RecordType::Srv,
            class,
            ttl,
            rdata: encode_srv_rdata(self.priority, self.weight, self.port, &self.host)?,
        });

        records.push(ResourceRecord {
            name: full,
            rtype: RecordType::Txt,
            class,
            ttl,
            rdata: pack_txt(&self.properties)?,
        });

        for addr in &self.addresses {
            match addr {
                IpAddr::V4(v4) => records.push(ResourceRecord {
                    name: self.host.clone(),
                    rtype: RecordType::A,
                    class,
                    ttl,
                    rdata: encode_a_rdata(*v4),
                }),
                IpAddr::V6(v6) => records.push(ResourceRecord {
                    name: self.host.clone(),
                    rtype: RecordType::Aaaa,
                    class,
                    ttl,
                    rdata: encode_aaaa_rdata(*v6),
                }),
            }
        }
        Ok(records)
    }

    /// Build a response message announcing this service.
    pub fn to_response_message(&self, goodbye: bool) -> MdnsResult<DnsMessage> {
        let records = self.to_records(goodbye)?;
        let (answers, additionals) = if records.is_empty() {
            (vec![], vec![])
        } else {
            let mut answers = vec![records[0].clone()];
            let additionals = records[1..].to_vec();
            // Put SRV/TXT in answers too for compatibility — PTR in answers, rest additional.
            let _ = &mut answers;
            (answers, additionals)
        };
        Ok(DnsMessage {
            id: 0,
            flags: 0x8400, // QR=1, AA=1
            questions: vec![],
            answers,
            authorities: vec![],
            additionals,
        })
    }
}

/// Normalize service type to `_service._proto.local.` form.
pub fn normalize_service_type(raw: &str) -> MdnsResult<String> {
    let mut s = raw.trim().to_string();
    if s.is_empty() {
        return Err(MdnsError::Invalid("service type must not be empty".into()));
    }
    s = s.trim_end_matches('.').to_string();
    // Ensure leading underscore pieces look like DNS-SD.
    let lower = s.to_ascii_lowercase();
    if !lower.contains("._") && !lower.starts_with('_') {
        return Err(MdnsError::Invalid(format!(
            "service type must look like '_http._tcp': '{raw}'"
        )));
    }
    if !lower.ends_with(".local") {
        s.push_str(".local");
    }
    Ok(normalize_name(&s))
}

/// True when `s` looks like a DNS-SD service type (`_x._y` …).
pub fn is_mdns_type(s: &str) -> bool {
    normalize_service_type(s).is_ok()
}

fn default_host() -> String {
    let name = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "localhost".into());
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    normalize_name(&format!("{safe}.local"))
}

/// Local host label used as default SRV target.
pub fn localhost_name() -> String {
    default_host()
}

/// Parse an IP string into [`IpAddr`].
pub fn parse_ip(s: &str) -> MdnsResult<IpAddr> {
    s.parse::<IpAddr>()
        .map_err(|e| MdnsError::Invalid(format!("invalid IP '{s}': {e}")))
}

/// Collect discovered service fields from a set of answer RRs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiscoveredService {
    pub name: String,
    pub service_type: String,
    pub server: String,
    pub port: u16,
    pub priority: u16,
    pub weight: u16,
    pub addresses: Vec<IpAddr>,
    pub properties: BTreeMap<String, String>,
    pub ttl: u32,
}

impl DiscoveredService {
    pub fn fullname(&self) -> String {
        if self.name.is_empty() {
            return String::new();
        }
        normalize_name(&format!(
            "{}.{}",
            self.name.trim_end_matches('.'),
            self.service_type.trim_start_matches('.')
        ))
    }
}

/// Merge PTR/SRV/TXT/A/AAAA records into discovered services keyed by full instance name.
pub fn services_from_message(
    msg: &DnsMessage,
    type_filter: Option<&str>,
) -> Vec<DiscoveredService> {
    let type_norm = type_filter.map(|t| normalize_name(t));
    let mut by_name: BTreeMap<String, DiscoveredService> = BTreeMap::new();

    let all: Vec<&ResourceRecord> = msg
        .answers
        .iter()
        .chain(msg.authorities.iter())
        .chain(msg.additionals.iter())
        .collect();

    // First pass: PTR → instance names
    for rr in &all {
        if rr.rtype != RecordType::Ptr {
            continue;
        }
        if let Some(ref tn) = type_norm {
            if !names_equal(&rr.name, tn) {
                continue;
            }
        }
        if let Ok(target) = crate::dns::decode_ptr_rdata(&rr.rdata) {
            let full = normalize_name(&target);
            let entry = by_name.entry(full.clone()).or_default();
            entry.service_type = normalize_name(&rr.name);
            entry.ttl = rr.ttl;
            // instance = full with type suffix stripped
            let ty = entry.service_type.clone();
            if full.len() > ty.len()
                && full
                    .to_ascii_lowercase()
                    .ends_with(&ty.to_ascii_lowercase())
            {
                entry.name = full[..full.len() - ty.len()]
                    .trim_end_matches('.')
                    .to_string();
            } else {
                entry.name = full.trim_end_matches('.').to_string();
            }
        }
    }

    // SRV / TXT keyed by instance full name
    for rr in &all {
        match rr.rtype {
            RecordType::Srv => {
                if let Ok((pri, w, port, target)) = crate::dns::decode_srv_rdata(&rr.rdata) {
                    let full = normalize_name(&rr.name);
                    let entry = by_name.entry(full.clone()).or_default();
                    if entry.name.is_empty() {
                        // Infer type as everything after first label
                        if let Some((_, rest)) = full.trim_end_matches('.').split_once('.') {
                            entry.service_type = normalize_name(rest);
                            entry.name = full
                                .trim_end_matches('.')
                                .split('.')
                                .next()
                                .unwrap_or("")
                                .to_string();
                        }
                    }
                    entry.priority = pri;
                    entry.weight = w;
                    entry.port = port;
                    entry.server = normalize_name(&target);
                    entry.ttl = rr.ttl;
                }
            }
            RecordType::Txt => {
                if let Ok(props) = unpack_txt(&rr.rdata) {
                    let full = normalize_name(&rr.name);
                    let entry = by_name.entry(full).or_default();
                    entry.properties = props;
                    entry.ttl = rr.ttl;
                }
            }
            _ => {}
        }
    }

    // A / AAAA attached via server host name
    for rr in &all {
        match rr.rtype {
            RecordType::A => {
                if let Ok(ip) = crate::dns::decode_a_rdata(&rr.rdata) {
                    let host = normalize_name(&rr.name);
                    for entry in by_name.values_mut() {
                        if names_equal(&entry.server, &host) || entry.server.is_empty() {
                            let a = IpAddr::V4(ip);
                            if !entry.addresses.contains(&a) {
                                entry.addresses.push(a);
                            }
                        }
                    }
                }
            }
            RecordType::Aaaa => {
                if let Ok(ip) = crate::dns::decode_aaaa_rdata(&rr.rdata) {
                    let host = normalize_name(&rr.name);
                    for entry in by_name.values_mut() {
                        if names_equal(&entry.server, &host) || entry.server.is_empty() {
                            let a = IpAddr::V6(ip);
                            if !entry.addresses.contains(&a) {
                                entry.addresses.push(a);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    by_name
        .into_values()
        .filter(|s| !s.name.is_empty())
        .collect()
}

/// Quick helpers exposing common addresses for tests / defaults.
pub fn loopback_v4() -> IpAddr {
    IpAddr::V4(Ipv4Addr::LOCALHOST)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_type() {
        assert_eq!(
            normalize_service_type("_http._tcp").unwrap(),
            "_http._tcp.local."
        );
        assert_eq!(
            normalize_service_type("_http._tcp.local.").unwrap(),
            "_http._tcp.local."
        );
    }

    #[test]
    fn fullname_and_records() {
        let mut props = BTreeMap::new();
        props.insert("path".into(), "/".into());
        let svc = ServiceInfo::new(
            "Demo",
            "_http._tcp",
            8080,
            Some("demo.local.".into()),
            vec![IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))],
            props,
            0,
            0,
            120,
        )
        .unwrap();
        assert_eq!(svc.fullname(), "Demo._http._tcp.local.");
        let recs = svc.to_records(false).unwrap();
        assert!(recs.len() >= 4);
        let msg = svc.to_response_message(false).unwrap();
        let wire = crate::dns::encode_message(&msg).unwrap();
        let back = crate::dns::decode_message(&wire).unwrap();
        let found = services_from_message(&back, Some("_http._tcp.local."));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].port, 8080);
        assert_eq!(
            found[0].properties.get("path").map(|s| s.as_str()),
            Some("/")
        );
    }
}
