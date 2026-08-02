//! Parallel batch rendering.

use crate::engine::{CompiledTemplate, EscapeMode, ViewOpts};
use crate::error::ViewResult;
use niao_parallel;
use serde_json::Value as JsonValue;

/// Render `source` once per context object.
///
/// When `threads == 0`, uses [`niao_parallel::available_threads`].
pub fn batch_render(
    source: &str,
    contexts: &[JsonValue],
    opts: &ViewOpts,
    threads: usize,
) -> ViewResult<Vec<String>> {
    let compiled = CompiledTemplate::compile(source, opts)?;
    batch_compiled(&compiled, contexts, threads)
}

/// Render a compiled template once per context.
pub fn batch_compiled(
    compiled: &CompiledTemplate,
    contexts: &[JsonValue],
    threads: usize,
) -> ViewResult<Vec<String>> {
    let threads = if threads == 0 {
        niao_parallel::available_threads()
    } else {
        threads
    };
    // Compile is shared read-only; each worker renders independently.
    let results = niao_parallel::try_map(contexts, threads, |ctx| compiled.render(ctx))?;
    Ok(results)
}

/// Convenience: force HTML autoescape for batch HTML pages.
pub fn html_opts() -> ViewOpts {
    ViewOpts {
        autoescape: EscapeMode::Html,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn batch_two() {
        let ctxs = vec![json!({"n": 1}), json!({"n": 2})];
        let out = batch_render("{{ n }}", &ctxs, &ViewOpts::default(), 1).unwrap();
        assert_eq!(out, vec!["1".to_string(), "2".to_string()]);
    }
}
