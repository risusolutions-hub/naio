//! WebSocket client connect.

use crate::error::WsError;
use crate::handshake::client_handshake;
use crate::role::Role;
use crate::stream::{connect_tcp, connect_tls, WsStream};
use crate::websocket::WebSocket;
use niao_http::parse_url;

pub fn connect(url: &str) -> Result<(WebSocket<WsStream>, String), WsError> {
    let parsed = parse_url(url).map_err(|e| WsError::Handshake(e))?;
    if parsed.scheme != "ws" && parsed.scheme != "wss" {
        return Err(WsError::Handshake(format!(
            "unsupported scheme {}",
            parsed.scheme
        )));
    }
    let path = if parsed.query.is_empty() {
        parsed.path.clone()
    } else {
        format!("{}?{}", parsed.path, parsed.query)
    };
    let host = parsed.authority();
    let mut stream = if parsed.scheme == "wss" {
        connect_tls(&parsed.host, parsed.port)?
    } else {
        connect_tcp(&parsed.host, parsed.port)?
    };
    let _key = client_handshake(&mut stream, &host, &path)?;
    Ok((WebSocket::new(stream, Role::Client), url.to_string()))
}
