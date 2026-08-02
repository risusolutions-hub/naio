//! Core Jinja-style template engine (thin wrapper over minijinja).

use crate::error::{ViewError, ViewResult};
use minijinja::{AutoEscape, Environment};
use serde_json::Value as JsonValue;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// Auto-escaping policy for rendered output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EscapeMode {
    /// Never auto-escape.
    None,
    /// Always HTML-escape interpolated values.
    Html,
    /// Escape based on template name extension (`.html`/`.htm`/`.xml` → HTML).
    #[default]
    Auto,
}

/// Options applied when creating an environment or compiling a template.
#[derive(Debug, Clone, Default)]
pub struct ViewOpts {
    /// Auto-escape mode (default [`EscapeMode::Auto`]).
    pub autoescape: EscapeMode,
    /// Preserve a trailing newline on templates.
    pub keep_trailing_newline: bool,
    /// Trim first newline after blocks.
    pub trim_blocks: bool,
    /// Strip leading whitespace before blocks.
    pub lstrip_blocks: bool,
}

/// Built-in filter names exposed by the engine (minijinja defaults + aliases).
pub const BUILTIN_FILTERS: &[&str] = &[
    "safe",
    "escape",
    "e",
    "lower",
    "upper",
    "title",
    "capitalize",
    "replace",
    "length",
    "count",
    "dictsort",
    "items",
    "reverse",
    "trim",
    "join",
    "split",
    "lines",
    "default",
    "d",
    "round",
    "abs",
    "int",
    "float",
    "attr",
    "first",
    "last",
    "min",
    "max",
    "sort",
    "list",
    "string",
    "bool",
    "batch",
    "slice",
    "sum",
    "indent",
    "select",
    "reject",
    "selectattr",
    "rejectattr",
    "map",
    "groupby",
    "unique",
    "chain",
    "zip",
    "pprint",
    "format",
];

fn apply_opts(env: &mut Environment<'static>, opts: &ViewOpts) {
    env.set_keep_trailing_newline(opts.keep_trailing_newline);
    env.set_trim_blocks(opts.trim_blocks);
    env.set_lstrip_blocks(opts.lstrip_blocks);
    match opts.autoescape {
        EscapeMode::None => {
            env.set_auto_escape_callback(|_| AutoEscape::None);
        }
        EscapeMode::Html => {
            env.set_auto_escape_callback(|_| AutoEscape::Html);
        }
        EscapeMode::Auto => {
            env.set_auto_escape_callback(minijinja::default_auto_escape_callback);
        }
    }
}

fn new_env(opts: &ViewOpts) -> Environment<'static> {
    let mut env = Environment::new();
    apply_opts(&mut env, opts);
    env
}

fn anon_name(opts: &ViewOpts) -> &'static str {
    match opts.autoescape {
        EscapeMode::Html => "__nview__.html",
        EscapeMode::None => "__nview__.txt",
        EscapeMode::Auto => "__nview__.html",
    }
}

/// Multi-template environment supporting `{% extends %}` / `{% include %}` / blocks.
pub struct ViewEnv {
    env: Environment<'static>,
    opts: ViewOpts,
}

impl ViewEnv {
    /// Create an empty environment.
    pub fn new(opts: ViewOpts) -> Self {
        Self {
            env: new_env(&opts),
            opts,
        }
    }

    /// Options used to construct this environment.
    pub fn opts(&self) -> &ViewOpts {
        &self.opts
    }

    /// Register or replace a named template source.
    pub fn add(&mut self, name: &str, source: &str) -> ViewResult<()> {
        self.env
            .add_template_owned(name.to_string(), source.to_string())
            .map_err(ViewError::from)
    }

    /// Whether a named template is registered.
    pub fn has(&self, name: &str) -> bool {
        self.env.get_template(name).is_ok()
    }

    /// Sorted list of registered template names.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.env.templates().map(|(n, _)| n.to_string()).collect();
        names.sort();
        names
    }

    /// Remove a named template. Returns whether it existed.
    pub fn remove(&mut self, name: &str) -> bool {
        let existed = self.has(name);
        self.env.remove_template(name);
        existed
    }

    /// Render a registered template by name.
    pub fn render_named(&self, name: &str, ctx: &JsonValue) -> ViewResult<String> {
        let tmpl = self.env.get_template(name).map_err(ViewError::from)?;
        tmpl.render(ctx).map_err(ViewError::from)
    }

    /// Render an anonymous source string with this environment's templates available
    /// (so `{% extends %}` / `{% include %}` resolve against registered names).
    pub fn render_in(&self, source: &str, ctx: &JsonValue) -> ViewResult<String> {
        let name = anon_name(&self.opts);
        // Temporary add — clone env to avoid mutating shared state mid-render races.
        let mut tmp = new_env(&self.opts);
        for (n, t) in self.env.templates() {
            tmp.add_template_owned(n.to_string(), t.source().to_string())
                .map_err(ViewError::from)?;
        }
        tmp.add_template_owned(name.to_string(), source.to_string())
            .map_err(ViewError::from)?;
        let tmpl = tmp.get_template(name).map_err(ViewError::from)?;
        tmpl.render(ctx).map_err(ViewError::from)
    }

    /// Load a file into the environment under `name`.
    pub fn add_file(&mut self, name: &str, path: &Path) -> ViewResult<()> {
        let source = fs::read_to_string(path).map_err(ViewError::from)?;
        self.add(name, &source)
    }

    /// Load templates from a directory (non-recursive).
    ///
    /// Files with extensions `html`, `htm`, `j2`, `jinja`, `jinja2`, `txt` are loaded.
    /// Template name defaults to the file name (with extension).
    pub fn load_dir(&mut self, dir: &Path) -> ViewResult<usize> {
        let mut count = 0usize;
        let entries = fs::read_dir(dir).map_err(ViewError::from)?;
        for entry in entries {
            let entry = entry.map_err(ViewError::from)?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !matches!(
                ext.as_str(),
                "html" | "htm" | "j2" | "jinja" | "jinja2" | "txt"
            ) {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| ViewError::io("invalid template file name"))?
                .to_string();
            self.add_file(&name, &path)?;
            count += 1;
        }
        Ok(count)
    }
}

