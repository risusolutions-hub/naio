//! gRPC client/server over HTTP/2 for Niao.
//!
//! Message payloads are raw protobuf bytes (encode/decode with `nproto`).
//! Transport is cleartext HTTP/2 prior-knowledge (h2c).

mod channel;
mod codec;
mod error;
mod metadata;
mod runtime;
mod server;
mod status;

pub use channel::{status_trailers, CallOptions, Channel, ClientCall, RpcResult};
pub use codec::{frame_message, unframe_all, unframe_one, FrameDecoder};
pub use error::{GrpcError, GrpcResult};
pub use metadata::{
    method_path, normalize_metadata, normalize_method_path, parse_method, Metadata,
};
pub use server::{GrpcServer, HandlerReply, IncomingRpc, MethodKind, SyncHandler};
pub use status::{status_from_headers, Status, StatusCode};
