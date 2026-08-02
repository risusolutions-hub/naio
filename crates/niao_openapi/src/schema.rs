//! JSON Schema / OpenAPI schema helpers and example-driven inference.

use serde_json::{json, Map, Value};

/// `#/components/schemas/{name}` reference object.
pub fn schema_ref(name: &str) -> Value {
    json!({ "$ref": format!("#/components/schemas/{name}") })
}

pub fn schema_string(opts: Option<&Map<String, Value>>) -> Value {
    let mut m = Map::new();
    m.insert("type".into(), Value::String("string".into()));
    apply_opts(&mut m, opts);
    Value::Object(m)
}

pub fn schema_integer(opts: Option<&Map<String, Value>>) -> Value {
    let mut m = Map::new();
    m.insert("type".into(), Value::String("integer".into()));
    apply_opts(&mut m, opts);
    Value::Object(m)
}

pub fn schema_number(opts: Option<&Map<String, Value>>) -> Value {
    let mut m = Map::new();
    m.insert("type".into(), Value::String("number".into()));
    apply_opts(&mut m, opts);
    Value::Object(m)
}

pub fn schema_boolean(opts: Option<&Map<String, Value>>) -> Value {
    let mut m = Map::new();
    m.insert("type".into(), Value::String("boolean".into()));
    apply_opts(&mut m, opts);
    Value::Object(m)
}

pub fn schema_array(items: Value, opts: Option<&Map<String, Value>>) -> Value {
    let mut m = Map::new();
    m.insert("type".into(), Value::String("array".into()));
    m.insert("items".into(), items);
    apply_opts(&mut m, opts);
    Value::Object(m)
}

pub fn schema_object(
    properties: Map<String, Value>,
    required: Option<Vec<String>>,
    opts: Option<&Map<String, Value>>,
) -> Value {
    let mut m = Map::new();
    m.insert("type".into(), Value::String("object".into()));
    m.insert("properties".into(), Value::Object(properties));
    if let Some(req) = required {
        if !req.is_empty() {
            m.insert(
                "required".into(),
                Value::Array(req.into_iter().map(Value::String).collect()),
            );
        }
    }
    apply_opts(&mut m, opts);
    Value::Object(m)
}

fn apply_opts(m: &mut Map<String, Value>, opts: Option<&Map<String, Value>>) {
    if let Some(opts) = opts {
        for (k, v) in opts {
            if k == "type" || k == "properties" || k == "items" {
                continue;
            }
            m.insert(k.clone(), v.clone());
        }
    }
}

/// Infer a JSON Schema-ish object from an example value (FastAPI/pydantic-style).
pub fn infer_schema(example: &Value) -> Value {
    match example {
        Value::Null => json!({ "nullable": true }),
        Value::Bool(_) => json!({ "type": "boolean", "example": example }),
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                json!({ "type": "integer", "example": example })
            } else {
                json!({ "type": "number", "example": example })
            }
        }
        Value::String(_) => json!({ "type": "string", "example": example }),
        Value::Array(items) => {
            let item_schema = if let Some(first) = items.first() {
                infer_schema(first)
            } else {
                json!({})
            };
            json!({
                "type": "array",
                "items": item_schema,
                "example": example
            })
        }
        Value::Object(map) => {
            let mut props = Map::new();
            let mut required = Vec::new();
            for (k, v) in map {
                props.insert(k.clone(), infer_schema(v));
                if !v.is_null() {
                    required.push(k.clone());
                }
            }
            let mut out = Map::new();
            out.insert("type".into(), Value::String("object".into()));
            out.insert("properties".into(), Value::Object(props));
            if !required.is_empty() {
                out.insert(
                    "required".into(),
                    Value::Array(required.into_iter().map(Value::String).collect()),
                );
            }
            out.insert("example".into(), example.clone());
            Value::Object(out)
        }
    }
}

/// Build an OpenAPI parameter object.
pub fn param(
    name: &str,
    location: &str,
    schema: Option<Value>,
    opts: Option<&Map<String, Value>>,
) -> Value {
    let loc = location.trim().to_ascii_lowercase();
    let mut m = Map::new();
    m.insert("name".into(), Value::String(name.into()));
    m.insert("in".into(), Value::String(loc.clone()));
    if loc == "path" {
        m.insert("required".into(), Value::Bool(true));
    }
    m.insert(
        "schema".into(),
        schema.unwrap_or_else(|| json!({ "type": "string" })),
    );
    apply_opts(&mut m, opts);
    Value::Object(m)
}

/// Build an OpenAPI requestBody object wrapping a JSON schema.
pub fn request_body(schema: Value, opts: Option<&Map<String, Value>>) -> Value {
    let required = opts
        .and_then(|o| o.get("required"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let description = opts
        .and_then(|o| o.get("description"))
        .cloned()
        .unwrap_or(Value::Null);
    let content_type = opts
        .and_then(|o| o.get("content_type"))
        .and_then(|v| v.as_str())
        .unwrap_or("application/json");
    let mut body = Map::new();
    if !description.is_null() {
        body.insert("description".into(), description);
    }
    body.insert("required".into(), Value::Bool(required));
    body.insert(
        "content".into(),
        json!({
            content_type: { "schema": schema }
        }),
    );
    Value::Object(body)
}

/// Build a responses-map entry for a status code.
pub fn response(
    status: &str,
    description: &str,
    schema: Option<Value>,
    opts: Option<&Map<String, Value>>,
) -> (String, Value) {
    let mut m = Map::new();
    m.insert("description".into(), Value::String(description.into()));
    if let Some(schema) = schema {
        let content_type = opts
            .and_then(|o| o.get("content_type"))
            .and_then(|v| v.as_str())
            .unwrap_or("application/json");
        m.insert(
            "content".into(),
            json!({
                content_type: { "schema": schema }
            }),
        );
    }
    apply_opts(&mut m, opts);
    (status.to_string(), Value::Object(m))
}

/// Build a free-form operation object from opts map keys.
pub fn operation(opts: &Map<String, Value>) -> Value {
    let mut m = Map::new();
    for (k, v) in opts {
        m.insert(k.clone(), v.clone());
    }
    if !m.contains_key("responses") {
        m.insert(
            "responses".into(),
            json!({ "200": { "description": "OK" } }),
        );
    }
    Value::Object(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_object() {
        let ex = json!({ "id": 1, "name": "a" });
        let s = infer_schema(&ex);
        assert_eq!(s["type"], "object");
        assert!(s["properties"]["id"]["type"] == "integer");
    }

    #[test]
    fn ref_shape() {
        assert_eq!(
            schema_ref("User"),
            json!({ "$ref": "#/components/schemas/User" })
        );
    }
}
