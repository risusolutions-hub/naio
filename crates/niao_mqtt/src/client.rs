//! Synchronous MQTT client (3.1.1 / 5) with QoS 0–2, wills, reconnect.

use crate::error::{MqttError, MqttResult};
use crate::packet::{self, ConnectOptions, Packet, PublishPacket, PROTO_MQTT311, PROTO_MQTT5};
use crate::stream::{connect_tcp, connect_tls, MqttStream};
use crate::topic::topic_matches;
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::time::{Duration, Instant};

/// Network endpoint and session options for [`Client`].
#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub host: String,
    pub port: u16,
    pub tls: bool,
    pub connect: ConnectOptions,
    pub reconnect: bool,
    pub reconnect_delay_ms: u64,
    pub reconnect_max_delay_ms: u64,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 1883,
            tls: false,
            connect: ConnectOptions::default(),
            reconnect: false,
            reconnect_delay_ms: 1000,
            reconnect_max_delay_ms: 30_000,
        }
    }
}

/// Incoming publish delivered to the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub topic: String,
    pub payload: Vec<u8>,
    pub qos: u8,
    pub retain: bool,
    pub dup: bool,
}

/// Blocking MQTT client.
pub struct Client {
    cfg: ClientConfig,
    stream: Option<MqttStream>,
    connected: bool,
    next_id: u16,
    inbox: VecDeque<Message>,
    read_buf: Vec<u8>,
    subscriptions: HashMap<String, u8>,
    /// In-flight QoS>0 publishes awaiting completion (packet_id → state).
    pending_pub: HashMap<u16, PendingPub>,
    last_activity: Instant,
    reconnect_attempts: u32,
}

#[derive(Debug, Clone)]
enum PendingPub {
    /// Sent PUBLISH qos1, waiting PUBACK.
    WaitPuback,
    /// Sent PUBLISH qos2, waiting PUBREC.
    WaitPubrec,
    /// Sent PUBREL, waiting PUBCOMP.
    WaitPubcomp,
}

impl Client {
    /// Create a disconnected client from config.
    pub fn new(mut cfg: ClientConfig) -> MqttResult<Self> {
        if cfg.host.is_empty() {
            return Err(MqttError::InvalidArgument("host must not be empty".into()));
        }
        if cfg.connect.client_id.is_empty() {
            cfg.connect.client_id = format!("niao-{}", simple_id());
        }
        if cfg.connect.protocol_level != PROTO_MQTT311 && cfg.connect.protocol_level != PROTO_MQTT5
        {
            return Err(MqttError::InvalidArgument(
                "protocol must be 3.1.1 (4) or 5".into(),
            ));
        }
        Ok(Self {
            cfg,
            stream: None,
            connected: false,
            next_id: 1,
            inbox: VecDeque::new(),
            read_buf: Vec::with_capacity(256),
            subscriptions: HashMap::new(),
            pending_pub: HashMap::new(),
            last_activity: Instant::now(),
            reconnect_attempts: 0,
        })
    }

