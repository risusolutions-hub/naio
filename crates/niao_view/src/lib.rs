//! Jinja-style templating for Niao: inheritance, blocks, filters, autoescape, partials.
//! (~jinja2 subset via minijinja — distinct from ntemplate's LLM prompt templates)

mod batch;
mod engine;
mod error;
mod escape;

pub use batch::{batch_compiled, batch_render, html_opts};
pub use engine::{
    filters, render, render_file, valid, vars, CompiledTemplate, EscapeMode, ViewEnv, ViewOpts,
    BUILTIN_FILTERS,
};
pub use error::{ViewError, ViewErrorKind, ViewResult};
pub use escape::{escape, escape_attr, unescape};