/// A compiled single-template handle (owns its own environment).
pub struct CompiledTemplate {
    env: Environment<'static>,
    name: String,
}

impl CompiledTemplate {
    /// Compile `source` under the given options.
    pub fn compile(source: &str, opts: &ViewOpts) -> ViewResult<Self> {
        let name = anon_name(opts).to_string();
        let mut env = new_env(opts);
        env.add_template_owned(name.clone(), source.to_string())
            .map_err(ViewError::from)?;
        Ok(Self { env, name })
    }

    /// Render with a JSON-like context object.
    pub fn render(&self, ctx: &JsonValue) -> ViewResult<String> {
        let tmpl = self.env.get_template(&self.name).map_err(ViewError::from)?;
        tmpl.render(ctx).map_err(ViewError::from)
    }

    /// Undeclared top-level variable names referenced by the template.
    pub fn vars(&self) -> Vec<String> {
        match self.env.get_template(&self.name) {
            Ok(tmpl) => {
                let set = tmpl.undeclared_variables(false);
                let mut v: Vec<String> = set.into_iter().collect();
                v.sort();
                v
            }
            Err(_) => Vec::new(),
        }
    }
}

/// One-shot render of a template string.
pub fn render(source: &str, ctx: &JsonValue, opts: &ViewOpts) -> ViewResult<String> {
    CompiledTemplate::compile(source, opts)?.render(ctx)
}

/// Render a template loaded from `path`.
pub fn render_file(path: &Path, ctx: &JsonValue, opts: &ViewOpts) -> ViewResult<String> {
    let source = fs::read_to_string(path).map_err(ViewError::from)?;
    // Prefer Auto based on filename when caller left Auto.
    let mut local = opts.clone();
    if matches!(opts.autoescape, EscapeMode::Auto) {
        // Name the template after the file so Auto picks the right escape mode.
        let mut env = new_env(&local);
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("__file__.html")
            .to_string();
        env.add_template_owned(name.clone(), source)
            .map_err(ViewError::from)?;
        let tmpl = env.get_template(&name).map_err(ViewError::from)?;
        return tmpl.render(ctx).map_err(ViewError::from);
    }
    let _ = &mut local;
    render(&source, ctx, &local)
}

/// Return whether `source` parses successfully.
pub fn valid(source: &str) -> bool {
    CompiledTemplate::compile(source, &ViewOpts::default()).is_ok()
}

/// Undeclared top-level variables in `source`.
pub fn vars(source: &str) -> ViewResult<Vec<String>> {
    Ok(CompiledTemplate::compile(source, &ViewOpts::default())?.vars())
}

/// Built-in filter name list (sorted).
pub fn filters() -> Vec<String> {
    let set: BTreeSet<&str> = BUILTIN_FILTERS.iter().copied().collect();
    set.into_iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn render_hello() {
        let out = render(
            "Hello {{ name }}!",
            &json!({"name": "Ada"}),
            &ViewOpts::default(),
        )
        .unwrap();
        assert_eq!(out, "Hello Ada!");
    }

    #[test]
    fn autoescape_html() {
        let opts = ViewOpts {
            autoescape: EscapeMode::Html,
            ..Default::default()
        };
        let out = render("{{ x }}", &json!({"x": "<b>"}), &opts).unwrap();
        assert_eq!(out, "&lt;b&gt;");
    }

    #[test]
    fn inheritance() {
        let mut env = ViewEnv::new(ViewOpts {
            autoescape: EscapeMode::None,
            ..Default::default()
        });
        env.add("base.html", "{% block body %}default{% endblock %}")
            .unwrap();
        env.add(
            "child.html",
            "{% extends \"base.html\" %}{% block body %}child{% endblock %}",
        )
        .unwrap();
        let out = env.render_named("child.html", &json!({})).unwrap();
        assert_eq!(out, "child");
    }

    #[test]
    fn include_partial() {
        let mut env = ViewEnv::new(ViewOpts {
            autoescape: EscapeMode::None,
            ..Default::default()
        });
        env.add("partial.html", "P={{ v }}").unwrap();
        let out = env
            .render_in("{% include \"partial.html\" %}", &json!({"v": 1}))
            .unwrap();
        assert_eq!(out, "P=1");
    }

    #[test]
    fn filters_and_vars() {
        let names = filters();
        assert!(names.iter().any(|n| n == "upper"));
        let v = vars("{{ a }} {{ b|upper }}").unwrap();
        assert!(v.contains(&"a".to_string()));
        assert!(v.contains(&"b".to_string()));
    }

    #[test]
    fn invalid_syntax() {
        assert!(!valid("{% if %}"));
    }
}
