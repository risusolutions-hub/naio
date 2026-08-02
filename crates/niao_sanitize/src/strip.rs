//! Strip all HTML tags, keeping text content.

use crate::clean::{run_clean, CleanOpts};
use crate::error::SanitizeError;
use std::collections::HashSet;

/// Remove all tags; keep text (~bleach clean with tags=[]).
pub fn strip_tags(html: &str, strip_comments: bool) -> Result<String, SanitizeError> {
    let mut opts = CleanOpts::default();
    opts.tags = Some(HashSet::new());
    opts.tag_attributes = Some(Default::default());
    opts.strip_comments = strip_comments;
    opts.link_rel = None;
    Ok(run_clean(html, &opts))
}
