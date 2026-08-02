//! DNS message encode / decode (RFC 1035 + mDNS/DNS-SD record types).

use crate::error::{MdnsError, MdnsResult};
use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};

/// Classic DNS / mDNS record type codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum RecordType {
    A = 1,
    Ptr = 12,
    Txt = 16,
    Aaaa = 28,
    Srv = 33,
    Any = 255,
    Other(u16),
}

impl RecordType {
    pub fn from_u16(n: u16) -> Self {
        match n {
            1 => Self::A,
            12 => Self::Ptr,
            16 => Self::Txt,
            28 => Self::Aaaa,
            33 => Self::Srv,
            255 => Self::Any,
            other => Self::Other(other),
        }
    }

    pub fn as_u16(self) -> u16 {
        match self {
            Self::A => 1,
            Self::Ptr => 12,
            Self::Txt => 16,
            Self::Aaaa => 28,
            Self::Srv => 33,
            Self::Any => 255,
            Self::Other(n) => n,
        }
    }

    pub fn name(self) -> String {
        match self {
            Self::A => "A".into(),
            Self::Ptr => "PTR".into(),
            Self::Txt => "TXT".into(),
            Self::Aaaa => "AAAA".into(),
            Self::Srv => "SRV".into(),
            Self::Any => "ANY".into(),
            Self::Other(n) => format!("TYPE{n}"),
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_uppercase().as_str() {
            "A" => Self::A,
            "PTR" => Self::Ptr,
            "TXT" => Self::Txt,
            "AAAA" => Self::Aaaa,
            "SRV" => Self::Srv,
            "ANY" | "*" => Self::Any,
            other => {
                if let Some(rest) = other.strip_prefix("TYPE") {
                    if let Ok(n) = rest.parse::<u16>() {
                        return Self::from_u16(n);
                    }
                }
                Self::Any
            }
        }
    }
}

/// DNS class (internet + mDNS cache-flush bit helpers).
pub const CLASS_IN: u16 = 1;
/// mDNS cache-flush bit in the class field (RFC 6762).
pub const CLASS_CACHE_FLUSH: u16 = 0x8000;
/// mDNS unicast-response bit in the qclass field.
pub const QCLASS_UNICAST: u16 = 0x8000;

/// One resource record (answer / authority / additional).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceRecord {
    pub name: String,
    pub rtype: RecordType,
    pub class: u16,
    pub ttl: u32,
    pub rdata: Vec<u8>,
}

impl ResourceRecord {
    pub fn cache_flush(&self) -> bool {
        (self.class & CLASS_CACHE_FLUSH) != 0
    }

    pub fn class_base(&self) -> u16 {
        self.class & !CLASS_CACHE_FLUSH
    }
}

/// One question in a DNS query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub name: String,
    pub qtype: RecordType,
    pub qclass: u16,
}

/// Parsed DNS / mDNS message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsMessage {
    pub id: u16,
    pub flags: u16,
    pub questions: Vec<Question>,
    pub answers: Vec<ResourceRecord>,
    pub authorities: Vec<ResourceRecord>,
    pub additionals: Vec<ResourceRecord>,
}

impl DnsMessage {
    pub fn is_response(&self) -> bool {
        (self.flags & 0x8000) != 0
    }

    pub fn is_query(&self) -> bool {
        !self.is_response()
    }
}

