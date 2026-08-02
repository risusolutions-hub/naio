//! Client channel over HTTP/2 (cleartext h2c prior-knowledge).

use crate::codec::{frame_message, FrameDecoder};
use crate::error::{GrpcError, GrpcResult};
use crate::metadata::{normalize_method_path, Metadata};
use crate::runtime::{block_on, runtime};
use crate::status::{percent_encode, status_from_headers, Status};
use bytes::Bytes;
use h2::client::{ResponseFuture, SendRequest};
use h2::RecvStream;
use http::{HeaderMap, HeaderName, HeaderValue, Method, Request};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Options for opening a channel / RPC.
#[derive(Debug, Clone, Default)]
pub struct CallOptions {
    pub headers: Metadata,
    pub timeout: Option<Duration>,
    pub authority: Option<String>,
}

/// Result of a finished RPC.
#[derive(Debug, Clone)]
pub struct RpcResult {
    pub status: Status,
    pub bytes: Vec<u8>,
    pub messages: Vec<Vec<u8>>,
    pub headers: Metadata,
    pub trailers: Metadata,
}

struct ChannelInner {
    target: String,
    authority: String,
    sender: Mutex<SendRequest<Bytes>>,
}

/// Persistent HTTP/2 client channel.
#[derive(Clone)]
pub struct Channel {
    inner: Arc<ChannelInner>,
}

impl Channel {
    /// Connect with HTTP/2 prior knowledge (h2c) to `host:port`.
    pub fn connect(target: &str, opts: &CallOptions) -> GrpcResult<Self> {
        let target = target.trim();
        if target.is_empty() {
            return Err(GrpcError::new("channel target must be non-empty"));
        }
        let authority = opts.authority.clone().unwrap_or_else(|| target.to_string());

        block_on(async {
            let tcp = tokio::net::TcpStream::connect(target)
                .await
                .map_err(|e| GrpcError::new(format!("connect {target}: {e}")))?;
            tcp.set_nodelay(true).ok();
            let (sender, conn) = h2::client::handshake(tcp)
                .await
                .map_err(|e| GrpcError::new(format!("h2 handshake: {e}")))?;
            runtime().spawn(async move {
                let _ = conn.await;
            });
            Ok(Self {
                inner: Arc::new(ChannelInner {
                    target: target.to_string(),
                    authority,
                    sender: Mutex::new(sender),
                }),
            })
        })
    }

    pub fn target(&self) -> &str {
        &self.inner.target
    }

    async fn take_sender_async(&self) -> GrpcResult<SendRequest<Bytes>> {
        let mut guard = self
            .inner
            .sender
            .lock()
            .map_err(|_| GrpcError::new("channel lock poisoned"))?;
        std::future::poll_fn(|cx| guard.poll_ready(cx))
            .await
            .map_err(|e| GrpcError::new(format!("channel not ready: {e}")))?;
        Ok(guard.clone())
    }

    /// Unary RPC: one request message, one response message.
    pub fn unary(&self, method: &str, request: &[u8], opts: &CallOptions) -> GrpcResult<RpcResult> {
        let method = normalize_method_path(method)?;
        let framed = frame_message(request)?;
        block_on(async {
            let mut sender = self.take_sender_async().await?;
            let (response_future, mut send_stream) =
                start_call(&mut sender, &self.inner.authority, &method, opts)?;
            send_stream
                .send_data(framed, true)
                .map_err(|e| GrpcError::new(format!("send request: {e}")))?;
            read_full_response(response_future, opts.timeout).await
        })
    }

    /// Open a server-streaming call (one request, many responses).
    pub fn open_server_stream(
        &self,
        method: &str,
        request: &[u8],
        opts: &CallOptions,
    ) -> GrpcResult<ClientCall> {
        let method = normalize_method_path(method)?;
        let framed = frame_message(request)?;
        block_on(async {
            let mut sender = self.take_sender_async().await?;
            let (response_future, mut send_stream) =
                start_call(&mut sender, &self.inner.authority, &method, opts)?;
            send_stream
                .send_data(framed, true)
                .map_err(|e| GrpcError::new(format!("send request: {e}")))?;
            Ok(ClientCall::new(
                None,
                Some(response_future),
                opts.timeout,
                true,
            ))
        })
    }

    /// Open a client-streaming call (many requests, one response).
    pub fn open_client_stream(&self, method: &str, opts: &CallOptions) -> GrpcResult<ClientCall> {
        let method = normalize_method_path(method)?;
        block_on(async {
            let mut sender = self.take_sender_async().await?;
            let (response_future, send_stream) =
                start_call(&mut sender, &self.inner.authority, &method, opts)?;
            Ok(ClientCall::new(
                Some(send_stream),
                Some(response_future),
                opts.timeout,
                false,
            ))
        })
    }

