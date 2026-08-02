//! Ergonomic HTTP client for Niao (~requests / httpx).
//!
//! Thin session/cookie/retry/multipart layer over `niao_http`.

mod auth;
mod cookie;
mod error;
mod form;
mod multipart;
mod request;
mod response;
mod session;

pub use auth::{basic_auth, bearer};
pub use cookie::{cookie_header_from_map, parse_set_cookie, Cookie, CookieJar};
pub use error::{ReqError, ReqResult};
pub use form::{decode_form, decode_form_map, encode_form, encode_form_map};
pub use multipart::{build_multipart, random_boundary, MultipartBody, MultipartPart};
pub use request::{default_user_agent, download, execute, form_body, join_url, prepare_url};
pub use response::{from_http, reason_phrase, Response};
pub use session::{RequestOpts, Session, DEFAULT_USER_AGENT};

/// Convenience: GET with a fresh default session.
pub fn get(url: &str, opts: &RequestOpts) -> ReqResult<Response> {
    let mut s = Session::new();
    execute("GET", url, &mut s, opts)
}

/// Convenience: POST.
pub fn post(url: &str, opts: &RequestOpts) -> ReqResult<Response> {
    let mut s = Session::new();
    execute("POST", url, &mut s, opts)
}

/// Convenience: PUT.
pub fn put(url: &str, opts: &RequestOpts) -> ReqResult<Response> {
    let mut s = Session::new();
    execute("PUT", url, &mut s, opts)
}

/// Convenience: PATCH.
pub fn patch(url: &str, opts: &RequestOpts) -> ReqResult<Response> {
    let mut s = Session::new();
    execute("PATCH", url, &mut s, opts)
}

/// Convenience: DELETE.
pub fn delete(url: &str, opts: &RequestOpts) -> ReqResult<Response> {
    let mut s = Session::new();
    execute("DELETE", url, &mut s, opts)
}

/// Convenience: HEAD.
pub fn head(url: &str, opts: &RequestOpts) -> ReqResult<Response> {
    let mut s = Session::new();
    execute("HEAD", url, &mut s, opts)
}

/// Convenience: arbitrary method.
pub fn request(method: &str, url: &str, opts: &RequestOpts) -> ReqResult<Response> {
    let mut s = Session::new();
    execute(method, url, &mut s, opts)
}
