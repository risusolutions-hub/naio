//! RFC 6455 WebSocket client and server for Niao.

mod client;
mod error;
mod frame;
mod handshake;
mod role;
mod server;
mod stream;
mod utf8;
mod websocket;

pub use client::connect;
pub use error::WsError;
pub use frame::{Frame, OPCODE_CLOSE, OPCODE_PING, OPCODE_PONG, OPCODE_TEXT};
pub use role::Role;
pub use server::WsServer;
pub use stream::WsStream;
pub use websocket::{CloseFrame, Message, WebSocket};

#[cfg(test)]
mod integration;
