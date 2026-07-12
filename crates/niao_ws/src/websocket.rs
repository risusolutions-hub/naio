//! WebSocket connection over a byte stream.

use crate::error::WsError;
use crate::frame::{
    decode_frame, encode_frame, parse_close_payload, Frame, OPCODE_BINARY, OPCODE_CLOSE,
    OPCODE_CONT, OPCODE_PING, OPCODE_PONG, OPCODE_TEXT,
};
use crate::role::Role;
use crate::utf8;
use std::io::{Read, Write};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close(Option<CloseFrame>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseFrame {
    pub code: u16,
    pub reason: String,
}

pub struct WebSocket<S> {
    stream: S,
    role: Role,
    read_buf: Vec<u8>,
    frag_opcode: Option<u8>,
    frag_buf: Vec<u8>,
    closed: bool,
}

impl<S: Read + Write> WebSocket<S> {
    pub fn new(stream: S, role: Role) -> Self {
        Self {
            stream,
            role,
            read_buf: Vec::with_capacity(4096),
            frag_opcode: None,
            frag_buf: Vec::new(),
            closed: false,
        }
    }

    pub fn send(&mut self, msg: Message) -> Result<(), WsError> {
        if self.closed {
            return Err(WsError::Protocol("connection closed".into()));
        }
        let frame = match msg {
            Message::Text(s) => Frame::text(s.into_bytes(), true),
            Message::Binary(b) => Frame::binary(b, true),
            Message::Ping(p) => Frame::ping(p),
            Message::Pong(p) => Frame::pong(p),
            Message::Close(cf) => {
                let (code, reason) = match cf {
                    Some(c) => (Some(c.code), c.reason),
                    None => (None, String::new()),
                };
                Frame::close(code, &reason)
            }
        };
        let mut out = Vec::with_capacity(frame.payload.len() + 16);
        encode_frame(&frame, self.role, &mut out);
        self.stream
            .write_all(&out)
            .map_err(|e| WsError::Io(e.to_string()))?;
        if frame.opcode == OPCODE_CLOSE {
            self.closed = true;
        }
        Ok(())
    }

    pub fn read(&mut self) -> Result<Message, WsError> {
        loop {
            if let Some(msg) = self.try_parse_message()? {
                return Ok(msg);
            }
            let mut tmp = [0u8; 4096];
            let n = self
                .stream
                .read(&mut tmp)
                .map_err(|e| WsError::Io(e.to_string()))?;
            if n == 0 {
                return Err(WsError::Io("connection closed".into()));
            }
            self.read_buf.extend_from_slice(&tmp[..n]);
        }
    }

    fn try_parse_message(&mut self) -> Result<Option<Message>, WsError> {
        let mut offset = 0usize;
        loop {
            if offset >= self.read_buf.len() {
                return Ok(None);
            }
            match decode_frame(&self.read_buf[offset..], self.role) {
                Ok((frame, n)) => {
                    offset += n;
                    if let Some(msg) = self.process_frame(frame)? {
                        self.read_buf.drain(..offset);
                        return Ok(Some(msg));
                    }
                }
                Err(WsError::Incomplete) => return Ok(None),
                Err(e) => return Err(e),
            }
        }
    }

    fn process_frame(&mut self, frame: Frame) -> Result<Option<Message>, WsError> {
        let opcode = if frame.opcode == OPCODE_CONT {
            self.frag_opcode
                .ok_or_else(|| WsError::Protocol("unexpected continuation".into()))?
        } else {
            if self.frag_opcode.is_some() {
                return Err(WsError::Protocol("new data before fin".into()));
            }
            frame.opcode
        };

        if frame.opcode == OPCODE_CONT || (frame.opcode != OPCODE_CONT && !frame.fin) {
            self.frag_buf.extend_from_slice(&frame.payload);
            if self.frag_opcode.is_none() && frame.opcode != OPCODE_CONT {
                self.frag_opcode = Some(frame.opcode);
            }
            if !frame.fin {
                return Ok(None);
            }
            let payload = std::mem::take(&mut self.frag_buf);
            self.frag_opcode = None;
            return self.payload_to_message(opcode, payload);
        }

        self.payload_to_message(opcode, frame.payload)
    }

    fn payload_to_message(&mut self, opcode: u8, payload: Vec<u8>) -> Result<Option<Message>, WsError> {
        match opcode {
            OPCODE_TEXT => {
                if !utf8::is_valid_utf8(&payload) {
                    return Err(WsError::Utf8);
                }
                Ok(Some(Message::Text(
                    String::from_utf8_lossy(&payload).into_owned(),
                )))
            }
            OPCODE_BINARY => Ok(Some(Message::Binary(payload))),
            OPCODE_PING => {
                let _ = self.send(Message::Pong(payload.clone()));
                Ok(None)
            }
            OPCODE_PONG => Ok(None),
            OPCODE_CLOSE => {
                self.closed = true;
                let cf = if payload.is_empty() {
                    None
                } else {
                    let (code, reason) = parse_close_payload(&payload)?;
                    Some(CloseFrame {
                        code: code.unwrap_or(1000),
                        reason,
                    })
                };
                Ok(Some(Message::Close(cf)))
            }
            other => Err(WsError::Protocol(format!("unknown opcode {other}"))),
        }
    }

    pub fn close(&mut self, frame: Option<CloseFrame>) -> Result<(), WsError> {
        self.send(Message::Close(frame))
    }
}
