//! Minimal in-process MQTT broker for unit/integration tests.

use crate::error::MqttResult;
use crate::packet::{self, Packet, PublishPacket};
use crate::topic::topic_matches;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

struct BrokerState {
    /// filter → set of client keys that subscribed
    subs: HashMap<String, Vec<u64>>,
    /// client key → subscriptions
    client_subs: HashMap<u64, Vec<String>>,
    next_client: u64,
}

impl BrokerState {
    fn new() -> Self {
        Self {
            subs: HashMap::new(),
            client_subs: HashMap::new(),
            next_client: 1,
        }
    }
}

/// Loopback MQTT 3.1.1 broker used by tests and benches.
pub struct MockBroker {
    port: u16,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl MockBroker {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock mqtt");
        let port = listener.local_addr().expect("addr").port();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);
        let state = Arc::new(Mutex::new(BrokerState::new()));
        let retained = Arc::new(Mutex::new(HashMap::new()));
        let (ready_tx, ready_rx) = mpsc::channel();
        let retained_clone = Arc::clone(&retained);
        let handle = thread::spawn(move || {
            ready_tx.send(()).ok();
            serve_loop(listener, stop_flag, state, retained_clone);
        });
        ready_rx.recv().expect("mock mqtt ready");
        Self {
            port,
            stop,
            handle: Some(handle),
        }
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn shutdown(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // wake accept
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn serve_loop(
    listener: TcpListener,
    stop: Arc<AtomicBool>,
    state: Arc<Mutex<BrokerState>>,
    retained: Arc<Mutex<HashMap<String, (Vec<u8>, u8, bool)>>>,
) {
    listener.set_nonblocking(true).ok();
    // writers: client_id → sender channel of raw packets to write
    let writers: Arc<Mutex<HashMap<u64, mpsc::Sender<Vec<u8>>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).ok();
                let state = Arc::clone(&state);
                let retained = Arc::clone(&retained);
                let writers = Arc::clone(&writers);
                thread::spawn(move || {
                    let _ = handle_client(stream, state, retained, writers);
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(2));
            }
            Err(_) => break,
        }
    }
}

