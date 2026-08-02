//! Map-based GraphQL execution against a JSON root value.

use crate::ast::*;
use crate::error::{GqlError, GqlResult};
use crate::schema::Schema;
use serde_json::{json, Map, Value as JsonValue};
use std::collections::HashMap;

/// Result of executing a GraphQL operation.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub data: Option<JsonValue>,
    pub errors: Vec<ExecError>,
}

#[derive(Debug, Clone)]
pub struct ExecError {
    pub message: String,
    pub path: Vec<JsonValue>,
}

impl ExecutionResult {
    pub fn to_json(&self) -> JsonValue {
        let mut obj = Map::new();
        if let Some(data) = &self.data {
            obj.insert("data".into(), data.clone());
        } else {
            obj.insert("data".into(), JsonValue::Null);
        }
        if !self.errors.is_empty() {
            obj.insert(
                "errors".into(),
                JsonValue::Array(
                    self.errors
                        .iter()
                        .map(|e| {
                            json!({
                                "message": e.message,
                                "path": e.path,
                            })
                        })
                        .collect(),
                ),
            );
        }
        JsonValue::Object(obj)
    }
}

/// Execute a query string against a schema and root value.
pub fn execute(
    schema: &Schema,
    query: &str,
    root: &JsonValue,
    variables: &Map<String, JsonValue>,
    operation_name: Option<&str>,
) -> GqlResult<ExecutionResult> {
    let doc = crate::parser::parse_document(query)?;
    execute_doc(schema, &doc, root, variables, operation_name)
}

/// Execute a parsed document.
pub fn execute_doc(
    schema: &Schema,
    doc: &Document,
    root: &JsonValue,
    variables: &Map<String, JsonValue>,
    operation_name: Option<&str>,
) -> GqlResult<ExecutionResult> {
    let fragments: HashMap<&str, &FragmentDefinition> = doc
        .definitions
        .iter()
        .filter_map(|d| match d {
            Definition::Fragment(f) => Some((f.name.as_str(), f)),
            _ => None,
        })
        .collect();

    let ops: Vec<&OperationDefinition> = doc
        .definitions
        .iter()
        .filter_map(|d| match d {
            Definition::Operation(o) => Some(o),
            _ => None,
        })
        .collect();

    let op = select_operation(&ops, operation_name)?;
    let mut vars = variables.clone();
    // Apply defaults from variable definitions
    for vdef in &op.variables {
        if !vars.contains_key(&vdef.name) {
            if let Some(default) = &vdef.default_value {
                if let Some(jv) = default.to_json(&vars) {
                    vars.insert(vdef.name.clone(), jv);
                }
            } else if vdef.ty.is_non_null() {
                return Ok(ExecutionResult {
                    data: None,
                    errors: vec![ExecError {
                        message: format!(
                            "variable '${}' of type '{}' was not provided",
                            vdef.name,
                            crate::schema::type_to_string(&vdef.ty)
                        ),
                        path: vec![],
                    }],
                });
            }
        }
    }

    let root_type = match op.operation {
        OperationType::Query => schema.query_type.as_str(),
        OperationType::Mutation => schema
            .mutation_type
            .as_deref()
            .ok_or_else(|| GqlError::new("schema has no mutation type"))?,
        OperationType::Subscription => schema
            .subscription_type
            .as_deref()
            .ok_or_else(|| GqlError::new("schema has no subscription type"))?,
    };

    let mut errors = Vec::new();
    let data = execute_selection_set(
        &op.selection_set,
        root,
        root_type,
        schema,
        &fragments,
        &vars,
        &mut errors,
        &mut Vec::new(),
    );

    Ok(ExecutionResult {
        data: Some(data),
        errors,
    })
}

fn select_operation<'a>(
    ops: &[&'a OperationDefinition],
    operation_name: Option<&str>,
) -> GqlResult<&'a OperationDefinition> {
    if ops.is_empty() {
        return Err(GqlError::new("document has no operations"));
    }
    if let Some(name) = operation_name {
        return ops
            .iter()
            .find(|o| o.name.as_deref() == Some(name))
            .copied()
            .ok_or_else(|| GqlError::new(format!("unknown operation '{name}'")));
    }
    if ops.len() == 1 {
        return Ok(ops[0]);
    }
    Err(GqlError::new(
        "operation name is required when document has multiple operations",
    ))
}

#[allow(clippy::too_many_arguments)]
fn execute_selection_set(
    set: &SelectionSet,
    parent: &JsonValue,
    parent_type: &str,
    schema: &Schema,
    fragments: &HashMap<&str, &FragmentDefinition>,
    variables: &Map<String, JsonValue>,
    errors: &mut Vec<ExecError>,
    path: &mut Vec<JsonValue>,
) -> JsonValue {
    let mut result = Map::new();
    collect_fields(
        set,
        parent_type,
        schema,
        fragments,
        variables,
        &mut |field| {
            if !should_include(&field.directives, variables) {
                return;
            }
            let key = field.response_key().to_string();
            path.push(JsonValue::String(key.clone()));
            let value = resolve_field(
                field,
                parent,
                parent_type,
                schema,
                fragments,
                variables,
                errors,
                path,
            );
            result.insert(key, value);
            path.pop();
        },
    );
    JsonValue::Object(result)
}

