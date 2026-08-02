//! Allowlist HTML sanitizer for user content (XSS-safe), URL scheme policy.

mod clean;
mod error;
mod escape;
mod linkify;
mod parallel;
mod policy;
mod strip;

pub use clean::{clean, clean_text, is_html, CleanOpts, Sanitizer};
pub use error::SanitizeError;
pub use escape::{escape_attr, escape_html};
pub use linkify::{linkify, LinkifyOpts};
pub use parallel::{parallel_clean, parallel_clean_once};
pub use policy::{
    allowed_url, default_protocols, default_tag_attributes, default_tags, RelativeUrlMode,
};
pub use strip::strip_tags;

/// Maximum input size (16 MiB) — matches other text-processing libs.
pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;

pub fn check_input_len(len: usize) -> Result<(), SanitizeError> {
    if len > MAX_INPUT_BYTES {
        return Err(SanitizeError::new(format!(
            "input size {len} exceeds limit {MAX_INPUT_BYTES}"
        )));
    }
    Ok(())
}
