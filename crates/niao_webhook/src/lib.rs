//! `niao_webhook` — Standard Webhooks HMAC sign/verify for Niao (`nwebhook`).
//! Symmetric `v1` signatures (~svix / standard-webhooks subset).

mod error;
mod replay;
mod secret;
mod timestamp;
mod webhook;

#[cfg(test)]
mod conformance;

pub use error::{WebhookError, WebhookResult};
pub use replay::ReplayGuard;
pub use secret::{
    encode_secret, parse_secret, SecretFormat, DEFAULT_TOLERANCE_SECS, HDR_ID, HDR_SIGNATURE,
    HDR_TIMESTAMP, SECRET_PREFIX,
};
pub use timestamp::{check_timestamp, now_secs, parse_timestamp, verify_timestamp_header};
pub use webhook::{
    make_headers, new_msg_id, sign_request, SignRequest, Verified, VerifyOptions, Webhook,
    WebhookOptions,
};
