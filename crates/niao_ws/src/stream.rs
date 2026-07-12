//! TCP/TLS stream wrapper for WebSocket I/O.

use crate::error::WsError;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use rustls_native_certs::load_native_certs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

pub enum WsStream {
    Plain(TcpStream),
    Tls(Box<StreamOwned<ClientConnection, TcpStream>>),
}

impl Read for WsStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(s) => s.read(buf),
            Self::Tls(s) => s.read(buf),
        }
    }
}

impl Write for WsStream {
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

pub fn connect_tcp(host: &str, port: u16) -> Result<WsStream, WsError> {
    let addr = format!("{host}:{port}");
    let stream = TcpStream::connect(&addr).map_err(|e| WsError::Io(e.to_string()))?;
    Ok(WsStream::Plain(stream))
}

pub fn connect_tls(host: &str, port: u16) -> Result<WsStream, WsError> {
    let addr = format!("{host}:{port}");
    let tcp = TcpStream::connect(&addr).map_err(|e| WsError::Io(e.to_string()))?;
    let cfg = tls_config()?;
    let sni = ServerName::try_from(host.to_string())
        .map_err(|_| WsError::Tls("invalid sni".into()))?;
    let conn =
        ClientConnection::new(Arc::new(cfg), sni).map_err(|e| WsError::Tls(e.to_string()))?;
    let mut tls = StreamOwned::new(conn, tcp);
    tls.flush().map_err(|e| WsError::Io(e.to_string()))?;
    Ok(WsStream::Tls(Box::new(tls)))
}

fn tls_config() -> Result<ClientConfig, WsError> {
    static CONFIG: Mutex<Option<Arc<ClientConfig>>> = Mutex::new(None);
    let mut guard = CONFIG.lock().unwrap();
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
