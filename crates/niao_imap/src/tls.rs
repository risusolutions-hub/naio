//! TLS helpers (rustls), matching niao_http.

use crate::error::{ImapError, Result};
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use rustls_native_certs::load_native_certs;
use std::net::TcpStream;
use std::sync::{Arc, OnceLock};

static TLS_CFG: OnceLock<Arc<ClientConfig>> = OnceLock::new();

fn install_crypto() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

pub fn tls_config() -> Result<Arc<ClientConfig>> {
    Ok(TLS_CFG
        .get_or_init(|| {
            install_crypto();
            let mut roots = RootCertStore::empty();
            for cert in load_native_certs().certs {
                let _ = roots.add(cert);
            }
            let cfg = ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth();
            Arc::new(cfg)
        })
        .clone())
}

pub type TlsStream = StreamOwned<ClientConnection, TcpStream>;

pub fn wrap_tls(stream: TcpStream, host: &str) -> Result<TlsStream> {
    let cfg = tls_config()?;
    let sni = ServerName::try_from(host.to_string())
        .map_err(|_| ImapError::Tls(format!("invalid SNI hostname: {host}")))?;
    let conn = ClientConnection::new(cfg, sni).map_err(|e| ImapError::Tls(e.to_string()))?;
    Ok(StreamOwned::new(conn, stream))
}
