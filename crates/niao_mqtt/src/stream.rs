//! TCP / TLS transport for MQTT.

use crate::error::{MqttError, MqttResult};
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use rustls_native_certs::load_native_certs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub enum MqttStream {
    Plain(TcpStream),
    Tls(Box<StreamOwned<ClientConnection, TcpStream>>),
}

impl Read for MqttStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(s) => s.read(buf),
            Self::Tls(s) => s.read(buf),
        }
    }
}

impl Write for MqttStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(s) => s.write(buf),
            Self::Tls(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Plain(s) => s.flush(),
            Self::Tls(s) => s.flush(),
        }
    }
}

impl MqttStream {
    pub fn set_read_timeout(&self, dur: Option<Duration>) -> std::io::Result<()> {
        match self {
            Self::Plain(s) => s.set_read_timeout(dur),
            Self::Tls(s) => s.sock.set_read_timeout(dur),
        }
    }

    pub fn set_write_timeout(&self, dur: Option<Duration>) -> std::io::Result<()> {
        match self {
            Self::Plain(s) => s.set_write_timeout(dur),
            Self::Tls(s) => s.sock.set_write_timeout(dur),
        }
    }
}

pub fn connect_tcp(host: &str, port: u16) -> MqttResult<MqttStream> {
    let addr = format!("{host}:{port}");
    let stream = TcpStream::connect(&addr).map_err(|e| MqttError::Io(e.to_string()))?;
    Ok(MqttStream::Plain(stream))
}

pub fn connect_tls(host: &str, port: u16) -> MqttResult<MqttStream> {
    let addr = format!("{host}:{port}");
    let tcp = TcpStream::connect(&addr).map_err(|e| MqttError::Io(e.to_string()))?;
    let cfg = tls_config()?;
    let sni = ServerName::try_from(host.to_string())
        .map_err(|_| MqttError::Tls("invalid SNI hostname".into()))?;
    let conn =
        ClientConnection::new(Arc::new(cfg), sni).map_err(|e| MqttError::Tls(e.to_string()))?;
    let mut tls = StreamOwned::new(conn, tcp);
    tls.flush().map_err(|e| MqttError::Io(e.to_string()))?;
    Ok(MqttStream::Tls(Box::new(tls)))
}

fn tls_config() -> MqttResult<ClientConfig> {
    static CONFIG: Mutex<Option<Arc<ClientConfig>>> = Mutex::new(None);
    let mut guard = CONFIG
        .lock()
        .map_err(|_| MqttError::Tls("tls config lock".into()))?;
    if let Some(cfg) = guard.clone() {
        return Ok((*cfg).clone());
    }
    let mut roots = RootCertStore::empty();
    for cert in load_native_certs().certs {
        let _ = roots.add(cert);
    }
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let cfg = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    *guard = Some(Arc::new(cfg.clone()));
    Ok(cfg)
}