fn handle_client(
    stream: TcpStream,
    state: Arc<Mutex<BrokerState>>,
    retained: Arc<Mutex<HashMap<String, (Vec<u8>, u8, bool)>>>,
    writers: Arc<Mutex<HashMap<u64, mpsc::Sender<Vec<u8>>>>>,
) -> MqttResult<()> {
    let mut stream = stream;
    stream.set_read_timeout(Some(Duration::from_secs(60))).ok();

    let client_key = {
        let mut st = state.lock().unwrap();
        let id = st.next_client;
        st.next_client += 1;
        id
    };

    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    writers.lock().unwrap().insert(client_key, tx.clone());

    // Writer thread
    let mut write_stream = stream.try_clone()?;
    let stop_write = Arc::new(AtomicBool::new(false));
    let stop_write2 = Arc::clone(&stop_write);
    let writer = thread::spawn(move || {
        while !stop_write2.load(Ordering::SeqCst) {
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(pkt) => {
                    if write_stream.write_all(&pkt).is_err() {
                        break;
                    }
                    let _ = write_stream.flush();
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => break,
            }
        }
    });

    let mut buf = Vec::new();
    let mut connected = false;

    loop {
        let mut tmp = [0u8; 2048];
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(_) => break,
        }

        loop {
            match packet::decode_packet(&buf) {
                Ok((pkt, n)) => {
                    buf.drain(..n);
                    match pkt {
                        Packet::Connect(opts) => {
                            let _ = opts;
                            let _ = tx.send(packet::encode_connack(false, 0));
                            connected = true;
                        }
                        Packet::Subscribe { packet_id, filters } => {
                            if !connected {
                                break;
                            }
                            let mut codes = Vec::new();
                            {
                                let mut st = state.lock().unwrap();
                                for (filter, qos) in &filters {
                                    st.subs.entry(filter.clone()).or_default().push(client_key);
                                    st.client_subs
                                        .entry(client_key)
                                        .or_default()
                                        .push(filter.clone());
                                    codes.push(*qos);
                                }
                            }
                            let _ = tx.send(packet::encode_suback(packet_id, &codes));
                            // deliver retained
                            let retained_map = retained.lock().unwrap();
                            for (filter, _) in &filters {
                                for (topic, (payload, qos, retain)) in retained_map.iter() {
                                    if topic_matches(filter, topic) {
                                        let pubpkt = PublishPacket {
                                            topic: topic.clone(),
                                            payload: payload.clone(),
                                            qos: *qos,
                                            retain: *retain,
                                            dup: false,
                                            packet_id: if *qos > 0 { Some(1) } else { None },
                                        };
                                        if let Ok(enc) = packet::encode_publish(&pubpkt) {
                                            let _ = tx.send(enc);
                                        }
                                    }
                                }
                            }
                        }
                        Packet::Unsubscribe { packet_id, filters } => {
                            {
                                let mut st = state.lock().unwrap();
                                if let Some(cs) = st.client_subs.get_mut(&client_key) {
                                    cs.retain(|f| !filters.contains(f));
                                }
                                for f in &filters {
                                    if let Some(v) = st.subs.get_mut(f) {
                                        v.retain(|c| *c != client_key);
                                    }
                                }
                            }
                            let _ = tx.send(packet::encode_unsuback(packet_id));
                        }
                        Packet::Publish(p) => {
                            if !connected {
                                break;
                            }
                            // QoS handshake with publisher
                            match p.qos {
                                1 => {
                                    if let Some(id) = p.packet_id {
                                        let _ = tx.send(packet::encode_puback(id));
                                    }
                                }
                                2 => {
                                    if let Some(id) = p.packet_id {
                                        let _ = tx.send(packet::encode_pubrec(id));
                                    }
                                }
                                _ => {}
                            }
                            if p.retain {
                                retained
                                    .lock()
                                    .unwrap()
                                    .insert(p.topic.clone(), (p.payload.clone(), p.qos, true));
                            }
                            // fan-out
                            let targets: Vec<u64> = {
                                let st = state.lock().unwrap();
                                let mut set = Vec::new();
                                for (filter, clients) in st.subs.iter() {
                                    if topic_matches(filter, &p.topic) {
                                        for c in clients {
                                            if !set.contains(c) {
                                                set.push(*c);
                                            }
                                        }
                                    }
                                }
                                set
                            };
                            let wr = writers.lock().unwrap();
                            for dest in targets {
                                let out_qos = p.qos.min(2);
                                let pid = if out_qos > 0 { Some(2) } else { None };
                                let out = PublishPacket {
                                    topic: p.topic.clone(),
                                    payload: p.payload.clone(),
                                    qos: out_qos,
                                    retain: p.retain,
                                    dup: false,
                                    packet_id: pid,
                                };
                                if let Ok(enc) = packet::encode_publish(&out) {
                                    if let Some(s) = wr.get(&dest) {
                                        let _ = s.send(enc);
                                    }
                                }
                            }
                        }
                        Packet::Pubrel(id) => {
                            let _ = tx.send(packet::encode_pubcomp(id));
                        }
                        Packet::Pingreq => {
                            let _ = tx.send(packet::encode_pingresp());
                        }
                        Packet::Disconnect => {
                            connected = false;
                            break;
                        }
                        _ => {}
                    }
                }
                Err(crate::error::MqttError::Protocol(ref m))
                    if m.contains("incomplete") || m.contains("truncated") =>
                {
                    break;
                }
                Err(_) => break,
            }
            if !connected && buf.is_empty() {
                // after disconnect
            }
        }
        if !connected && matches!(buf.first(), None) {
            // keep reading until disconnect path — break when disconnect handled
        }
    }

    stop_write.store(true, Ordering::SeqCst);
    writers.lock().unwrap().remove(&client_key);
    {
        let mut st = state.lock().unwrap();
        if let Some(filters) = st.client_subs.remove(&client_key) {
            for f in filters {
                if let Some(v) = st.subs.get_mut(&f) {
                    v.retain(|c| *c != client_key);
                }
            }
        }
    }
    let _ = writer.join();
    Ok(())
}
