//! Zero-dependency HTTP/1.1 client and server for Niao.

mod client;
mod headers;
mod method;
mod parser;
mod server;
mod status;
mod url;

pub use client::{delete, get, head, post, put, request, ClientOptions, Error, RequestBuilder, Response};
pub use headers::HeaderMap;
pub use method::Method;
pub use parser::{parse_request, parse_response, ParseError, RequestHead, ResponseHead};
pub use server::{IncomingRequest, OutgoingResponse, Server};
pub use status::Status;
pub use url::{form_urlencode, join, parse_url, percent_decode, percent_encode, Url};

#[cfg(test)]
mod integration;
