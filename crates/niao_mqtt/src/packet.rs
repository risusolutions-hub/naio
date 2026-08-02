//! MQTT 3.1.1 / 5.0 packet codec (encode + decode).

use crate::error::{MqttError, MqttResult};

pub const PROTO_MQTT311: u8 = 4;
pub const PROTO_MQTT5: u8 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PacketType {
    Connect = 1,
    Connack = 2,
    Publish = 3,
    Puback = 4,
    Pubrec = 5,
    Pubrel = 6,
    Pubcomp = 7,
    Subscribe = 8,
    Suback = 9,
    Unsubscribe = 10,
    Unsuback = 11,
    Pingreq = 12,
    Pingresp = 13,
    Disconnect = 14,
}

impl PacketType {
    pub fn from_u8(v: u8) -> MqttResult<Self> {
        match v {
            1 => Ok(Self::Connect),
            2 => Ok(Self::Connack),
            3 => Ok(Self::Publish),
            4 => Ok(Self::Puback),
            5 => Ok(Self::Pubrec),
            6 => Ok(Self::Pubrel),
            7 => Ok(Self::Pubcomp),
            8 => Ok(Self::Subscribe),
            9 => Ok(Self::Suback),
            10 => Ok(Self::Unsubscribe),
            11 => Ok(Self::Unsuback),
            12 => Ok(Self::Pingreq),
            13 => Ok(Self::Pingresp),
            14 => Ok(Self::Disconnect),
            _ => Err(MqttError::Protocol(format!("unknown packet type {v}"))),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Connect => "CONNECT",
            Self::Connack => "CONNACK",
            Self::Publish => "PUBLISH",
            Self::Puback => "PUBACK",
            Self::Pubrec => "PUBREC",
            Self::Pubrel => "PUBREL",
            Self::Pubcomp => "PUBCOMP",
            Self::Subscribe => "SUBSCRIBE",
            Self::Suback => "SUBACK",
            Self::Unsubscribe => "UNSUBSCRIBE",
            Self::Unsuback => "UNSUBACK",
            Self::Pingreq => "PINGREQ",
            Self::Pingresp => "PINGRESP",
            Self::Disconnect => "DISCONNECT",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Will {
    pub topic: String,
    pub payload: Vec<u8>,
    pub qos: u8,
    pub retain: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectOptions {
    pub client_id: String,
    pub protocol_level: u8,
    pub clean_session: bool,
    pub keepalive: u16,
    pub username: Option<String>,
    pub password: Option<Vec<u8>>,
    pub will: Option<Will>,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            protocol_level: PROTO_MQTT311,
            clean_session: true,
            keepalive: 60,
            username: None,
            password: None,
            will: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishPacket {
    pub topic: String,
    pub payload: Vec<u8>,
    pub qos: u8,
    pub retain: bool,
    pub dup: bool,
    pub packet_id: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    Connect(ConnectOptions),
    Connack {
        session_present: bool,
        return_code: u8,
    },
    Publish(PublishPacket),
    Puback(u16),
    Pubrec(u16),
    Pubrel(u16),
    Pubcomp(u16),
    Subscribe {
        packet_id: u16,
        filters: Vec<(String, u8)>,
    },
    Suback {
        packet_id: u16,
        codes: Vec<u8>,
    },
    Unsubscribe {
        packet_id: u16,
        filters: Vec<String>,
    },
    Unsuback(u16),
    Pingreq,
    Pingresp,
    Disconnect,
}

fn write_u16(buf: &mut Vec<u8>, v: u16) {
    buf.push((v >> 8) as u8);
    buf.push((v & 0xff) as u8);
}

fn write_bytes(buf: &mut Vec<u8>, data: &[u8]) {
    write_u16(buf, data.len() as u16);
    buf.extend_from_slice(data);
}

fn write_str(buf: &mut Vec<u8>, s: &str) {
    write_bytes(buf, s.as_bytes());
}

fn write_remaining_length(buf: &mut Vec<u8>, mut len: usize) {
    loop {
        let mut byte = (len % 128) as u8;
        len /= 128;
        if len > 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if len == 0 {
            break;
        }
    }
}

fn read_u16(data: &[u8], off: &mut usize) -> MqttResult<u16> {
    if *off + 2 > data.len() {
        return Err(MqttError::Protocol("truncated u16".into()));
    }
    let v = ((data[*off] as u16) << 8) | data[*off + 1] as u16;
    *off += 2;
    Ok(v)
}

fn read_bytes<'a>(data: &'a [u8], off: &mut usize) -> MqttResult<&'a [u8]> {
    let len = read_u16(data, off)? as usize;
    if *off + len > data.len() {
        return Err(MqttError::Protocol("truncated bytes".into()));
    }
    let slice = &data[*off..*off + len];
    *off += len;
    Ok(slice)
}

fn read_str(data: &[u8], off: &mut usize) -> MqttResult<String> {
    let bytes = read_bytes(data, off)?;
    String::from_utf8(bytes.to_vec())
        .map_err(|_| MqttError::Protocol("invalid utf-8 string".into()))
}

fn parse_remaining_length(data: &[u8], off: &mut usize) -> MqttResult<usize> {
    let mut multiplier = 1usize;
    let mut value = 0usize;
    for _ in 0..4 {
        if *off >= data.len() {
            return Err(MqttError::Protocol("truncated remaining length".into()));
        }
        let byte = data[*off];
        *off += 1;
        value += (byte & 0x7f) as usize * multiplier;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
        multiplier *= 128;
        if multiplier > 128 * 128 * 128 {
            return Err(MqttError::Protocol("remaining length overflow".into()));
        }
    }
    Err(MqttError::Protocol("remaining length too long".into()))
}

/// Encode CONNECT packet body + fixed header.
pub fn encode_connect(opts: &ConnectOptions) -> MqttResult<Vec<u8>> {
    if opts.protocol_level != PROTO_MQTT311 && opts.protocol_level != PROTO_MQTT5 {
        return Err(MqttError::InvalidArgument(format!(
            "unsupported protocol level {}",
            opts.protocol_level
        )));
    }
    if let Some(will) = &opts.will {
        validate_topic_name(&will.topic)?;
        if will.qos > 2 {
            return Err(MqttError::InvalidArgument("will qos must be 0..=2".into()));
        }
    }

    let mut body = Vec::with_capacity(64);
    write_str(&mut body, "MQTT");
    body.push(opts.protocol_level);

    let mut flags: u8 = 0;
    if opts.clean_session {
        flags |= 0x02;
    }
    if let Some(will) = &opts.will {
        flags |= 0x04;
        flags |= (will.qos & 0x03) << 3;
        if will.retain {
            flags |= 0x20;
        }
    }
    if opts.password.is_some() {
        flags |= 0x40;
    }
    if opts.username.is_some() {
        flags |= 0x80;
    }
    body.push(flags);
    write_u16(&mut body, opts.keepalive);

    if opts.protocol_level == PROTO_MQTT5 {
        // properties length = 0
        body.push(0);
    }

    write_str(&mut body, &opts.client_id);
    if let Some(will) = &opts.will {
        if opts.protocol_level == PROTO_MQTT5 {
            body.push(0); // will properties
        }
        write_str(&mut body, &will.topic);
        write_bytes(&mut body, &will.payload);
    }
    if let Some(user) = &opts.username {
        write_str(&mut body, user);
    }
    if let Some(pass) = &opts.password {
        write_bytes(&mut body, pass);
    }

    let mut out = Vec::with_capacity(body.len() + 5);
    out.push((PacketType::Connect as u8) << 4);
    write_remaining_length(&mut out, body.len());
    out.extend_from_slice(&body);
    Ok(out)
}

/// Encode a PUBLISH packet.
pub fn encode_publish(pubpkt: &PublishPacket) -> MqttResult<Vec<u8>> {
    validate_topic_name(&pubpkt.topic)?;
    if pubpkt.qos > 2 {
        return Err(MqttError::InvalidArgument("qos must be 0..=2".into()));
    }
    if pubpkt.qos > 0 && pubpkt.packet_id.is_none() {
        return Err(MqttError::InvalidArgument(
            "packet_id required for qos > 0".into(),
        ));
    }

    let mut body = Vec::with_capacity(pubpkt.topic.len() + pubpkt.payload.len() + 8);
    write_str(&mut body, &pubpkt.topic);
    if pubpkt.qos > 0 {
        write_u16(&mut body, pubpkt.packet_id.unwrap());
    }
    body.extend_from_slice(&pubpkt.payload);

    let mut flags = pubpkt.qos << 1;
    if pubpkt.retain {
        flags |= 0x01;
    }
    if pubpkt.dup {
        flags |= 0x08;
    }

    let mut out = Vec::with_capacity(body.len() + 5);
    out.push(((PacketType::Publish as u8) << 4) | flags);
    write_remaining_length(&mut out, body.len());
    out.extend_from_slice(&body);
    Ok(out)
}

fn encode_id_packet(ptype: PacketType, flags: u8, packet_id: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(4);
    out.push(((ptype as u8) << 4) | flags);
    write_remaining_length(&mut out, 2);
    write_u16(&mut out, packet_id);
    out
}

pub fn encode_puback(id: u16) -> Vec<u8> {
    encode_id_packet(PacketType::Puback, 0, id)
}

pub fn encode_pubrec(id: u16) -> Vec<u8> {
    encode_id_packet(PacketType::Pubrec, 0, id)
}

pub fn encode_pubrel(id: u16) -> Vec<u8> {
    encode_id_packet(PacketType::Pubrel, 0x02, id)
}

pub fn encode_pubcomp(id: u16) -> Vec<u8> {
    encode_id_packet(PacketType::Pubcomp, 0, id)
}

pub fn encode_subscribe(packet_id: u16, filters: &[(String, u8)]) -> MqttResult<Vec<u8>> {
    if filters.is_empty() {
        return Err(MqttError::InvalidArgument(
            "subscribe requires at least one filter".into(),
        ));
    }
    let mut body = Vec::new();
    write_u16(&mut body, packet_id);
    for (topic, qos) in filters {
        validate_topic_filter(topic)?;
        if *qos > 2 {
            return Err(MqttError::InvalidArgument("qos must be 0..=2".into()));
        }
        write_str(&mut body, topic);
        body.push(*qos);
    }
    let mut out = Vec::with_capacity(body.len() + 5);
    out.push(((PacketType::Subscribe as u8) << 4) | 0x02);
    write_remaining_length(&mut out, body.len());
    out.extend_from_slice(&body);
    Ok(out)
}

pub fn encode_unsubscribe(packet_id: u16, filters: &[String]) -> MqttResult<Vec<u8>> {
    if filters.is_empty() {
        return Err(MqttError::InvalidArgument(
            "unsubscribe requires at least one filter".into(),
        ));
    }
    let mut body = Vec::new();
    write_u16(&mut body, packet_id);
    for topic in filters {
        validate_topic_filter(topic)?;
        write_str(&mut body, topic);
    }
    let mut out = Vec::with_capacity(body.len() + 5);
    out.push(((PacketType::Unsubscribe as u8) << 4) | 0x02);
    write_remaining_length(&mut out, body.len());
    out.extend_from_slice(&body);
    Ok(out)
}

pub fn encode_pingreq() -> Vec<u8> {
    vec![(PacketType::Pingreq as u8) << 4, 0]
}

pub fn encode_disconnect() -> Vec<u8> {
    vec![(PacketType::Disconnect as u8) << 4, 0]
}

pub fn encode_connack(session_present: bool, return_code: u8) -> Vec<u8> {
    vec![
        (PacketType::Connack as u8) << 4,
        2,
        if session_present { 1 } else { 0 },
        return_code,
    ]
}

pub fn encode_suback(packet_id: u16, codes: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(2 + codes.len());
    write_u16(&mut body, packet_id);
    body.extend_from_slice(codes);
    let mut out = Vec::with_capacity(body.len() + 5);
    out.push((PacketType::Suback as u8) << 4);
    write_remaining_length(&mut out, body.len());
    out.extend_from_slice(&body);
    out
}

pub fn encode_unsuback(packet_id: u16) -> Vec<u8> {
    encode_id_packet(PacketType::Unsuback, 0, packet_id)
}

pub fn encode_pingresp() -> Vec<u8> {
    vec![(PacketType::Pingresp as u8) << 4, 0]
}

fn validate_topic_name(topic: &str) -> MqttResult<()> {
    if topic.is_empty() {
        return Err(MqttError::InvalidTopic("topic must not be empty".into()));
    }
    if topic.as_bytes().contains(&0) {
        return Err(MqttError::InvalidTopic("topic must not contain NUL".into()));
    }
    if topic.contains('+') || topic.contains('#') {
        return Err(MqttError::InvalidTopic(
            "topic name must not contain wildcards".into(),
        ));
    }
    Ok(())
}

fn validate_topic_filter(filter: &str) -> MqttResult<()> {
    if filter.is_empty() {
        return Err(MqttError::InvalidTopic("filter must not be empty".into()));
    }
    if filter.as_bytes().contains(&0) {
        return Err(MqttError::InvalidTopic(
            "filter must not contain NUL".into(),
        ));
    }
    // Basic wildcard placement checks
    let parts: Vec<&str> = filter.split('/').collect();
    for (i, part) in parts.iter().enumerate() {
        if part.contains('#') && (*part != "#" || i != parts.len() - 1) {
            return Err(MqttError::InvalidTopic(
                "invalid '#' wildcard placement".into(),
            ));
        }
        if part.contains('+') && *part != "+" {
            return Err(MqttError::InvalidTopic(
                "invalid '+' wildcard placement".into(),
            ));
        }
    }
    Ok(())
}

/// Decode one complete MQTT packet from `data`. Returns `(packet, bytes_consumed)`.
pub fn decode_packet(data: &[u8]) -> MqttResult<(Packet, usize)> {
    if data.is_empty() {
        return Err(MqttError::Protocol("empty packet".into()));
    }
    let first = data[0];
    let ptype = PacketType::from_u8(first >> 4)?;
    let flags = first & 0x0f;
    let mut off = 1usize;
    let rem = parse_remaining_length(data, &mut off)?;
    let header_len = off;
    if header_len + rem > data.len() {
        return Err(MqttError::Protocol("incomplete packet".into()));
    }
    let body = &data[header_len..header_len + rem];
    let mut boff = 0usize;

    let packet = match ptype {
        PacketType::Connect => {
            let proto = read_str(body, &mut boff)?;
            if proto != "MQTT" {
                return Err(MqttError::Protocol(format!("bad protocol name {proto}")));
            }
            if boff >= body.len() {
                return Err(MqttError::Protocol("truncated CONNECT".into()));
            }
            let level = body[boff];
            boff += 1;
            if boff >= body.len() {
                return Err(MqttError::Protocol("truncated CONNECT flags".into()));
            }
            let cflags = body[boff];
            boff += 1;
            let keepalive = read_u16(body, &mut boff)?;
            if level == PROTO_MQTT5 {
                let prop_len = read_varint(body, &mut boff)?;
                boff += prop_len;
            }
            let client_id = read_str(body, &mut boff)?;
            let clean = (cflags & 0x02) != 0;
            let mut will = None;
            if cflags & 0x04 != 0 {
                if level == PROTO_MQTT5 {
                    let prop_len = read_varint(body, &mut boff)?;
                    boff += prop_len;
                }
                let topic = read_str(body, &mut boff)?;
                let payload = read_bytes(body, &mut boff)?.to_vec();
                will = Some(Will {
                    topic,
                    payload,
                    qos: (cflags >> 3) & 0x03,
                    retain: cflags & 0x20 != 0,
                });
            }
            let username = if cflags & 0x80 != 0 {
                Some(read_str(body, &mut boff)?)
            } else {
                None
            };
            let password = if cflags & 0x40 != 0 {
                Some(read_bytes(body, &mut boff)?.to_vec())
            } else {
                None
            };
            Packet::Connect(ConnectOptions {
                client_id,
                protocol_level: level,
                clean_session: clean,
                keepalive,
                username,
                password,
                will,
            })
        }
        PacketType::Connack => {
            if body.len() < 2 {
                return Err(MqttError::Protocol("truncated CONNACK".into()));
            }
            Packet::Connack {
                session_present: body[0] & 0x01 != 0,
                return_code: body[1],
            }
        }
        PacketType::Publish => {
            let qos = (flags >> 1) & 0x03;
            let topic = read_str(body, &mut boff)?;
            let packet_id = if qos > 0 {
                Some(read_u16(body, &mut boff)?)
            } else {
                None
            };
            let payload = body[boff..].to_vec();
            Packet::Publish(PublishPacket {
                topic,
                payload,
                qos,
                retain: flags & 0x01 != 0,
                dup: flags & 0x08 != 0,
                packet_id,
            })
        }
        PacketType::Puback => Packet::Puback(read_u16(body, &mut boff)?),
        PacketType::Pubrec => Packet::Pubrec(read_u16(body, &mut boff)?),
        PacketType::Pubrel => Packet::Pubrel(read_u16(body, &mut boff)?),
        PacketType::Pubcomp => Packet::Pubcomp(read_u16(body, &mut boff)?),
        PacketType::Subscribe => {
            let packet_id = read_u16(body, &mut boff)?;
            let mut filters = Vec::new();
            while boff < body.len() {
                let topic = read_str(body, &mut boff)?;
                if boff >= body.len() {
                    return Err(MqttError::Protocol("truncated SUBSCRIBE qos".into()));
                }
                let qos = body[boff];
                boff += 1;
                filters.push((topic, qos));
            }
            Packet::Subscribe { packet_id, filters }
        }
        PacketType::Suback => {
            let packet_id = read_u16(body, &mut boff)?;
            Packet::Suback {
                packet_id,
                codes: body[boff..].to_vec(),
            }
        }
        PacketType::Unsubscribe => {
            let packet_id = read_u16(body, &mut boff)?;
            let mut filters = Vec::new();
            while boff < body.len() {
                filters.push(read_str(body, &mut boff)?);
            }
            Packet::Unsubscribe { packet_id, filters }
        }
        PacketType::Unsuback => Packet::Unsuback(read_u16(body, &mut boff)?),
        PacketType::Pingreq => Packet::Pingreq,
        PacketType::Pingresp => Packet::Pingresp,
        PacketType::Disconnect => Packet::Disconnect,
    };

    Ok((packet, header_len + rem))
}

fn read_varint(data: &[u8], off: &mut usize) -> MqttResult<usize> {
    parse_remaining_length(data, off)
}

/// Human-readable decode of a packet into typed field map pieces for binding layers.
pub fn packet_type_name(data: &[u8]) -> MqttResult<&'static str> {
    if data.is_empty() {
        return Err(MqttError::Protocol("empty packet".into()));
    }
    Ok(PacketType::from_u8(data[0] >> 4)?.name())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_roundtrip_311() {
        let opts = ConnectOptions {
            client_id: "c1".into(),
            protocol_level: PROTO_MQTT311,
            clean_session: true,
            keepalive: 30,
            username: Some("u".into()),
            password: Some(b"p".to_vec()),
            will: Some(Will {
                topic: "w/t".into(),
                payload: b"bye".to_vec(),
                qos: 1,
                retain: false,
            }),
        };
        let enc = encode_connect(&opts).unwrap();
        let (pkt, n) = decode_packet(&enc).unwrap();
        assert_eq!(n, enc.len());
        match pkt {
            Packet::Connect(c) => {
                assert_eq!(c.client_id, "c1");
                assert_eq!(c.keepalive, 30);
                assert!(c.will.is_some());
                assert_eq!(c.username.as_deref(), Some("u"));
            }
            _ => panic!("expected CONNECT"),
        }
    }

    #[test]
    fn publish_qos0_roundtrip() {
        let p = PublishPacket {
            topic: "a/b".into(),
            payload: b"hello".to_vec(),
            qos: 0,
            retain: true,
            dup: false,
            packet_id: None,
        };
        let enc = encode_publish(&p).unwrap();
        let (pkt, _) = decode_packet(&enc).unwrap();
        match pkt {
            Packet::Publish(q) => {
                assert_eq!(q.topic, "a/b");
                assert_eq!(q.payload, b"hello");
                assert!(q.retain);
                assert_eq!(q.qos, 0);
            }
            _ => panic!("expected PUBLISH"),
        }
    }

    #[test]
    fn remaining_length_large() {
        let payload = vec![0u8; 200];
        let p = PublishPacket {
            topic: "t".into(),
            payload,
            qos: 0,
            retain: false,
            dup: false,
            packet_id: None,
        };
        let enc = encode_publish(&p).unwrap();
        let (pkt, n) = decode_packet(&enc).unwrap();
        assert_eq!(n, enc.len());
        assert!(matches!(pkt, Packet::Publish(_)));
    }
}