/// Split an FQDN into labels (trailing empty label for root is dropped).
pub fn split_labels(name: &str) -> MdnsResult<Vec<String>> {
    let trimmed = name.trim().trim_end_matches('.');
    if trimmed.is_empty() {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    for part in trimmed.split('.') {
        if part.is_empty() {
            return Err(MdnsError::Invalid(format!("empty DNS label in '{name}'")));
        }
        if part.len() > 63 {
            return Err(MdnsError::Invalid(format!(
                "DNS label longer than 63 bytes: '{part}'"
            )));
        }
        if part.as_bytes().iter().any(|&b| b > 0x7f) {
            // Allow UTF-8 for service instance names (RFC 6763) via raw length check below.
        }
        if part.len() > 63 {
            return Err(MdnsError::Invalid("label too long".into()));
        }
        out.push(part.to_string());
    }
    let total: usize = out.iter().map(|l| l.len() + 1).sum::<usize>() + 1;
    if total > 255 {
        return Err(MdnsError::Invalid(format!(
            "DNS name too long ({total} bytes): '{name}'"
        )));
    }
    Ok(out)
}

/// Join labels into a dotted name ending with `.`.
pub fn join_labels(labels: &[String]) -> String {
    if labels.is_empty() {
        return ".".into();
    }
    let mut s = labels.join(".");
    s.push('.');
    s
}

/// Normalize a DNS name: lowercase ASCII labels, ensure trailing dot.
pub fn normalize_name(name: &str) -> String {
    let t = name.trim();
    if t.is_empty() || t == "." {
        return ".".into();
    }
    let mut s = t.trim_end_matches('.').to_ascii_lowercase();
    s.push('.');
    s
}

/// Case-insensitive DNS name equality (ASCII).
pub fn names_equal(a: &str, b: &str) -> bool {
    normalize_name(a) == normalize_name(b)
}

fn write_u16(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_be_bytes());
}

fn write_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_be_bytes());
}

fn read_u16(data: &[u8], offset: &mut usize) -> MdnsResult<u16> {
    if *offset + 2 > data.len() {
        return Err(MdnsError::Decode("truncated u16".into()));
    }
    let v = u16::from_be_bytes([data[*offset], data[*offset + 1]]);
    *offset += 2;
    Ok(v)
}

fn read_u32(data: &[u8], offset: &mut usize) -> MdnsResult<u32> {
    if *offset + 4 > data.len() {
        return Err(MdnsError::Decode("truncated u32".into()));
    }
    let v = u32::from_be_bytes([
        data[*offset],
        data[*offset + 1],
        data[*offset + 2],
        data[*offset + 3],
    ]);
    *offset += 4;
    Ok(v)
}

/// Decode a DNS name starting at `offset`, advancing `offset` past the name on the wire.
pub fn decode_name(data: &[u8], offset: &mut usize) -> MdnsResult<String> {
    let mut labels = Vec::new();
    let mut jumped = false;
    let mut jumps = 0usize;
    let mut pos = *offset;
    let start = *offset;

    loop {
        if pos >= data.len() {
            return Err(MdnsError::Decode("truncated name".into()));
        }
        let len = data[pos];
        if len == 0 {
            pos += 1;
            if !jumped {
                *offset = pos;
            }
            break;
        }
        // Compression pointer
        if len & 0xC0 == 0xC0 {
            if pos + 1 >= data.len() {
                return Err(MdnsError::Decode("truncated compression pointer".into()));
            }
            let ptr = (((len as u16) & 0x3F) << 8) | (data[pos + 1] as u16);
            if (ptr as usize) >= data.len() || (ptr as usize) >= start && jumps == 0 {
                // pointer into current partial name is ok for message-global; reject out of bounds
            }
            if (ptr as usize) >= data.len() {
                return Err(MdnsError::Decode(format!(
                    "compression pointer out of range: {ptr}"
                )));
            }
            if !jumped {
                *offset = pos + 2;
                jumped = true;
            }
            pos = ptr as usize;
            jumps += 1;
            if jumps > 32 {
                return Err(MdnsError::Decode("compression pointer loop".into()));
            }
            continue;
        }
        if len & 0xC0 != 0 {
            return Err(MdnsError::Decode(format!(
                "invalid label length byte {len}"
            )));
        }
        pos += 1;
        let end = pos + len as usize;
        if end > data.len() {
            return Err(MdnsError::Decode("truncated label".into()));
        }
        let label = match std::str::from_utf8(&data[pos..end]) {
            Ok(s) => s.to_string(),
            Err(_) => {
                // Keep lossy for binary labels but mark as invalid for APIs that need UTF-8.
                return Err(MdnsError::Decode(format!(
                    "non-UTF-8 DNS label at offset {pos}"
                )));
            }
        };
        if label.len() > 63 {
            return Err(MdnsError::Decode("label too long".into()));
        }
        labels.push(label);
        pos = end;
        if !jumped {
            *offset = pos;
        }
    }
    Ok(join_labels(&labels))
}

