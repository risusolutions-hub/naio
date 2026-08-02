//! gRPC server over HTTP/2 (cleartext h2c).

use crate::channel::status_trailers;
use crate::codec::{frame_message, FrameDecoder};
use crate::error::{GrpcError, GrpcResult};
use crate::metadata::{normalize_method_path, Metadata};
use crate::runtime::runtime;
use crate::status::{Status, StatusCode};
use bytes::Bytes;
use http::Response;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::Mutex as AsyncMutex;

/// Kind of registered RPC handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodKind {
    Unary,
    ServerStream,
    ClientStream,
    Bidi,
}

impl MethodKind {
    pub fn parse(s: &str) -> GrpcResult<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "unary" => Ok(Self::Unary),
            "server_stream" | "serverstream" | "server-streaming" => Ok(Self::ServerStream),
            "client_stream" | "clientstream" | "client-streaming" => Ok(Self::ClientStream),
            "bidi" | "bidi_stream" | "stream_stream" | "bidi-streaming" => Ok(Self::Bidi),
            other => Err(GrpcError::new(format!(
                "unknown method kind '{other}' (use unary|server_stream|client_stream|bidi)"
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unary => "unary",
            Self::ServerStream => "server_stream",
            Self::ClientStream => "client_stream",
            Self::Bidi => "bidi",
        }
    }
}

/// Incoming RPC before user handler runs.
#[derive(Debug, Clone)]
pub struct IncomingRpc {
    pub method: String,
    pub kind: MethodKind,
    pub metadata: Metadata,
    pub messages: Vec<Vec<u8>>,
}

/// Handler reply.
#[derive(Debug, Clone)]
pub struct HandlerReply {
    pub status: Status,
    pub messages: Vec<Vec<u8>>,
    pub headers: Metadata,
}

impl HandlerReply {
    pub fn ok_bytes(bytes: Vec<u8>) -> Self {
        Self {
            status: Status::ok(),
            messages: vec![bytes],
            headers: Metadata::new(),
        }
    }

    pub fn ok_messages(messages: Vec<Vec<u8>>) -> Self {
        Self {
            status: Status::ok(),
            messages,
            headers: Metadata::new(),
        }
    }

    pub fn status_only(status: Status) -> Self {
        Self {
            status,
            messages: Vec::new(),
            headers: Metadata::new(),
        }
    }
}

pub type SyncHandler = Arc<dyn Fn(IncomingRpc) -> HandlerReply + Send + Sync>;

struct Route {
    kind: MethodKind,
    handler: SyncHandler,
}

struct ServerShared {
    routes: Mutex<HashMap<String, Route>>,
    stop: AtomicBool,
}

/// Bound gRPC server ready to accept connections.
pub struct GrpcServer {
    listener: Arc<AsyncMutex<Option<TcpListener>>>,
    shared: Arc<ServerShared>,
    local: SocketAddr,
    bg: Mutex<Option<JoinHandle<()>>>,
}

impl GrpcServer {
    pub fn bind(addr: &str) -> GrpcResult<Self> {
        let addr = addr.trim();
        if addr.is_empty() {
            return Err(GrpcError::new("server address must be non-empty"));
        }
        let std_listener = std::net::TcpListener::bind(addr)
            .map_err(|e| GrpcError::new(format!("bind {addr}: {e}")))?;
        std_listener
            .set_nonblocking(true)
            .map_err(|e| GrpcError::new(e.to_string()))?;
        let local = std_listener
            .local_addr()
            .map_err(|e| GrpcError::new(e.to_string()))?;
        let listener = runtime().block_on(async {
            TcpListener::from_std(std_listener)
                .map_err(|e| GrpcError::new(format!("tokio listener: {e}")))
        })?;
        Ok(Self {
            listener: Arc::new(AsyncMutex::new(Some(listener))),
            shared: Arc::new(ServerShared {
                routes: Mutex::new(HashMap::new()),
                stop: AtomicBool::new(false),
            }),
            local,
            bg: Mutex::new(None),
        })
    }

    pub fn addr(&self) -> String {
        format!("{}:{}", self.local.ip(), self.local.port())
    }

    pub fn register(&self, method: &str, kind: MethodKind, handler: SyncHandler) -> GrpcResult<()> {
        let path = normalize_method_path(method)?;
        self.shared
            .routes
            .lock()
            .map_err(|_| GrpcError::new("routes lock poisoned"))?
            .insert(path, Route { kind, handler });
        Ok(())
    }

    pub fn stop(&self) {
        self.shared.stop.store(true, Ordering::SeqCst);
    }

    /// Accept and serve one TCP connection using registered SyncHandlers.
    pub fn poll(&self, timeout: Option<Duration>) -> GrpcResult<bool> {
        let shared = Arc::clone(&self.shared);
        self.poll_connection(timeout, move |incoming| dispatch_sync(&shared, incoming))
    }

