//! `niao_mqtt` — synchronous MQTT 3.1.1 / 5.0 client.
//!
//! Features: QoS 0–2, TLS (rustls), last wills, optional reconnect, topic
//! wildcards. Packet codec is pure Rust with a thin TCP/TLS transport layer so
//! the VM boundary stays small.

mod client;
mod error;
mod mock;
mod packet;
mod stream;
mod topic;

pub use client::{Client, ClientConfig, Message};
pub use error::{MqttError, MqttResult};
pub use mock::MockBroker;
pub use packet::{
    decode_packet, encode_connect, encode_disconnect, encode_pingreq, encode_publish,
    encode_subscribe, encode_unsubscribe, packet_type_name, ConnectOptions, Packet, PacketType,
    PublishPacket, Will, PROTO_MQTT311, PROTO_MQTT5,
};
pub use topic::topic_matches;

#[cfg(test)]
mod integration {
    use super::*;
    use std::time::Duration;

    #[test]
    fn pub_sub_qos0() {
        let broker = MockBroker::start();
        let port = broker.port();

        let mut sub = Client::new(ClientConfig {
            host: "127.0.0.1".into(),
            port,
            connect: ConnectOptions {
                client_id: "sub".into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
        sub.connect().unwrap();
        sub.subscribe("sensors/#", 0).unwrap();

        let mut pubc = Client::new(ClientConfig {
            host: "127.0.0.1".into(),
            port,
            connect: ConnectOptions {
                client_id: "pub".into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
        pubc.connect().unwrap();
        pubc.publish("sensors/temp", b"21.5", 0, false).unwrap();

        let msg = sub
            .recv(Some(Duration::from_secs(3)))
            .unwrap()
            .expect("message");
        assert_eq!(msg.topic, "sensors/temp");
        assert_eq!(msg.payload, b"21.5");

        pubc.disconnect().unwrap();
        sub.disconnect().unwrap();
        broker.shutdown();
    }

    #[test]
    fn qos1_and_qos2_publish() {
        let broker = MockBroker::start();
        let port = broker.port();
        let mut c = Client::new(ClientConfig {
            host: "127.0.0.1".into(),
            port,
            connect: ConnectOptions {
                client_id: "qos".into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
        c.connect().unwrap();
        c.publish("t/q1", b"one", 1, false).unwrap();
        c.publish("t/q2", b"two", 2, false).unwrap();
        c.ping().unwrap();
        c.disconnect().unwrap();
        broker.shutdown();
    }

    #[test]
    fn will_in_connect_packet() {
        let opts = ConnectOptions {
            client_id: "w".into(),
            will: Some(Will {
                topic: "status/client".into(),
                payload: b"offline".to_vec(),
                qos: 0,
                retain: true,
            }),
            ..Default::default()
        };
        let enc = encode_connect(&opts).unwrap();
        let (pkt, _) = decode_packet(&enc).unwrap();
        match pkt {
            Packet::Connect(c) => {
                let w = c.will.unwrap();
                assert_eq!(w.topic, "status/client");
                assert_eq!(w.payload, b"offline");
            }
            _ => panic!("expected connect"),
        }
    }

    #[test]
    fn mqtt5_connect_encodes() {
        let opts = ConnectOptions {
            client_id: "v5".into(),
            protocol_level: PROTO_MQTT5,
            ..Default::default()
        };
        let enc = encode_connect(&opts).unwrap();
        let (pkt, _) = decode_packet(&enc).unwrap();
        match pkt {
            Packet::Connect(c) => assert_eq!(c.protocol_level, PROTO_MQTT5),
            _ => panic!("expected connect"),
        }
    }
}
