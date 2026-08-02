//! JMESPath queries (~jmespath Python subset).

use crate::error::{JpathError, JpathResult};
use jmespath::{self, Expression, JmespathError, Rcvar, ToJmespath};
use serde_json::Value;

pub struct CompiledJmes {
    expr: Expression<'static>,
    source: String,
}

impl CompiledJmes {
    pub fn expression(&self) -> &str {
        &self.source
    }
}

fn map_jmes_compile_err(e: JmespathError) -> JpathError {
    JpathError::InvalidJmesPath(e.to_string())
}

fn to_jmes_var(doc: &Value) -> JpathResult<Rcvar> {
    doc.to_jmespath()
        .map_err(|e| JpathError::TypeMismatch(e.to_string()))
}

fn from_jmes_var(v: Rcvar) -> Value {
    serde_json::to_value(v.as_ref()).unwrap_or(Value::Null)
}

/// True when expression parses.
///
/// >>> njpath.jmes_valid("foo.bar | length(@)")
/// true
pub fn valid(expression: &str) -> bool {
    jmespath::compile(expression).is_ok()
}

/// Compile JMESPath expression.
pub fn compile(expression: &str) -> JpathResult<CompiledJmes> {
    let expr = jmespath::compile(expression).map_err(map_jmes_compile_err)?;
    Ok(CompiledJmes {
        expr,
        source: expression.to_string(),
    })
}

/// Search document with JMESPath expression.
///
/// >>> njpath.jmes({"foo": {"bar": [1, 2, 3]}}, "foo.bar | sum(@)")
/// 6
pub fn search(doc: &Value, expression: &str) -> JpathResult<Value> {
    let expr = jmespath::compile(expression).map_err(map_jmes_compile_err)?;
    search_compiled(&expr, doc)
}

pub fn search_compiled(expr: &Expression<'_>, doc: &Value) -> JpathResult<Value> {
    let var = to_jmes_var(doc)?;
    let result = expr.search(var).map_err(map_jmes_compile_err)?;
    Ok(from_jmes_var(result))
}

pub fn search_with_compiled(compiled: &CompiledJmes, doc: &Value) -> JpathResult<Value> {
    search_compiled(&compiled.expr, doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn basic_search() {
        let doc = json!({"foo": {"bar": {"baz": true}}});
        assert_eq!(search(&doc, "foo.bar.baz").unwrap(), json!(true));
    }

    #[test]
    fn pipe_projection() {
        let doc = json!({"items": [{"n": 1}, {"n": 2}, {"n": 3}]});
        let out = search(&doc, "items[*].n | sum(@)").unwrap();
        assert_eq!(out, json!(6.0));
    }

    #[test]
    fn valid_expr() {
        assert!(valid("a.b"));
        assert!(!valid("a.."));
    }
}