    /// Accept one TCP connection and handle streams with a same-thread callback
    /// (safe for Niao `call_niao_function` from the VM thread).
    pub fn poll_with<F>(&self, timeout: Option<Duration>, mut handler: F) -> GrpcResult<bool>
    where
        F: FnMut(IncomingRpc) -> HandlerReply,
    {
        self.poll_connection(timeout, |incoming| handler(incoming))
    }

    fn poll_connection<F>(&self, timeout: Option<Duration>, mut handler: F) -> GrpcResult<bool>
    where
        F: FnMut(IncomingRpc) -> HandlerReply,
    {
        if self.shared.stop.load(Ordering::SeqCst) {
            return Ok(false);
        }
        let listener = Arc::clone(&self.listener);
        runtime().block_on(async move {
            let mut guard = listener.lock().await;
            let tcp_listener = guard
                .as_mut()
                .ok_or_else(|| GrpcError::new("server listener closed"))?;
            let accept = tcp_listener.accept();
            let (tcp, _) = match timeout {
                Some(t) => match tokio::time::timeout(t, accept).await {
                    Ok(Ok(pair)) => pair,
                    Ok(Err(e)) => return Err(GrpcError::new(e.to_string())),
                    Err(_) => return Ok(false),
                },
                None => accept.await.map_err(|e| GrpcError::new(e.to_string()))?,
            };
            drop(guard);
            tcp.set_nodelay(true).ok();
            serve_connection_serial(tcp, &mut handler).await?;
            Ok(true)
        })
    }

    /// Blocking accept loop until `stop()`, calling `handler` on the current thread.
    pub fn serve_with<F>(&self, mut handler: F) -> GrpcResult<()>
    where
        F: FnMut(IncomingRpc) -> HandlerReply,
    {
        while !self.shared.stop.load(Ordering::SeqCst) {
            let _ = self.poll_with(Some(Duration::from_millis(200)), |rpc| handler(rpc))?;
        }
        Ok(())
    }

    /// Blocking accept loop until `stop()` using SyncHandlers.
    pub fn serve(&self) -> GrpcResult<()> {
        while !self.shared.stop.load(Ordering::SeqCst) {
            let _ = self.poll(Some(Duration::from_millis(200)))?;
        }
        Ok(())
    }

    /// Serve connections on a background OS thread.
    pub fn serve_bg(&self) -> GrpcResult<()> {
        let mut bg = self
            .bg
            .lock()
            .map_err(|_| GrpcError::new("bg lock poisoned"))?;
        if bg.is_some() {
            return Ok(());
        }
        let listener = Arc::clone(&self.listener);
        let shared = Arc::clone(&self.shared);
        let handle = thread::Builder::new()
            .name("ngrpc-server".into())
            .spawn(move || {
                runtime().block_on(async move {
                    while !shared.stop.load(Ordering::SeqCst) {
                        let accept_result = {
                            let mut guard = listener.lock().await;
                            let Some(l) = guard.as_mut() else {
                                break;
                            };
                            tokio::time::timeout(Duration::from_millis(200), l.accept()).await
                        };
                        match accept_result {
                            Ok(Ok((tcp, _))) => {
                                tcp.set_nodelay(true).ok();
                                let shared2 = Arc::clone(&shared);
                                runtime().spawn(async move {
                                    let _ = serve_connection(tcp, shared2).await;
                                });
                            }
                            Ok(Err(_)) => break,
                            Err(_) => {} // accept timeout
                        }
                    }
                });
            })
            .map_err(|e| GrpcError::new(format!("spawn server thread: {e}")))?;
        *bg = Some(handle);
        Ok(())
    }

