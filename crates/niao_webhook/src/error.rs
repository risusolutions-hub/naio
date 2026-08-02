//! Webhook errors (domain layer — mapped to Niao E-codes at the VM boundary).

use std::fmt;

/// Domain error for webhook sign/verify operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebhookError {
    EmptySecret,
    InvalidSecret(String),
    MissingHeaders,
    InvalidTimestamp,
    TimestampTooOld,
    TimestampTooNew,
    NoMatchingSignature,
    InvalidSignatureHeader,
    InvalidJson(String),
    EmptyPayload,
    BadArgument(String),
}

impl fmt::Display for WebhookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptySecret => write!(f, "webhook secret may not be empty"),
            Self::InvalidSecret(m) => write!(f, "invalid webhook secret: {m}"),
            Self::MissingHeaders => write!(f, "missing required webhook headers"),
            Self::InvalidTimestamp => write!(f, "invalid webhook timestamp"),
            Self::TimestampTooOld => write!(f, "message timestamp too old"),
            Self::TimestampTooNew => write!(f, "message timestamp too new"),
            Self::NoMatchingSignature => write!(f, "no matching signature found"),
            Self::InvalidSignatureHeader => write!(f, "invalid signature header"),
            Self::InvalidJson(m) => write!(f, "invalid webhook JSON payload: {m}"),
            Self::EmptyPayload => write!(f, "webhook payload is empty"),
            Self::BadArgument(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for WebhookError {}

pub type WebhookResult<T> = Result<T, WebhookError>;
