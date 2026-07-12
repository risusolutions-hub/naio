//! WebSocket server accept.

use crate::error::WsError;
use crate::handshake::server_handshake;
use crate::role::Role;
use crate::websocket::WebSocket;
use std::net::{TcpListener, TcpStream};

pub struct WsServer {
    listener: TcpListener,
}

impl WsServer {
    pub fn bind(addr: &str) -> Result<Self, WsError> {
        let listener = TcpListener::bind(addr).map_err(|e| WsError::Io(e.to_string()))?;
        Ok(Self { listener })
    }

    pub fn local_addr(&self) -> Result<std::net::SocketAddr, WsError> {
        self.listener
            .local_addr()
            .map_err(|e| WsError::Io(e.to_string()))
    }

    pub fn accept(&self) -> Result<WebSocket<TcpStream>, WsError> {
        let (mut stream, _) = self
            .listener
            .accept()
            .map_err(|e| WsError::Io(e.to_string()))?;
        server_handshake(&mut stream)?;
        Ok(WebSocket::new(stream, Role::Server))
    }
}
