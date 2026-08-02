//! `niao_otp` — high-performance HOTP/TOTP for Niao (`notp` stdlib).

mod base32;
mod digest;
mod error;
mod hotp;
mod totp;
mod uri;

pub use base32::{decode as base32_decode, encode as base32_encode, random_base32};
pub use digest::{Digest, DEFAULT_DIGEST};
pub use error::OtpError;
pub use hotp::{
    generate_hotp, hotp_at, hotp_at_bulk, verify_token, Hotp, DEFAULT_DIGITS, MAX_DIGITS,
    MIN_DIGITS,
};
pub use totp::{totp_at, totp_at_bulk, Totp, DEFAULT_INTERVAL};
pub use uri::{build_hotp_uri, build_totp_uri, parse_uri, OtpKind, ParsedOtp};