    /// Open a bidi-streaming call.
    pub fn open_bidi(&self, method: &str, opts: &CallOptions) -> GrpcResult<ClientCall> {
        let method = normalize_method_path(method)?;
        block_on(async {
            let mut sender = self.take_sender_async().await?;
            let (response_future, send_stream) =
                start_call(&mut sender, &self.inner.authority, &method, opts)?;
            Ok(ClientCall::new(
                Some(send_stream),
                Some(response_future),
                opts.timeout,
                false,
            ))
        })
    }
}

fn start_call(
    sender: &mut SendRequest<Bytes>,
    authority: &str,
    method: &str,
    opts: &CallOptions,
) -> GrpcResult<(ResponseFuture, h2::SendStream<Bytes>)> {
    let uri = format!("http://{authority}{method}");
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header("content-type", "application/grpc")
        .header("te", "trailers")
        .header("user-agent", "ngrpc/0.1");

    for (k, v) in &opts.headers {
        let name = HeaderName::from_bytes(k.as_bytes())
            .map_err(|e| GrpcError::new(format!("invalid header name {k}: {e}")))?;
        let value = HeaderValue::from_str(v)
            .map_err(|e| GrpcError::new(format!("invalid header value for {k}: {e}")))?;
        builder = builder.header(name, value);
    }

    if let Some(timeout) = opts.timeout {
        let millis = timeout.as_millis();
        builder = builder.header("grpc-timeout", format!("{millis}m"));
    }

    let request = builder
        .body(())
        .map_err(|e| GrpcError::new(format!("build request: {e}")))?;

    sender
        .send_request(request, false)
        .map_err(|e| GrpcError::new(format!("open stream: {e}")))
}