    pub fn join_bg(&self) {
        if let Ok(mut bg) = self.bg.lock() {
            if let Some(h) = bg.take() {
                let _ = h.join();
            }
        }
    }
}

fn dispatch_sync(shared: &ServerShared, mut incoming: IncomingRpc) -> HandlerReply {
    let route = shared.routes.lock().ok().and_then(|routes| {
        routes
            .get(&incoming.method)
            .map(|r| (r.kind, Arc::clone(&r.handler)))
    });
    match route {
        Some((kind, handler)) => {
            incoming.kind = kind;
            handler(incoming)
        }
        None => HandlerReply::status_only(Status::new(
            StatusCode::Unimplemented,
            format!("unknown method {}", incoming.method),
        )),
    }
}

async fn serve_connection(tcp: tokio::net::TcpStream, shared: Arc<ServerShared>) -> GrpcResult<()> {
    let mut conn = h2::server::handshake(tcp)
        .await
        .map_err(|e| GrpcError::new(format!("h2 server handshake: {e}")))?;

    while let Some(result) = conn.accept().await {
        let (request, respond) =
            result.map_err(|e| GrpcError::new(format!("accept stream: {e}")))?;
        let shared = Arc::clone(&shared);
        // Handle serially so SyncHandlers don't need an extra thread pool steal.
        let _ = handle_request_sync(request, respond, |incoming| {
            dispatch_sync(&shared, incoming)
        })
        .await;
    }
    Ok(())
}

async fn serve_connection_serial<F>(tcp: tokio::net::TcpStream, handler: &mut F) -> GrpcResult<()>
where
    F: FnMut(IncomingRpc) -> HandlerReply,
{
    let mut conn = h2::server::handshake(tcp)
        .await
        .map_err(|e| GrpcError::new(format!("h2 server handshake: {e}")))?;

    while let Some(result) = conn.accept().await {
        let (request, respond) =
            result.map_err(|e| GrpcError::new(format!("accept stream: {e}")))?;
        handle_request_sync(request, respond, |incoming| handler(incoming)).await?;
    }
    Ok(())
}

async fn handle_request_sync<F>(
    request: http::Request<h2::RecvStream>,
    mut respond: h2::server::SendResponse<Bytes>,
    mut handler: F,
) -> GrpcResult<()>
where
    F: FnMut(IncomingRpc) -> HandlerReply,
{
    let path = request.uri().path().to_string();
    let metadata = headers_to_meta(request.headers());
    let mut body = request.into_body();

    let mut decoder = FrameDecoder::new();
    let mut messages = Vec::new();
    loop {
        let chunk = body
            .data()
            .await
            .transpose()
            .map_err(|e| GrpcError::new(format!("read request: {e}")))?;
        match chunk {
            Some(bytes) => {
                let _ = body.flow_control().release_capacity(bytes.len());
                decoder.push(&bytes);
                while let Some(msg) = decoder.next_message()? {
                    messages.push(msg);
                }
            }
            None => break,
        }
    }
    let _ = body.trailers().await;

    let kind_hint = MethodKind::Unary;
    let incoming = IncomingRpc {
        method: path,
        kind: kind_hint,
        metadata,
        messages,
    };
    let reply = handler(incoming);
    write_reply(&mut respond, reply).await
}

async fn write_reply(
    respond: &mut h2::server::SendResponse<Bytes>,
    reply: HandlerReply,
) -> GrpcResult<()> {
    let mut response = Response::builder()
        .status(200)
        .header("content-type", "application/grpc")
        .body(())
        .map_err(|e| GrpcError::new(e.to_string()))?;
    for (k, v) in &reply.headers {
        if let (Ok(name), Ok(val)) = (
            http::HeaderName::from_bytes(k.as_bytes()),
            http::HeaderValue::from_str(v),
        ) {
            response.headers_mut().insert(name, val);
        }
    }

    let mut send = respond
        .send_response(response, false)
        .map_err(|e| GrpcError::new(format!("send response headers: {e}")))?;

    for msg in &reply.messages {
        let framed = frame_message(msg)?;
        send.send_data(framed, false)
            .map_err(|e| GrpcError::new(format!("send response: {e}")))?;
    }

    let trailers = status_trailers(&reply.status);
    send.send_trailers(trailers)
        .map_err(|e| GrpcError::new(format!("send trailers: {e}")))?;
    Ok(())
}

fn headers_to_meta(headers: &http::HeaderMap) -> Metadata {
    let mut out = Metadata::new();
    for (k, v) in headers.iter() {
        if let Ok(val) = v.to_str() {
            out.insert(k.as_str().to_string(), val.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::{CallOptions, Channel};

    #[test]
    fn unary_echo_roundtrip() {
        let server = GrpcServer::bind("127.0.0.1:0").expect("bind");
        let addr = server.addr();
        server
            .register(
                "/test.Echo/Echo",
                MethodKind::Unary,
                Arc::new(|rpc| {
                    let msg = rpc.messages.first().cloned().unwrap_or_default();
                    HandlerReply::ok_bytes(msg)
                }),
            )
            .unwrap();
        server.serve_bg().unwrap();
        std::thread::sleep(Duration::from_millis(50));

        let ch = Channel::connect(&addr, &CallOptions::default()).expect("connect");
        let result = ch
            .unary("/test.Echo/Echo", b"ping", &CallOptions::default())
            .expect("unary");
        assert!(result.status.is_ok());
        assert_eq!(result.bytes, b"ping");

        server.stop();
        server.join_bg();
    }

    #[test]
    fn server_stream_roundtrip() {
        let server = GrpcServer::bind("127.0.0.1:0").expect("bind");
        let addr = server.addr();
        server
            .register(
                "/test.Echo/Stream",
                MethodKind::ServerStream,
                Arc::new(|rpc| {
                    let base = rpc.messages.first().cloned().unwrap_or_default();
                    HandlerReply::ok_messages(vec![base.clone(), base])
                }),
            )
            .unwrap();
        server.serve_bg().unwrap();
        std::thread::sleep(Duration::from_millis(50));

        let ch = Channel::connect(&addr, &CallOptions::default()).unwrap();
        let mut call = ch
            .open_server_stream("/test.Echo/Stream", b"x", &CallOptions::default())
            .unwrap();
        let mut msgs = Vec::new();
        while let Some(m) = call.recv().unwrap() {
            msgs.push(m);
        }
        let finished = call.finish().unwrap();
        assert!(finished.status.is_ok());
        assert_eq!(msgs, vec![b"x".to_vec(), b"x".to_vec()]);

        server.stop();
        server.join_bg();
    }
}