fn collect_fields(
    set: &SelectionSet,
    parent_type: &str,
    schema: &Schema,
    fragments: &HashMap<&str, &FragmentDefinition>,
    variables: &Map<String, JsonValue>,
    visit: &mut dyn FnMut(&Field),
) {
    for sel in &set.selections {
        match sel {
            Selection::Field(f) => visit(f),
            Selection::FragmentSpread(s) => {
                if !should_include(&s.directives, variables) {
                    continue;
                }
                if let Some(frag) = fragments.get(s.name.as_str()) {
                    if type_applies(schema, parent_type, &frag.type_condition) {
                        collect_fields(
                            &frag.selection_set,
                            parent_type,
                            schema,
                            fragments,
                            variables,
                            visit,
                        );
                    }
                }
            }
            Selection::InlineFragment(i) => {
                if !should_include(&i.directives, variables) {
                    continue;
                }
                let applies = match &i.type_condition {
                    Some(tc) => type_applies(schema, parent_type, tc),
                    None => true,
                };
                if applies {
                    collect_fields(
                        &i.selection_set,
                        parent_type,
                        schema,
                        fragments,
                        variables,
                        visit,
                    );
                }
            }
        }
    }
}

fn type_applies(schema: &Schema, runtime_type: &str, condition: &str) -> bool {
    if runtime_type == condition {
        return true;
    }
    // Interface / union membership
    match schema.get_type(runtime_type) {
        Some(crate::schema::SchemaType::Object { implements, .. }) => {
            implements.iter().any(|i| i == condition)
        }
        _ => false,
    }
}

fn should_include(directives: &[Directive], variables: &Map<String, JsonValue>) -> bool {
    for d in directives {
        if d.name == "skip" {
            if arg_bool(d, "if", variables) == Some(true) {
                return false;
            }
        } else if d.name == "include" {
            if arg_bool(d, "if", variables) == Some(false) {
                return false;
            }
        }
    }
    true
}

fn arg_bool(d: &Directive, name: &str, variables: &Map<String, JsonValue>) -> Option<bool> {
    let arg = d.arguments.iter().find(|a| a.name == name)?;
    match arg.value.to_json(variables)? {
        JsonValue::Bool(b) => Some(b),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn resolve_field(
    field: &Field,
    parent: &JsonValue,
    parent_type: &str,
    schema: &Schema,
    fragments: &HashMap<&str, &FragmentDefinition>,
    variables: &Map<String, JsonValue>,
    errors: &mut Vec<ExecError>,
    path: &mut Vec<JsonValue>,
) -> JsonValue {
    if field.name == "__typename" {
        return JsonValue::String(parent_type.to_string());
    }

    // Resolve from parent object (map-based, graphene/strawberry-like simple default)
    let resolved = match parent {
        JsonValue::Object(map) => map.get(&field.name).cloned().unwrap_or(JsonValue::Null),
        _ => JsonValue::Null,
    };

    // Apply argument-based filtering for list fields when arg `id` matches
    let resolved = apply_simple_args(resolved, field, variables);

    let return_type = schema
        .get_type(parent_type)
        .and_then(|t| t.field(&field.name))
        .map(|f| f.ty.named_inner().to_string());

    match (&field.selection_set, return_type.as_deref(), &resolved) {
        (Some(ss), Some(ty_name), JsonValue::Array(items)) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                path.push(JsonValue::Number(i.into()));
                let item_type = item
                    .get("__typename")
                    .and_then(|v| v.as_str())
                    .unwrap_or(ty_name);
                out.push(execute_selection_set(
                    ss, item, item_type, schema, fragments, variables, errors, path,
                ));
                path.pop();
            }
            JsonValue::Array(out)
        }
        (Some(ss), Some(ty_name), JsonValue::Object(_)) => {
            let item_type = resolved
                .get("__typename")
                .and_then(|v| v.as_str())
                .unwrap_or(ty_name);
            execute_selection_set(
                ss, &resolved, item_type, schema, fragments, variables, errors, path,
            )
        }
        (Some(_), _, JsonValue::Null) => JsonValue::Null,
        (Some(_), _, _) => {
            errors.push(ExecError {
                message: format!(
                    "field '{}' returned a scalar but a selection set was provided",
                    field.name
                ),
                path: path.clone(),
            });
            JsonValue::Null
        }
        (None, _, v) => v.clone(),
    }
}

fn apply_simple_args(
    value: JsonValue,
    field: &Field,
    variables: &Map<String, JsonValue>,
) -> JsonValue {
    if field.arguments.is_empty() {
        return value;
    }
    // If list of objects and `id` arg present, filter or find by id
    let id_arg = field.arguments.iter().find(|a| a.name == "id");
    if let (Some(id_arg), JsonValue::Array(items)) = (id_arg, &value) {
        if let Some(want) = id_arg.value.to_json(variables) {
            let matches: Vec<_> = items
                .iter()
                .filter(|item| item.get("id") == Some(&want))
                .cloned()
                .collect();
            // Singular field names often expect one object
            if matches.len() == 1 {
                return matches.into_iter().next().unwrap();
            }
            return JsonValue::Array(matches);
        }
    }
    value
}
