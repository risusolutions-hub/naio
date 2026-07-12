//! Sync HTTP/1.1 server (thread-pool friendly accept loop).

use crate::headers::HeaderMap;
use crate::parser::{body_mode, parse_request, read_body, ParseError, RequestHead};
use crate::status::Status;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct Server {
    listener: Arc<TcpListener>,
    stop: Arc<Mutex<bool>>,
}

impl Server {
    pub fn http(addr: &str) -> io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        listener.set_nonblocking(false)?;
        Ok(Self {
            listener: Arc::new(listener),
            stop: Arc::new(Mutex::new(false)),
        })
    }

    pub fn recv(&self) -> io::Result<IncomingRequest> {
        loop {
            if *self.stop.lock().unwrap() {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "stopped"));
            }
            match self.listener.accept() {
                Ok((stream, addr)) => return read_request(stream, addr),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                Err(e) => return Err(e),
            }
        }
    }

    pub fn try_recv(&self) -> io::Result<Option<IncomingRequest>> {
        if *self.stop.lock().unwrap() {
            return Ok(None);
        }
        self.listener.set_nonblocking(true)?;
        let result = match self.listener.accept() {
            Ok((stream, addr)) => Ok(Some(read_request(stream, addr)?)),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e),
        };
        let _ = self.listener.set_nonblocking(false);
        result
    }

    pub fn stop(&self) {
        *self.stop.lock().unwrap() = true;
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }
}

pub struct IncomingRequest {
    stream: TcpStream,
    pub head: RequestHead,
    pub body: Vec<u8>,
    pub remote_addr: SocketAddr,
}

impl IncomingRequest {
    pub fn method(&self) -> &str {
        self.head.method.as_str()
    }

    pub fn url(&self) -> &str {
        &self.head.target
    }

    pub fn headers(&self) -> &HeaderMap {
        &self.head.headers
    }

    pub fn as_reader(&mut self) -> &[u8] {
        &self.body
    }

    pub fn respond(mut self, response: OutgoingResponse) -> io::Result<()> {
        let status = response.status;
        let reason = Status(status).reason();
        let mut out = format!("HTTP/1.1 {status} {reason}\r\n");
        let body = response.body;
        if !body.is_empty() {
            out.push_str(&format!("Content-Length: {}\r\n", body.len()));
        }
        for (k, v) in &response.headers {
            out.push_str(&format!("{k}: {v}\r\n"));
        }
        out.push_str("Connection: close\r\n\r\n");
        self.stream.write_all(out.as_bytes())?;
        if !body.is_empty() {
            self.stream.write_all(&body)?;
        }
        self.stream.flush()?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct OutgoingResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl OutgoingResponse {
    pub fn from_data(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            headers: Vec::new(),
            body: body.into(),
        }
    }

    pub fn from_string(body: impl Into<String>) -> Self {
        Self::from_data(body.into().into_bytes())
    }

    pub fn with_status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }
}

fn read_request(mut stream: TcpStream, remote_addr: SocketAddr) -> io::Result<IncomingRequest> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let mut buf = Vec::with_capacity(4096);
    let mut tmp = [0u8; 4096];
    loop {
        let n = stream.read(&mut tmp)?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            if let Ok((head, off)) = parse_request(&buf) {
                let mode = body_mode(&head.headers, Some(head.method)).map_err(map_parse)?;
                if read_body(mode, &buf, off).is_ok() {
                    break;
                }
            }
        }
        if buf.len() > 1024 * 1024 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "request too large"));
        }
    }
    let (head, off) = parse_request(&buf).map_err(map_parse)?;
    let mode = body_mode(&head.headers, Some(head.method)).map_err(map_parse)?;
    let (body, _) = read_body(mode, &buf, off).map_err(map_parse)?;
    Ok(IncomingRequest {
        stream,
        head,
        body,
        remote_addr,
    })
}

fn map_parse(e: ParseError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("{e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn client_server_roundtrip() {
        let server = Server::http("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let req = server.recv().unwrap();
            assert_eq!(req.method(), "GET");
            req.respond(OutgoingResponse::from_string("hello")).unwrap();
        });
        let mut stream = TcpStream::connect(addr).unwrap();
        stream
            .write_all(b"GET /test HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();
        let mut resp = Vec::new();
        stream.read_to_end(&mut resp).unwrap();
        assert!(String::from_utf8_lossy(&resp).contains("200"));
        assert!(String::from_utf8_lossy(&resp).contains("hello"));
        handle.join().unwrap();
    }
}