fn encode_name_labels(buf: &mut Vec<u8>, labels: &[String], compress: &mut HashMap<String, u16>) {
    // For simplicity and correctness under streaming builds, we compress when exact suffix
    // already appeared. Build remaining suffix at each step.
    for i in 0..labels.len() {
        let suffix = join_labels(&labels[i..]);
        if let Some(&ptr) = compress.get(&suffix.to_ascii_lowercase()) {
            write_u16(buf, 0xC000 | ptr);
            return;
        }
        let pos = buf.len() as u16;
        compress.insert(suffix.to_ascii_lowercase(), pos);
        let lab = labels[i].as_bytes();
        buf.push(lab.len() as u8);
        buf.extend_from_slice(lab);
    }
    buf.push(0);
}

/// Encode a DNS name into `buf` with optional compression table.
pub fn encode_name(
    buf: &mut Vec<u8>,
    name: &str,
    compress: &mut HashMap<String, u16>,
) -> MdnsResult<()> {
    let labels = split_labels(name)?;
    encode_name_labels(buf, &labels, compress);
    Ok(())
}

fn encode_name_nocompress(buf: &mut Vec<u8>, name: &str) -> MdnsResult<()> {
    let labels = split_labels(name)?;
    for lab in &labels {
        let b = lab.as_bytes();
        if b.len() > 63 {
            return Err(MdnsError::Encode(format!("label too long: {lab}")));
        }
        buf.push(b.len() as u8);
        buf.extend_from_slice(b);
    }
    buf.push(0);
    Ok(())
}

fn decode_rr(data: &[u8], offset: &mut usize) -> MdnsResult<ResourceRecord> {
    let name = decode_name(data, offset)?;
    let rtype = RecordType::from_u16(read_u16(data, offset)?);
    let class = read_u16(data, offset)?;
    let ttl = read_u32(data, offset)?;
    let rdlen = read_u16(data, offset)? as usize;
    if *offset + rdlen > data.len() {
        return Err(MdnsError::Decode("truncated rdata".into()));
    }
    let rdata = data[*offset..*offset + rdlen].to_vec();
    *offset += rdlen;
    Ok(ResourceRecord {
        name,
        rtype,
        class,
        ttl,
        rdata,
    })
}

fn encode_rr(
    buf: &mut Vec<u8>,
    rr: &ResourceRecord,
    compress: &mut HashMap<String, u16>,
) -> MdnsResult<()> {
    encode_name(buf, &rr.name, compress)?;
    write_u16(buf, rr.rtype.as_u16());
    write_u16(buf, rr.class);
    write_u32(buf, rr.ttl);
    if rr.rdata.len() > u16::MAX as usize {
        return Err(MdnsError::Encode("rdata too large".into()));
    }
    write_u16(buf, rr.rdata.len() as u16);
    buf.extend_from_slice(&rr.rdata);
    Ok(())
}

/// Decode a full DNS message from wire bytes.
pub fn decode_message(data: &[u8]) -> MdnsResult<DnsMessage> {
    if data.len() < 12 {
        return Err(MdnsError::Decode(format!(
            "message too short ({} bytes)",
            data.len()
        )));
    }
    let mut offset = 0usize;
    let id = read_u16(data, &mut offset)?;
    let flags = read_u16(data, &mut offset)?;
    let qdcount = read_u16(data, &mut offset)? as usize;
    let ancount = read_u16(data, &mut offset)? as usize;
    let nscount = read_u16(data, &mut offset)? as usize;
    let arcount = read_u16(data, &mut offset)? as usize;

    let mut questions = Vec::with_capacity(qdcount);
    for _ in 0..qdcount {
        let name = decode_name(data, &mut offset)?;
        let qtype = RecordType::from_u16(read_u16(data, &mut offset)?);
        let qclass = read_u16(data, &mut offset)?;
        questions.push(Question {
            name,
            qtype,
            qclass,
        });
    }

    let mut answers = Vec::with_capacity(ancount);
    for _ in 0..ancount {
        answers.push(decode_rr(data, &mut offset)?);
    }
    let mut authorities = Vec::with_capacity(nscount);
    for _ in 0..nscount {
        authorities.push(decode_rr(data, &mut offset)?);
    }
    let mut additionals = Vec::with_capacity(arcount);
    for _ in 0..arcount {
        additionals.push(decode_rr(data, &mut offset)?);
    }

    Ok(DnsMessage {
        id,
        flags,
        questions,
        answers,
        authorities,
        additionals,
    })
}

