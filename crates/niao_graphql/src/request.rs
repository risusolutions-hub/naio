//! Client request body helpers (~gql / apollo-style).

use crate::error::{GqlError, GqlResult};
use crate::parser::parse_document;
use crate::printer::{minify_document, print_document};
use serde_json::{json, Map, Value as JsonValue};

/// Canonicalize a query string (parse + print). Like the `gql` tag.
pub fn gql(source: &str) -> GqlResult<String> {
    let doc = parse_document(source)?;
    Ok(print_document(&doc))
}

/// Minify a query string.
pub fn minify(source: &str) -> GqlResult<String> {
    let doc = parse_document(source)?;
    Ok(minify_document(&doc))
}

/// Build a GraphQL HTTP request object: `{query, variables?, operationName?}`.
pub fn request(
    query: &str,
    variables: Option<&Map<String, JsonValue>>,
    operation_name: Option<&str>,
) -> GqlResult<JsonValue> {
    // Validate query parses
    let _ = parse_document(query)?;
    let mut obj = Map::new();
    obj.insert("query".into(), JsonValue::String(query.to_string()));
    if let Some(vars) = variables {
        if !vars.is_empty() {
            obj.insert("variables".into(), JsonValue::Object(vars.clone()));
        }
    }
    if let Some(name) = operation_name {
        if !name.is_empty() {
            obj.insert("operationName".into(), JsonValue::String(name.to_string()));
        }
    }
    Ok(JsonValue::Object(obj))
}

/// Build a JSON string suitable as an HTTP POST body.
pub fn request_json(
    query: &str,
    variables: Option<&Map<String, JsonValue>>,
    operation_name: Option<&str>,
) -> GqlResult<String> {
    let obj = request(query, variables, operation_name)?;
    serde_json::to_string(&obj).map_err(|e| GqlError::new(e.to_string()))
}

/// Parse variables from a JSON object value.
pub fn vars_from_json(v: &JsonValue) -> GqlResult<Map<String, JsonValue>> {
    match v {
        JsonValue::Object(m) => Ok(m.clone()),
        JsonValue::Null => Ok(Map::new()),
        _ => Err(GqlError::new("variables must be a JSON object")),
    }
}

/// Operation summary as JSON.
pub fn operation_summary(op: &crate::ast::OperationDefinition) -> JsonValue {
    json!({
        "name": op.name,
        "kind": op.operation.as_str(),
        "variables": op.variables.iter().map(|v| {
            json!({
                "name": v.name,
                "type": crate::schema::type_to_string(&v.ty),
            })
        }).collect::<Vec<_>>(),
    })
}

/// Fragment summary as JSON.
pub fn fragment_summary(f: &crate::ast::FragmentDefinition) -> JsonValue {
    json!({
        "name": f.name,
        "type_condition": f.type_condition,
    })
}
