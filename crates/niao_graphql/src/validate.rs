//! Query validation against a schema.

use crate::ast::*;
use crate::error::GqlResult;
use crate::schema::Schema;
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;

/// Validation result.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub ok: bool,
    pub errors: Vec<ValidationError>,
}

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub message: String,
    pub path: Vec<String>,
}

impl ValidationResult {
    pub fn to_json(&self) -> JsonValue {
        json!({
            "ok": self.ok,
            "errors": self.errors.iter().map(|e| {
                json!({
                    "message": e.message,
                    "path": e.path,
                })
            }).collect::<Vec<_>>()
        })
    }
}

/// Validate a document against a schema.
pub fn validate(
    doc: &Document,
    schema: &Schema,
    operation_name: Option<&str>,
) -> GqlResult<ValidationResult> {
    let mut errors = Vec::new();
    let fragments: HashMap<&str, &FragmentDefinition> = doc
        .definitions
        .iter()
        .filter_map(|d| match d {
            Definition::Fragment(f) => Some((f.name.as_str(), f)),
            _ => None,
        })
        .collect();

    // Duplicate fragment names
    let mut seen_frags = HashMap::new();
    for def in &doc.definitions {
        if let Definition::Fragment(f) = def {
            if seen_frags.insert(f.name.as_str(), ()).is_some() {
                errors.push(ValidationError {
                    message: format!("duplicate fragment name '{}'", f.name),
                    path: vec![],
                });
            }
            if !schema.has_type(&f.type_condition) {
                errors.push(ValidationError {
                    message: format!(
                        "fragment '{}' type condition '{}' is not defined",
                        f.name, f.type_condition
                    ),
                    path: vec![f.name.clone()],
                });
            }
        }
    }

    let ops: Vec<&OperationDefinition> = doc
        .definitions
        .iter()
        .filter_map(|d| match d {
            Definition::Operation(o) => Some(o),
            _ => None,
        })
        .collect();

    if ops.is_empty() {
        errors.push(ValidationError {
            message: "document has no operations".into(),
            path: vec![],
        });
        return Ok(ValidationResult { ok: false, errors });
    }

    let anonymous = ops.iter().filter(|o| o.name.is_none()).count();
    if anonymous > 1 || (anonymous == 1 && ops.len() > 1) {
        errors.push(ValidationError {
            message: "anonymous operation must be the only operation in the document".into(),
            path: vec![],
        });
    }

    let mut seen_ops = HashMap::new();
    for op in &ops {
        if let Some(name) = &op.name {
            if seen_ops.insert(name.as_str(), ()).is_some() {
                errors.push(ValidationError {
                    message: format!("duplicate operation name '{name}'"),
                    path: vec![name.clone()],
                });
            }
        }
    }

    let selected: Vec<&OperationDefinition> = if let Some(name) = operation_name {
        match ops.iter().find(|o| o.name.as_deref() == Some(name)) {
            Some(o) => vec![*o],
            None => {
                errors.push(ValidationError {
                    message: format!("unknown operation '{name}'"),
                    path: vec![],
                });
                Vec::new()
            }
        }
    } else if ops.len() == 1 {
        vec![ops[0]]
    } else {
        // Validate all when no name given and multiple exist
        ops
    };

    for op in selected {
        let root = match op.operation {
            OperationType::Query => Some(schema.query_type.as_str()),
            OperationType::Mutation => schema.mutation_type.as_deref(),
            OperationType::Subscription => schema.subscription_type.as_deref(),
        };
        let Some(root_name) = root else {
            errors.push(ValidationError {
                message: format!(
                    "schema does not support {} operations",
                    op.operation.as_str()
                ),
                path: op.name.clone().into_iter().collect(),
            });
            continue;
        };
        validate_selection_set(
            &op.selection_set,
            root_name,
            schema,
            &fragments,
            &mut errors,
            &mut Vec::new(),
            &mut Vec::new(),
        );
    }

    Ok(ValidationResult {
        ok: errors.is_empty(),
        errors,
    })
}

fn validate_selection_set(
    set: &SelectionSet,
    parent_type: &str,
    schema: &Schema,
    fragments: &HashMap<&str, &FragmentDefinition>,
    errors: &mut Vec<ValidationError>,
    path: &mut Vec<String>,
    visited_frags: &mut Vec<String>,
) {
    let Some(ty) = schema.get_type(parent_type) else {
        errors.push(ValidationError {
            message: format!("unknown type '{parent_type}'"),
            path: path.clone(),
        });
        return;
    };

    for sel in &set.selections {
        match sel {
            Selection::Field(f) => {
                // Introspection allowed lightly
                if f.name == "__typename" {
                    continue;
                }
                path.push(f.response_key().to_string());
                match ty.field(&f.name) {
                    None => {
                        errors.push(ValidationError {
                            message: format!(
                                "field '{}' does not exist on type '{}'",
                                f.name, parent_type
                            ),
                            path: path.clone(),
                        });
                    }
                    Some(field_def) => {
                        let return_name = field_def.ty.named_inner();
                        if let Some(ss) = &f.selection_set {
                            let needs_set = matches!(
                                schema.get_type(return_name),
                                Some(
                                    crate::schema::SchemaType::Object { .. }
                                        | crate::schema::SchemaType::Interface { .. }
                                        | crate::schema::SchemaType::Union { .. }
                                )
                            );
                            if needs_set {
                                validate_selection_set(
                                    ss,
                                    return_name,
                                    schema,
                                    fragments,
                                    errors,
                                    path,
                                    visited_frags,
                                );
                            }
                        } else if matches!(
                            schema.get_type(return_name),
                            Some(
                                crate::schema::SchemaType::Object { .. }
                                    | crate::schema::SchemaType::Interface { .. }
                                    | crate::schema::SchemaType::Union { .. }
                            )
                        ) {
                            errors.push(ValidationError {
                                message: format!(
                                    "field '{}' of type '{}' must have a selection set",
                                    f.name, return_name
                                ),
                                path: path.clone(),
                            });
                        }
                    }
                }
                path.pop();
            }
            Selection::FragmentSpread(s) => {
                if visited_frags.contains(&s.name) {
                    errors.push(ValidationError {
                        message: format!("fragment cycle involving '{}'", s.name),
                        path: path.clone(),
                    });
                    continue;
                }
                match fragments.get(s.name.as_str()) {
                    None => errors.push(ValidationError {
                        message: format!("unknown fragment '{}'", s.name),
                        path: path.clone(),
                    }),
                    Some(frag) => {
                        visited_frags.push(s.name.clone());
                        validate_selection_set(
                            &frag.selection_set,
                            &frag.type_condition,
                            schema,
                            fragments,
                            errors,
                            path,
                            visited_frags,
                        );
                        visited_frags.pop();
                    }
                }
            }
            Selection::InlineFragment(i) => {
                let cond = i.type_condition.as_deref().unwrap_or(parent_type);
                validate_selection_set(
                    &i.selection_set,
                    cond,
                    schema,
                    fragments,
                    errors,
                    path,
                    visited_frags,
                );
            }
        }
    }
}
