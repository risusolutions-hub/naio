//! `niao_sign` — signed + expiring tokens for Niao (`nsign` stdlib).
//! itsdangerous-compatible HMAC signing for strings, JSON, cookies, and URLs.

mod cookie;
mod encoding;
mod error;
mod serializer;
mod signer;
mod url;

pub use cookie::{format_set_cookie, sign_cookie_value, unsign_cookie_value};
pub use encoding::{b64_decode, b64_encode, bytes_to_int, int_to_bytes};
pub use error::{SignError, UnsafeLoad};
pub use serializer::{Serializer, SerializerKind, SerializerOptions};
pub use signer::{
    Digest, KeyDerivation, Signer, SignerConfig, TimestampSigner, DEFAULT_MAX_PAYLOAD,
};
pub use url::{default_param, sign_url, unsign_url};
