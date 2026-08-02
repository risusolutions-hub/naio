use niao_sanitize::{clean, strip_tags as san_strip, CleanOpts};

/// Fast single-pass HTML tag strip (no DOM).
///
/// >>> use niao_feed::strip_html;
/// >>> strip_html("<p>hi</p>")
/// "hi"
pub fn strip_html(html: &str) -> String {
    san_strip(html, true).unwrap_or_else(|_| html.to_string())
}

/// XSS-safe HTML cleanup for feed descriptions (allowlist policy).
///
/// >>> use niao_feed::sanitize_html;
/// >>> sanitize_html("<script>x</script><p>ok</p>", None).unwrap().contains("ok")
/// true
pub fn sanitize_html(
    html: &str,
    opts: Option<&CleanOpts>,
) -> Result<String, niao_sanitize::SanitizeError> {
    let opts = opts.cloned().unwrap_or_default();
    clean(html, &opts)
}