async fn read_full_response(
    response_future: ResponseFuture,
    timeout: Option<Duration>,
) -> GrpcResult<RpcResult> {
    let response = await_response(response_future, timeout).await?;
    let headers = headers_to_meta(response.headers());
    let mut body = response.into_body();
    let mut decoder = FrameDecoder::new();
    let mut messages = Vec::new();
    let mut trailers_map = Metadata::new();

    loop {
        let chunk = body
            .data()
            .await
            .transpose()
            .map_err(|e| GrpcError::new(format!("read body: {e}")))?;
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

    if let Some(trailers) = body
        .trailers()
        .await
        .map_err(|e| GrpcError::new(format!("read trailers: {e}")))?
    {
        trailers_map = headers_to_meta(&trailers);
    }

    let status = status_from_merged(&headers, &trailers_map)?;
    let bytes = messages.first().cloned().unwrap_or_default();
    Ok(RpcResult {
        status,
        bytes,
        messages,
        headers,
        trailers: trailers_map,
    })
}

async fn await_response(
    response_future: ResponseFuture,
    timeout: Option<Duration>,
) -> GrpcResult<http::Response<RecvStream>> {
    match timeout {
        Some(t) => tokio::time::timeout(t, response_future)
            .await
            .map_err(|_| GrpcError::new("RPC deadline exceeded"))?
            .map_err(|e| GrpcError::new(format!("response headers: {e}"))),
        None => response_future
            .await
            .map_err(|e| GrpcError::new(format!("response headers: {e}"))),
    }
}

fn headers_to_meta(headers: &HeaderMap) -> Metadata {
    let mut out = Metadata::new();
    for (k, v) in headers.iter() {
        if let Ok(val) = v.to_str() {
            out.insert(k.as_str().to_string(), val.to_string());
        }
    }
    out
}

fn status_from_merged(headers: &Metadata, trailers: &Metadata) -> GrpcResult<Status> {
    let mut pairs: Vec<(&str, &str)> = Vec::new();
    for (k, v) in headers {
        pairs.push((k.as_str(), v.as_str()));
    }
    for (k, v) in trailers {
        pairs.push((k.as_str(), v.as_str()));
    }
    status_from_headers(pairs)
}

struct ActiveRecv {
    body: RecvStream,
    decoder: FrameDecoder,
    headers: Metadata,
    trailers: Metadata,
    status: Option<Status>,
    ended: bool,
}

/// Streaming client call handle.
pub struct ClientCall {
    send: Option<h2::SendStream<Bytes>>,
    pending: Option<ResponseFuture>,
    active: Option<ActiveRecv>,
    send_closed: bool,
    timeout: Option<Duration>,
}

impl ClientCall {
    fn new(
        send: Option<h2::SendStream<Bytes>>,
        pending: Option<ResponseFuture>,
        timeout: Option<Duration>,
        send_closed: bool,
    ) -> Self {
        Self {
            send,
            pending,
            active: None,
            send_closed,
            timeout,
        }
    }

    pub fn send(&mut self, payload: &[u8]) -> GrpcResult<()> {
        if self.send_closed {
            return Err(GrpcError::new("send on half-closed call"));
        }
        let framed = frame_message(payload)?;
        let send = self
            .send
            .as_mut()
            .ok_or_else(|| GrpcError::new("call has no send stream"))?;
        send.send_data(framed, false)
            .map_err(|e| GrpcError::new(format!("send: {e}")))
    }

    pub fn send_close(&mut self) -> GrpcResult<()> {
        if self.send_closed {
            return Ok(());
        }
        if let Some(send) = self.send.as_mut() {
            send.send_data(Bytes::new(), true)
                .map_err(|e| GrpcError::new(format!("send_close: {e}")))?;
        }
        self.send_closed = true;
        Ok(())
    }

    fn ensure_recv_active(&mut self) -> GrpcResult<()> {
        if self.active.is_some() {
            return Ok(());
        }
        let fut = self
            .pending
            .take()
            .ok_or_else(|| GrpcError::new("call has no response stream"))?;
        let response = block_on(await_response(fut, self.timeout))?;
        let headers = headers_to_meta(response.headers());
        let body = response.into_body();
        self.active = Some(ActiveRecv {
            body,
            decoder: FrameDecoder::new(),
            headers,
            trailers: Metadata::new(),
            status: None,
            ended: false,
        });
        Ok(())
    }

    pub fn recv(&mut self) -> GrpcResult<Option<Vec<u8>>> {
        self.ensure_recv_active()?;
        let timeout = self.timeout;
        let state = self
            .active
            .as_mut()
            .ok_or_else(|| GrpcError::new("call has no response stream"))?;
        if let Some(msg) = state.decoder.next_message()? {
            return Ok(Some(msg));
        }
        if state.ended {
            return Ok(None);
        }
        block_on(async {
            loop {
                let chunk = match timeout {
                    Some(t) => tokio::time::timeout(t, state.body.data())
                        .await
                        .map_err(|_| GrpcError::new("RPC deadline exceeded"))?
                        .transpose()
                        .map_err(|e| GrpcError::new(format!("read body: {e}")))?,
                    None => state
                        .body
                        .data()
                        .await
                        .transpose()
                        .map_err(|e| GrpcError::new(format!("read body: {e}")))?,
                };
                match chunk {
                    Some(bytes) => {
                        let _ = state.body.flow_control().release_capacity(bytes.len());
                        state.decoder.push(&bytes);
                        if let Some(msg) = state.decoder.next_message()? {
                            return Ok(Some(msg));
                        }
                    }
                    None => {
                        if let Some(tr) = state
                            .body
                            .trailers()
                            .await
                            .map_err(|e| GrpcError::new(format!("trailers: {e}")))?
                        {
                            state.trailers = headers_to_meta(&tr);
                        }
                        state.status = Some(status_from_merged(&state.headers, &state.trailers)?);
                        state.ended = true;
                        return Ok(None);
                    }
                }
            }
        })
    }

    pub fn finish(&mut self) -> GrpcResult<RpcResult> {
        let mut messages = Vec::new();
        while let Some(msg) = self.recv()? {
            messages.push(msg);
        }
        let state = self
            .active
            .as_ref()
            .ok_or_else(|| GrpcError::new("call finished before response"))?;
        let st = state.status.clone().unwrap_or_else(Status::ok);
        let bytes = messages.first().cloned().unwrap_or_default();
        Ok(RpcResult {
            status: st,
            bytes,
            messages,
            headers: state.headers.clone(),
            trailers: state.trailers.clone(),
        })
    }
}

/// Build trailers for a status.
pub fn status_trailers(status: &Status) -> HeaderMap {
    let mut map = HeaderMap::new();
    map.insert(
        HeaderName::from_static("grpc-status"),
        HeaderValue::from_str(&status.code.as_i32().to_string()).expect("status digit"),
    );
    if !status.message.is_empty() {
        if let Ok(v) = HeaderValue::from_str(&percent_encode(&status.message)) {
            map.insert(HeaderName::from_static("grpc-message"), v);
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_trailers_ok() {
        let t = status_trailers(&Status::ok());
        assert_eq!(t.get("grpc-status").unwrap(), "0");
    }
}
