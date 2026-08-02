//! Structural validation for OpenAPI 3 documents.

use crate::doc::OpenApiDoc;
use crate::error::OpenApiResult;
use crate::pathutil::method_key;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationReport {
    pub ok: bool,
    pub errors: Vec<ValidationIssue>,
}

impl ValidationReport {
    pub fn to_value(&self) -> Value {
        let errors: Vec<Value> = self
            .errors
            .iter()
            .map(|e| {
                serde_json::json!({
                    "path": e.path,
                    "message": e.message
                })
            })
            .collect();
        serde_json::json!({
            "ok": self.ok,
            "errors": errors
        })
    }
}

pub fn validate(doc: &OpenApiDoc) -> OpenApiResult<ValidationReport> {
    let mut errors = Vec::new();
    let root = &doc.root;

    match root.get("openapi").and_then(|v| v.as_str()) {
        Some(v) if v.starts_with("3.") => {}
        Some(v) => errors.push(ValidationIssue {
            path: "openapi".into(),
            message: format!("unsupported version '{v}', expected 3.x"),
        }),
        None => errors.push(ValidationIssue {
            path: "openapi".into(),
            message: "missing required field".into(),
        }),
    }

    match root.get("info") {
        Some(Value::Object(info)) => {
            if info.get("title").and_then(|v| v.as_str()).is_none() {
                errors.push(ValidationIssue {
                    path: "info.title".into(),
                    message: "missing required string".into(),
                });
            }
            if info.get("version").and_then(|v| v.as_str()).is_none() {
                errors.push(ValidationIssue {
                    path: "info.version".into(),
                    message: "missing required string".into(),
                });
            }
        }
        Some(_) => errors.push(ValidationIssue {
            path: "info".into(),
            message: "must be an object".into(),
        }),
        None => errors.push(ValidationIssue {
            path: "info".into(),
            message: "missing required field".into(),
        }),
    }

    match root.get("paths") {
        Some(Value::Object(paths)) => {
            for (path, item) in paths {
                if !path.starts_with('/') {
                    errors.push(ValidationIssue {
                        path: format!("paths.{path}"),
                        message: "path keys must start with '/'".into(),
                    });
                }
                let Some(item_obj) = item.as_object() else {
                    errors.push(ValidationIssue {
                        path: format!("paths.{path}"),
                        message: "path item must be an object".into(),
                    });
                    continue;
                };
                for (method, op) in item_obj {
                    let lower = method.to_ascii_lowercase();
                    if matches!(
                        lower.as_str(),
                        "get" | "post" | "put" | "delete" | "patch" | "options" | "head" | "trace"
                    ) {
                        if method_key(method).is_err() {
                            errors.push(ValidationIssue {
                                path: format!("paths.{path}.{method}"),
                                message: "invalid HTTP method".into(),
                            });
                        }
                        let Some(op_obj) = op.as_object() else {
                            errors.push(ValidationIssue {
                                path: format!("paths.{path}.{method}"),
                                message: "operation must be an object".into(),
                            });
                            continue;
                        };
                        match op_obj.get("responses") {
                            Some(Value::Object(r)) if !r.is_empty() => {}
                            Some(_) => errors.push(ValidationIssue {
                                path: format!("paths.{path}.{method}.responses"),
                                message: "responses must be a non-empty object".into(),
                            }),
                            None => errors.push(ValidationIssue {
                                path: format!("paths.{path}.{method}.responses"),
                                message: "missing required field".into(),
                            }),
                        }
                    } else if matches!(
                        lower.as_str(),
                        "summary" | "description" | "servers" | "parameters" | "$ref"
                    ) {
                        // path-item level fields — ok
                    } else {
                        errors.push(ValidationIssue {
                            path: format!("paths.{path}.{method}"),
                            message: format!("unknown path item field '{method}'"),
                        });
                    }
                }
            }
        }
        Some(_) => errors.push(ValidationIssue {
            path: "paths".into(),
            message: "must be an object".into(),
        }),
        None => errors.push(ValidationIssue {
            path: "paths".into(),
            message: "missing required field".into(),
        }),
    }

    if let Some(comps) = root.get("components") {
        if !comps.is_object() {
            errors.push(ValidationIssue {
                path: "components".into(),
                message: "must be an object".into(),
            });
        }
    }

    let ok = errors.is_empty();
    Ok(ValidationReport { ok, errors })
}

pub fn is_valid(doc: &OpenApiDoc) -> OpenApiResult<bool> {
    Ok(validate(doc)?.ok)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::OpenApiDoc;
    use serde_json::json;

    #[test]
    fn valid_minimal() {
        let info = json!({"title": "T", "version": "1"});
        let mut doc = OpenApiDoc::create(info.as_object().unwrap(), None).unwrap();
        let mut route = serde_json::Map::new();
        route.insert("method".into(), json!("get"));
        route.insert("path".into(), json!("/x"));
        doc.add_route(&route).unwrap();
        assert!(is_valid(&doc).unwrap());
    }

    #[test]
    fn missing_info_fails() {
        let raw = json!({"openapi": "3.1.0", "paths": {}});
        let doc = OpenApiDoc::parse_value(raw).unwrap();
        let r = validate(&doc).unwrap();
        assert!(!r.ok);
    }
}
