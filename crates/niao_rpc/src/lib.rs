//! `niao_rpc` — JSON-RPC 2.0 protocol engine for the Niao `nrpc` standard library.
//!
//! Supports message codec, NDJSON / Content-Length framing, method dispatch
//! (~jsonrpcserver), and sync stdio / TCP / HTTP transports.
//!
//! ```
//! use niao_rpc::{decode, encode, Id, Message, Request};
//!
//! let msg = Message::Request(Request::call("add", None, Id::Number(1)));
//! let text = encode(&msg);
//! let back = decode(&text).unwrap();
//! assert!(back.is_batch() == false);
//! ```

mod codec;
mod dispatch;
mod error;
mod frame;
mod message;
mod transport;

pub use codec::{decode, decode_raw, encode, encode_batch_values, encode_value, valid, MAX_BYTES};
pub use dispatch::{
    dispatch_request, dispatch_request_value, dispatch_str, dispatch_value, MethodResult,
    MethodTable,
};
pub use error::{codes, EngineError, RpcError};
pub use frame::{frame, frame_text, unframe, FrameStyle, UnframeResult};
pub use message::{
    invalid_request_response, parse_error_response, parse_message_value, parse_request_value,
    parse_response_value, Id, Message, Request, Response, ResponseBody,
};
pub use transport::{
    handle_payload, http_call, http_serve_once, serve_stream, stdio_exchange, tcp_call,
    tcp_serve_once, TransportOptions,
};
