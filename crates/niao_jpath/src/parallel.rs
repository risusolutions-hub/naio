//! Parallel batch JSONPath / JMESPath over many documents.

use crate::error::JpathResult;
use crate::jmespath::CompiledJmes;
use crate::jsonpath::{find as path_find, search as path_search, CompiledJsonPath};
use niao_parallel::map as parallel_map;
use serde_json::Value;

pub struct ParallelOpts {
    pub threads: usize,
}

impl Default for ParallelOpts {
    fn default() -> Self {
        Self {
            threads: niao_parallel::available_threads(),
        }
    }
}

/// Parallel JSONPath `find` over many documents.
pub fn parallel_find(
    docs: &[Value],
    query: &str,
    opts: &ParallelOpts,
) -> JpathResult<Vec<Vec<Value>>> {
    let compiled = crate::jsonpath::compile(query)?;
    parallel_search(&compiled, docs, opts)
}

/// Parallel search with compiled JSONPath.
pub fn parallel_search(
    compiled: &CompiledJsonPath,
    docs: &[Value],
    opts: &ParallelOpts,
) -> JpathResult<Vec<Vec<Value>>> {
    Ok(parallel_map(docs, opts.threads, |doc| {
        path_search(compiled, doc).unwrap()
    }))
}

/// Parallel JMESPath over many documents.
pub fn parallel_jmes(
    docs: &[Value],
    expression: &str,
    opts: &ParallelOpts,
) -> JpathResult<Vec<Value>> {
    let compiled = crate::jmespath::compile(expression)?;
    parallel_jmes_compiled(&compiled, docs, opts)
}

pub fn parallel_jmes_compiled(
    compiled: &CompiledJmes,
    docs: &[Value],
    opts: &ParallelOpts,
) -> JpathResult<Vec<Value>> {
    Ok(parallel_map(docs, opts.threads, |doc| {
        crate::jmespath::search_with_compiled(compiled, doc).unwrap()
    }))
}

/// Parallel JSONPath find_one.
pub fn parallel_find_one(
    docs: &[Value],
    query: &str,
    opts: &ParallelOpts,
) -> JpathResult<Vec<Value>> {
    Ok(parallel_map(docs, opts.threads, |doc| {
        path_find(doc, query)
            .ok()
            .and_then(|v| v.into_iter().next())
            .unwrap_or(Value::Null)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parallel_find_batch() {
        let docs = vec![json!({"x": 1}), json!({"x": 2}), json!({"y": 3})];
        let out = parallel_find(&docs, "$.x", &ParallelOpts::default()).unwrap();
        assert_eq!(out.len(), 3);
        assert_eq!(out[0], vec![json!(1)]);
        assert_eq!(out[2], Vec::<Value>::new());
    }
}