/// Encode a DNS message to wire bytes (with name compression).
pub fn encode_message(msg: &DnsMessage) -> MdnsResult<Vec<u8>> {
    let mut buf = Vec::with_capacity(64);
    let mut compress = HashMap::new();
    write_u16(&mut buf, msg.id);
    write_u16(&mut buf, msg.flags);
    write_u16(&mut buf, msg.questions.len() as u16);
    write_u16(&mut buf, msg.answers.len() as u16);
    write_u16(&mut buf, msg.authorities.len() as u16);
    write_u16(&mut buf, msg.additionals.len() as u16);

    for q in &msg.questions {
        encode_name(&mut buf, &q.name, &mut compress)?;
        write_u16(&mut buf, q.qtype.as_u16());
        write_u16(&mut buf, q.qclass);
    }
    for rr in &msg.answers {
        encode_rr(&mut buf, rr, &mut compress)?;
    }
    for rr in &msg.authorities {
        encode_rr(&mut buf, rr, &mut compress)?;
    }
    for rr in &msg.additionals {
        encode_rr(&mut buf, rr, &mut compress)?;
    }
    Ok(buf)
}

/// Build a standard mDNS query (QR=0, opcode=0) for `qname` / `qtype`.
pub fn build_query(qname: &str, qtype: RecordType) -> MdnsResult<Vec<u8>> {
    let msg = DnsMessage {
        id: 0,
        flags: 0,
        questions: vec![Question {
            name: normalize_name(qname),
            qtype,
            qclass: CLASS_IN,
        }],
        answers: vec![],
        authorities: vec![],
        additionals: vec![],
    };
    encode_message(&msg)
}

/// Encode SRV rdata (priority, weight, port, target).
pub fn encode_srv_rdata(
    priority: u16,
    weight: u16,
    port: u16,
    target: &str,
) -> MdnsResult<Vec<u8>> {
    let mut buf = Vec::new();
    write_u16(&mut buf, priority);
    write_u16(&mut buf, weight);
    write_u16(&mut buf, port);
    encode_name_nocompress(&mut buf, target)?;
    Ok(buf)
}

/// Decode SRV rdata.
pub fn decode_srv_rdata(data: &[u8]) -> MdnsResult<(u16, u16, u16, String)> {
    if data.len() < 7 {
        return Err(MdnsError::Decode("SRV rdata too short".into()));
    }
    let priority = u16::from_be_bytes([data[0], data[1]]);
    let weight = u16::from_be_bytes([data[2], data[3]]);
    let port = u16::from_be_bytes([data[4], data[5]]);
    let mut offset = 6;
    let target = decode_name(data, &mut offset)?;
    Ok((priority, weight, port, target))
}

/// Encode PTR rdata (a domain name).
pub fn encode_ptr_rdata(target: &str) -> MdnsResult<Vec<u8>> {
    let mut buf = Vec::new();
    encode_name_nocompress(&mut buf, target)?;
    Ok(buf)
}

/// Decode PTR rdata.
pub fn decode_ptr_rdata(data: &[u8]) -> MdnsResult<String> {
    let mut offset = 0;
    decode_name(data, &mut offset)
}

/// Encode an A record rdata.
pub fn encode_a_rdata(ip: Ipv4Addr) -> Vec<u8> {
    ip.octets().to_vec()
}

/// Decode A record rdata.
pub fn decode_a_rdata(data: &[u8]) -> MdnsResult<Ipv4Addr> {
    if data.len() != 4 {
        return Err(MdnsError::Decode(format!(
            "A rdata must be 4 bytes, got {}",
            data.len()
        )));
    }
    Ok(Ipv4Addr::new(data[0], data[1], data[2], data[3]))
}

/// Encode AAAA rdata.
pub fn encode_aaaa_rdata(ip: Ipv6Addr) -> Vec<u8> {
    ip.octets().to_vec()
}

