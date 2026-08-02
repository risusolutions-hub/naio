//! Minimal Chrome DevTools Protocol client over WebSocket.

use crate::error::{BrowserError, BrowserResult};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::time::timeout;
use tokio_tungstenite::{client_async, tungstenite::Message};
use url::Url;

type PendingMap = HashMap<u64, oneshot::Sender<BrowserResult<JsonValue>>>;

/// Bidirectional CDP connection.
pub struct CdpConn {
    next_id: AtomicU64,
    pending: Arc<Mutex<PendingMap>>,
    tx: mpsc::UnboundedSender<Message>,
    session_id: Option<String>,
    default_timeout: Duration,
}

impl CdpConn {
    /// Connect to a DevTools WebSocket URL (`ws://127.0.0.1:PORT/...`).
    pub async fn connect(ws_url: &str, request_timeout: Duration) -> BrowserResult<Self> {
        let url = Url::parse(ws_url).map_err(|e| BrowserError::msg(format!("bad ws url: {e}")))?;
        let host = url
            .host_str()
            .ok_or_else(|| BrowserError::msg("ws url missing host"))?;
        let port = url
            .port_or_known_default()
            .ok_or_else(|| BrowserError::msg("ws url missing port"))?;
        let addr = format!("{host}:{port}");
        let stream = timeout(request_timeout, TcpStream::connect(&addr))
            .await
            .map_err(|_| BrowserError::Timeout(format!("tcp connect timed out: {addr}")))?
            .map_err(|e| BrowserError::Connect(e.to_string()))?;
        let (ws, _) = timeout(request_timeout, client_async(ws_url, stream))
            .await
            .map_err(|_| BrowserError::Timeout("websocket handshake timed out".into()))?
            .map_err(|e| BrowserError::Protocol(format!("websocket: {e}")))?;

        let (mut sink, mut reader) = ws.split();
        let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
        let pending: Arc<Mutex<PendingMap>> = Arc::new(Mutex::new(HashMap::new()));
        let pending_r = pending.clone();

        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if sink.send(msg).await.is_err() {
                    break;
                }
            }
        });

        tokio::spawn(async move {
            while let Some(item) = reader.next().await {
                let Ok(Message::Text(text)) = item else {
                    continue;
                };
                let Ok(v) = serde_json::from_str::<JsonValue>(&text) else {
                    continue;
                };
                if let Some(id) = v.get("id").and_then(|x| x.as_u64()) {
                    let result = if let Some(err) = v.get("error") {
                        Err(BrowserError::Protocol(
                            err.get("message")
                                .and_then(|m| m.as_str())
                                .unwrap_or("CDP error")
                                .to_string(),
                        ))
                    } else {
                        Ok(v.get("result").cloned().unwrap_or(JsonValue::Null))
                    };
                    if let Some(resp_tx) = pending_r.lock().await.remove(&id) {
                        let _ = resp_tx.send(result);
                    }
                }
            }
        });

        Ok(Self {
            next_id: AtomicU64::new(1),
            pending,
            tx,
            session_id: None,
            default_timeout: request_timeout,
        })
    }

    /// Send a CDP command optionally scoped to a flat target session.
    pub async fn call_session(
        &self,
        session_id: Option<&str>,
        method: &str,
        params: JsonValue,
    ) -> BrowserResult<JsonValue> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut msg = json!({
            "id": id,
            "method": method,
            "params": params,
        });
        let sid = session_id.or(self.session_id.as_deref());
        if let Some(sid) = sid {
            msg.as_object_mut()
                .unwrap()
                .insert("sessionId".into(), JsonValue::String(sid.to_string()));
        }
        let (resp_tx, resp_rx) = oneshot::channel();
        self.pending.lock().await.insert(id, resp_tx);
        let text = serde_json::to_string(&msg)
            .map_err(|e| BrowserError::msg(format!("serialize: {e}")))?;
        self.tx
            .send(Message::Text(text.into()))
            .map_err(|_| BrowserError::Protocol("CDP connection closed".into()))?;
        match timeout(self.default_timeout, resp_rx).await {
            Ok(Ok(r)) => r,
            Ok(Err(_)) => Err(BrowserError::Protocol("CDP response channel closed".into())),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err(BrowserError::Timeout(format!(
                    "CDP {method} timed out after {}ms",
                    self.default_timeout.as_millis()
                )))
            }
        }
    }

    /// Send a CDP command and wait for the matching response.
    pub async fn call(&self, method: &str, params: JsonValue) -> BrowserResult<JsonValue> {
        self.call_session(None, method, params).await
    }

    /// Discover the browser WebSocket debugger URL from an HTTP endpoint.
    pub async fn discover_ws(http_endpoint: &str, wait: Duration) -> BrowserResult<String> {
        let base = http_endpoint.trim_end_matches('/');
        let url = if base.starts_with("http://") || base.starts_with("https://") {
            format!("{base}/json/version")
        } else {
            format!("http://{base}/json/version")
        };
        let parsed = Url::parse(&url).map_err(|e| BrowserError::msg(e.to_string()))?;
        let host = parsed.host_str().unwrap_or("127.0.0.1");
        let port = parsed.port_or_known_default().unwrap_or(80);
        let path = parsed.path();
        let req =
            format!("GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n");
        let stream = timeout(wait, TcpStream::connect(format!("{host}:{port}")))
            .await
            .map_err(|_| BrowserError::Timeout("discover connect timed out".into()))?
            .map_err(|e| BrowserError::Connect(e.to_string()))?;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = stream;
        stream
            .write_all(req.as_bytes())
            .await
            .map_err(|e| BrowserError::Io(e.to_string()))?;
        let mut buf = Vec::new();
        stream
            .read_to_end(&mut buf)
            .await
            .map_err(|e| BrowserError::Io(e.to_string()))?;
        let text = String::from_utf8_lossy(&buf);
        let body = text
            .split("\r\n\r\n")
            .nth(1)
            .ok_or_else(|| BrowserError::Protocol("empty /json/version response".into()))?;
        let v: JsonValue = serde_json::from_str(body)
            .map_err(|e| BrowserError::Protocol(format!("json/version: {e}")))?;
        v.get("webSocketDebuggerUrl")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| BrowserError::Protocol("webSocketDebuggerUrl missing".into()))
    }
}