    pub fn client_id(&self) -> &str {
        &self.cfg.connect.client_id
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub fn config(&self) -> &ClientConfig {
        &self.cfg
    }

    /// Open TCP/TLS and perform MQTT CONNECT handshake.
    pub fn connect(&mut self) -> MqttResult<()> {
        self.open_transport()?;
        self.mqtt_handshake()?;
        self.connected = true;
        self.reconnect_attempts = 0;
        self.last_activity = Instant::now();
        Ok(())
    }

    fn open_transport(&mut self) -> MqttResult<()> {
        let stream = if self.cfg.tls {
            connect_tls(&self.cfg.host, self.cfg.port)?
        } else {
            connect_tcp(&self.cfg.host, self.cfg.port)?
        };
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        stream.set_write_timeout(Some(Duration::from_secs(30)))?;
        self.stream = Some(stream);
        Ok(())
    }

    fn mqtt_handshake(&mut self) -> MqttResult<()> {
        let pkt = packet::encode_connect(&self.cfg.connect)?;
        self.write_all(&pkt)?;
        let resp = self.read_packet_blocking()?;
        match resp {
            Packet::Connack {
                session_present: _,
                return_code,
            } => {
                if return_code != 0 {
                    self.connected = false;
                    self.stream = None;
                    return Err(MqttError::Connack(
                        return_code,
                        connack_reason(return_code).into(),
                    ));
                }
                Ok(())
            }
            other => Err(MqttError::Protocol(format!(
                "expected CONNACK, got {:?}",
                other
            ))),
        }
    }

    /// Publish with QoS handshake (blocking until complete for qos>0).
    pub fn publish(
        &mut self,
        topic: &str,
        payload: &[u8],
        qos: u8,
        retain: bool,
    ) -> MqttResult<()> {
        self.ensure_connected()?;
        if qos > 2 {
            return Err(MqttError::InvalidArgument("qos must be 0..=2".into()));
        }
        let packet_id = if qos > 0 { Some(self.alloc_id()) } else { None };
        let pubpkt = PublishPacket {
            topic: topic.to_string(),
            payload: payload.to_vec(),
            qos,
            retain,
            dup: false,
            packet_id,
        };
        let enc = packet::encode_publish(&pubpkt)?;
        self.write_all(&enc)?;
        match qos {
            0 => Ok(()),
            1 => {
                let id = packet_id.unwrap();
                self.pending_pub.insert(id, PendingPub::WaitPuback);
                self.wait_for_puback(id)
            }
            2 => {
                let id = packet_id.unwrap();
                self.pending_pub.insert(id, PendingPub::WaitPubrec);
                self.wait_for_pubcomp(id)
            }
            _ => unreachable!(),
        }
    }

    pub fn subscribe(&mut self, topic: &str, qos: u8) -> MqttResult<()> {
        self.subscribe_many(&[(topic.to_string(), qos)])
    }

    pub fn subscribe_many(&mut self, filters: &[(String, u8)]) -> MqttResult<()> {
        self.ensure_connected()?;
        let id = self.alloc_id();
        let enc = packet::encode_subscribe(id, filters)?;
        self.write_all(&enc)?;
        loop {
            match self.read_packet_blocking()? {
                Packet::Suback { packet_id, codes } if packet_id == id => {
                    for (i, (topic, qos)) in filters.iter().enumerate() {
                        let code = codes.get(i).copied().unwrap_or(0x80);
                        if code == 0x80 {
                            return Err(MqttError::Protocol(format!(
                                "subscribe rejected for {topic}"
                            )));
                        }
                        self.subscriptions.insert(topic.clone(), *qos);
                    }
                    return Ok(());
                }
                other => self.handle_async_packet(other)?,
            }
        }
    }

    pub fn unsubscribe(&mut self, topic: &str) -> MqttResult<()> {
        self.unsubscribe_many(&[topic.to_string()])
    }

    pub fn unsubscribe_many(&mut self, filters: &[String]) -> MqttResult<()> {
        self.ensure_connected()?;
        let id = self.alloc_id();
        let enc = packet::encode_unsubscribe(id, filters)?;
        self.write_all(&enc)?;
        loop {
            match self.read_packet_blocking()? {
                Packet::Unsuback(packet_id) if packet_id == id => {
                    for t in filters {
                        self.subscriptions.remove(t);
                    }
                    return Ok(());
                }
                other => self.handle_async_packet(other)?,
            }
        }
    }

    /// Wait for an inbound PUBLISH (or return None on timeout).
    pub fn recv(&mut self, timeout: Option<Duration>) -> MqttResult<Option<Message>> {
        if let Some(msg) = self.inbox.pop_front() {
            return Ok(Some(msg));
        }
        self.ensure_connected()?;
        let deadline = timeout.map(|t| Instant::now() + t);
        loop {
            if let Some(msg) = self.inbox.pop_front() {
                return Ok(Some(msg));
            }
            if let Some(dl) = deadline {
                if Instant::now() >= dl {
                    return Ok(None);
                }
                let remain = dl.saturating_duration_since(Instant::now());
                self.set_read_timeout(Some(remain.max(Duration::from_millis(1))))?;
            } else {
                self.set_read_timeout(Some(Duration::from_secs(60)))?;
            }
            match self.try_read_packet() {
                Ok(Some(pkt)) => self.handle_async_packet(pkt)?,
                Ok(None) => {
                    if deadline.is_some() {
                        return Ok(None);
                    }
                }
                Err(MqttError::Io(ref msg))
                    if msg.contains("timed out") || msg.contains("WouldBlock") =>
                {
                    if deadline.is_some() {
                        return Ok(None);
                    }
                }
                Err(e) => {
                    if self.cfg.reconnect {
                        self.try_reconnect()?;
                        continue;
                    }
                    return Err(e);
                }
            }
            self.maybe_ping()?;
        }
    }

    pub fn ping(&mut self) -> MqttResult<()> {
        self.ensure_connected()?;
        self.write_all(&packet::encode_pingreq())?;
        loop {
            match self.read_packet_blocking()? {
                Packet::Pingresp => return Ok(()),
                other => self.handle_async_packet(other)?,
            }
        }
    }

    pub fn disconnect(&mut self) -> MqttResult<()> {
        if self.connected {
            let _ = self.write_all(&packet::encode_disconnect());
        }
        self.connected = false;
        self.stream = None;
        Ok(())
    }

    /// Reconnect using stored credentials and re-subscribe prior filters.
    pub fn reconnect(&mut self) -> MqttResult<()> {
        let _ = self.disconnect();
        let mut delay = self.cfg.reconnect_delay_ms;
        loop {
            match self.connect() {
                Ok(()) => break,
                Err(e) => {
                    if !self.cfg.reconnect {
                        return Err(e);
                    }
                    self.reconnect_attempts = self.reconnect_attempts.saturating_add(1);
                    std::thread::sleep(Duration::from_millis(delay));
                    delay = (delay.saturating_mul(2)).min(self.cfg.reconnect_max_delay_ms);
                    if self.reconnect_attempts > 20 {
                        return Err(e);
                    }
                }
            }
        }
        let subs: Vec<(String, u8)> = self
            .subscriptions
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        if !subs.is_empty() {
            self.subscribe_many(&subs)?;
        }
        Ok(())
    }

    fn try_reconnect(&mut self) -> MqttResult<()> {
        if !self.cfg.reconnect {
            return Err(MqttError::NotConnected);
        }
        self.reconnect()
    }

    fn ensure_connected(&mut self) -> MqttResult<()> {
        if self.connected && self.stream.is_some() {
            return Ok(());
        }
        if self.cfg.reconnect {
            return self.try_reconnect();
        }
        Err(MqttError::NotConnected)
    }

    fn alloc_id(&mut self) -> u16 {
        let id = self.next_id;
        self.next_id = if self.next_id == u16::MAX {
            1
        } else {
            self.next_id + 1
        };
        id
    }

    fn write_all(&mut self, data: &[u8]) -> MqttResult<()> {
        let stream = self.stream.as_mut().ok_or(MqttError::NotConnected)?;
        stream.write_all(data)?;
        stream.flush()?;
        self.last_activity = Instant::now();
        Ok(())
    }

    fn set_read_timeout(&self, dur: Option<Duration>) -> MqttResult<()> {
        if let Some(s) = &self.stream {
            s.set_read_timeout(dur)?;
        }
        Ok(())
    }

    fn read_packet_blocking(&mut self) -> MqttResult<Packet> {
        self.set_read_timeout(Some(Duration::from_secs(30)))?;
        loop {
            if let Some(pkt) = self.try_read_packet()? {
                return Ok(pkt);
            }
        }
    }

    fn try_read_packet(&mut self) -> MqttResult<Option<Packet>> {
        // Need at least 2 bytes for fixed header
        while self.read_buf.len() < 2 {
            if !self.read_more()? {
                return Ok(None);
            }
        }
        // Parse remaining length to know full size
        match packet::decode_packet(&self.read_buf) {
            Ok((pkt, n)) => {
                self.read_buf.drain(..n);
                self.last_activity = Instant::now();
                return Ok(Some(pkt));
            }
            Err(MqttError::Protocol(ref m))
                if m.contains("incomplete") || m.contains("truncated") =>
            {
                // need more bytes — fall through
            }
            Err(e) => return Err(e),
        }
        if !self.read_more()? {
            return Ok(None);
        }
        // retry after reading
        match packet::decode_packet(&self.read_buf) {
            Ok((pkt, n)) => {
                self.read_buf.drain(..n);
                self.last_activity = Instant::now();
                Ok(Some(pkt))
            }
            Err(MqttError::Protocol(ref m))
                if m.contains("incomplete") || m.contains("truncated") =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    fn read_more(&mut self) -> MqttResult<bool> {
        let mut tmp = [0u8; 2048];
        let stream = self.stream.as_mut().ok_or(MqttError::NotConnected)?;
        match stream.read(&mut tmp) {
            Ok(0) => {
                self.connected = false;
                Err(MqttError::Io("connection closed".into()))
            }
            Ok(n) => {
                self.read_buf.extend_from_slice(&tmp[..n]);
                Ok(true)
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                Ok(false)
            }
            Err(e) => Err(MqttError::Io(e.to_string())),
        }
    }

    fn handle_async_packet(&mut self, pkt: Packet) -> MqttResult<()> {
        match pkt {
            Packet::Publish(p) => self.on_publish(p),
            Packet::Puback(id) => {
                self.pending_pub.remove(&id);
                Ok(())
            }
            Packet::Pubrec(id) => {
                if matches!(self.pending_pub.get(&id), Some(PendingPub::WaitPubrec)) {
                    self.write_all(&packet::encode_pubrel(id))?;
                    self.pending_pub.insert(id, PendingPub::WaitPubcomp);
                }
                Ok(())
            }
            Packet::Pubcomp(id) => {
                self.pending_pub.remove(&id);
                Ok(())
            }
            Packet::Pubrel(id) => {
                self.write_all(&packet::encode_pubcomp(id))?;
                Ok(())
            }
            Packet::Pingresp => Ok(()),
            Packet::Disconnect => {
                self.connected = false;
                self.stream = None;
                Err(MqttError::Io("broker disconnected".into()))
            }
            other => Err(MqttError::Protocol(format!(
                "unexpected packet while waiting: {:?}",
                other
            ))),
        }
    }

    fn on_publish(&mut self, p: PublishPacket) -> MqttResult<()> {
        match p.qos {
            0 => {}
            1 => {
                if let Some(id) = p.packet_id {
                    self.write_all(&packet::encode_puback(id))?;
                }
            }
            2 => {
                if let Some(id) = p.packet_id {
                    self.write_all(&packet::encode_pubrec(id))?;
                    // wait for PUBREL handled in handle_async_packet
                }
            }
            _ => {
                return Err(MqttError::Protocol(format!("bad qos {}", p.qos)));
            }
        }
        // Deliver if matches any subscription (broker should filter, but be safe)
        let deliver = self.subscriptions.is_empty()
            || self
                .subscriptions
                .keys()
                .any(|f| topic_matches(f, &p.topic));
        if deliver {
            self.inbox.push_back(Message {
                topic: p.topic,
                payload: p.payload,
                qos: p.qos,
                retain: p.retain,
                dup: p.dup,
            });
        }
        Ok(())
    }

    fn wait_for_puback(&mut self, id: u16) -> MqttResult<()> {
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            if !self.pending_pub.contains_key(&id) {
                return Ok(());
            }
            let pkt = self.read_packet_blocking()?;
            self.handle_async_packet(pkt)?;
        }
        Err(MqttError::Protocol("PUBACK timeout".into()))
    }

    fn wait_for_pubcomp(&mut self, id: u16) -> MqttResult<()> {
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            if !self.pending_pub.contains_key(&id) {
                return Ok(());
            }
            let pkt = self.read_packet_blocking()?;
            self.handle_async_packet(pkt)?;
        }
        Err(MqttError::Protocol("PUBCOMP timeout".into()))
    }

    fn maybe_ping(&mut self) -> MqttResult<()> {
        let ka = self.cfg.connect.keepalive as u64;
        if ka == 0 {
            return Ok(());
        }
        if self.last_activity.elapsed() > Duration::from_secs(ka.saturating_mul(8) / 10) {
            self.write_all(&packet::encode_pingreq())?;
            // Don't block forever on pingresp inside recv loop — treat next packets normally
        }
        Ok(())
    }
}

fn connack_reason(code: u8) -> &'static str {
    match code {
        0 => "accepted",
        1 => "unacceptable protocol version",
        2 => "identifier rejected",
        3 => "server unavailable",
        4 => "bad username or password",
        5 => "not authorized",
        _ => "connection refused",
    }
}

fn simple_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1)
}