/// Decode AAAA rdata.
pub fn decode_aaaa_rdata(data: &[u8]) -> MdnsResult<Ipv6Addr> {
    if data.len() != 16 {
        return Err(MdnsError::Decode(format!(
            "AAAA rdata must be 16 bytes, got {}",
            data.len()
        )));
    }
    let mut o = [0u8; 16];
    o.copy_from_slice(data);
    Ok(Ipv6Addr::from(o))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_roundtrip() {
        let wire = build_query("_http._tcp.local.", RecordType::Ptr).unwrap();
        let msg = decode_message(&wire).unwrap();
        assert!(msg.is_query());
        assert_eq!(msg.questions.len(), 1);
        assert!(names_equal(&msg.questions[0].name, "_http._tcp.local."));
        assert_eq!(msg.questions[0].qtype, RecordType::Ptr);
    }

    #[test]
    fn name_compression_roundtrip() {
        let mut msg = DnsMessage {
            id: 1,
            flags: 0x8400,
            questions: vec![],
            answers: vec![ResourceRecord {
                name: "_http._tcp.local.".into(),
                rtype: RecordType::Ptr,
                class: CLASS_IN | CLASS_CACHE_FLUSH,
                ttl: 120,
                rdata: encode_ptr_rdata("My Printer._http._tcp.local.").unwrap(),
            }],
            authorities: vec![],
            additionals: vec![ResourceRecord {
                name: "My Printer._http._tcp.local.".into(),
                rtype: RecordType::Srv,
                class: CLASS_IN | CLASS_CACHE_FLUSH,
                ttl: 120,
                rdata: encode_srv_rdata(0, 0, 80, "printer.local.").unwrap(),
            }],
        };
        // also A
        msg.additionals.push(ResourceRecord {
            name: "printer.local.".into(),
            rtype: RecordType::A,
            class: CLASS_IN | CLASS_CACHE_FLUSH,
            ttl: 120,
            rdata: encode_a_rdata(Ipv4Addr::new(192, 168, 1, 10)),
        });
        let wire = encode_message(&msg).unwrap();
        let back = decode_message(&wire).unwrap();
        assert_eq!(back.answers.len(), 1);
        assert_eq!(back.additionals.len(), 2);
        let ptr = decode_ptr_rdata(&back.answers[0].rdata).unwrap();
        assert!(names_equal(&ptr, "My Printer._http._tcp.local."));
        let (pri, w, port, target) = decode_srv_rdata(&back.additionals[0].rdata).unwrap();
        assert_eq!((pri, w, port), (0, 0, 80));
        assert!(names_equal(&target, "printer.local."));
        assert_eq!(
            decode_a_rdata(&back.additionals[1].rdata).unwrap(),
            Ipv4Addr::new(192, 168, 1, 10)
        );
    }

    #[test]
    fn reject_truncated() {
        assert!(decode_message(&[0u8; 5]).is_err());
    }

    #[test]
    fn reject_empty_label() {
        assert!(split_labels("a..b.").is_err());
    }

    /// Conformance: RFC 6762-style PTR query wire layout (header + one question).
    #[test]
    fn conformance_mdns_ptr_query_header() {
        let wire = build_query("_services._dns-sd._udp.local.", RecordType::Ptr).unwrap();
        assert_eq!(wire[0], 0);
        assert_eq!(wire[1], 0); // id
        assert_eq!(wire[2], 0);
        assert_eq!(wire[3], 0); // flags query
        assert_eq!(u16::from_be_bytes([wire[4], wire[5]]), 1); // qdcount
        let msg = decode_message(&wire).unwrap();
        assert!(msg.is_query());
        assert_eq!(msg.questions.len(), 1);
        assert_eq!(msg.questions[0].qtype, RecordType::Ptr);
        assert!(names_equal(
            &msg.questions[0].name,
            "_services._dns-sd._udp.local."
        ));
    }

    /// Conformance: decode a fixed PTR answer rdata (domain name only).
    #[test]
    fn conformance_ptr_rdata() {
        let target = encode_ptr_rdata("My Host._http._tcp.local.").unwrap();
        let name = decode_ptr_rdata(&target).unwrap();
        assert!(names_equal(&name, "My Host._http._tcp.local."));
    }
}
