//! Schema DSL parsing and CREATE TABLE SQL generation.
//!
//! Field spec format: `"type[@attr1[@attr2...]]"`
//!
//! Types: `int`, `float`, `string`, `bool`, `datetime`
//! Attrs: `@id`, `@unique`, `@required`, `@default(value)`

use super::dialect::Dialect;
use crate::{RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::collections::HashMap;

// ── Field ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldType {
    Int,
    Float,
    Str,
    Bool,
    Datetime,
}

impl FieldType {
    pub fn sql_type(&self, dialect: Dialect) -> &'static str {
        match self {
            FieldType::Int => "INTEGER",
            FieldType::Float => "REAL",
            FieldType::Str => "TEXT",
            FieldType::Bool => dialect.bool_type(),
            FieldType::Datetime => dialect.datetime_type(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name: String,
    pub ty: FieldType,
    pub is_id: bool,
    pub is_unique: bool,
    pub nullable: bool,
    /// SQL expression to use in `DEFAULT <expr>`.
    pub default_sql: Option<String>,
}

// ── Model ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ModelDef {
    pub name: String,
    /// Ordered list of fields.
    pub fields: Vec<FieldDef>,
}

impl ModelDef {
    pub fn id_field(&self) -> Option<&FieldDef> {
        self.fields.iter().find(|f| f.is_id)
    }

    pub fn non_id_fields(&self) -> impl Iterator<Item = &FieldDef> {
        self.fields.iter().filter(|f| !f.is_id)
    }
}

// ── Schema handle ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SchemaHandle {
    pub models: HashMap<String, ModelDef>,
}

// ── Parsing ────────────────────────────────────────────────────────────────

pub fn parse_schema(val: &ValueRef, span: Span) -> Result<SchemaHandle, RuntimeError> {
    let borrowed = val.borrow();
    let obj = match &*borrowed {
        Value::Object(m) => m,
        other => {
            return Err(schema_err(
                span,
                format!("nmodel.schema() expects object, got {}", other.type_name()),
            ))
        }
    };
    let models_ref = obj
        .get("models")
        .ok_or_else(|| schema_err(span, "schema object missing \"models\" key"))?;
    let models_borrow = models_ref.borrow();
    let models_obj = match &*models_borrow {
        Value::Object(m) => m,
        other => {
            return Err(schema_err(
                span,
                format!("schema.models must be object, got {}", other.type_name()),
            ))
        }
    };

    let mut models: HashMap<String, ModelDef> = HashMap::new();
    for (model_name, model_ref) in models_obj {
        let model_borrow = model_ref.borrow();
        let model_obj = match &*model_borrow {
            Value::Object(m) => m,
            other => {
                return Err(schema_err(
                    span,
                    format!(
                        "model \"{}\" must be object, got {}",
                        model_name,
                        other.type_name()
                    ),
                ))
            }
        };
        let fields_ref = model_obj.get("fields").ok_or_else(|| {
            schema_err(
                span,
                format!("model \"{}\" missing \"fields\" key", model_name),
            )
        })?;
        let fields_borrow = fields_ref.borrow();
        let fields_obj = match &*fields_borrow {
            Value::Object(m) => m,
            other => {
                return Err(schema_err(
                    span,
                    format!(
                        "model \"{}\".fields must be object, got {}",
                        model_name,
                        other.type_name()
                    ),
                ))
            }
        };

        let mut fields: Vec<FieldDef> = Vec::new();
        for (field_name, field_val) in fields_obj {
            let spec = match &*field_val.borrow() {
                Value::String(s) => s.clone(),
                other => {
                    return Err(schema_err(
                        span,
                        format!(
                            "field spec for \"{}.{}\" must be string, got {}",
                            model_name,
                            field_name,
                            other.type_name()
                        ),
                    ))
                }
            };
            fields.push(parse_field(field_name, &spec, span)?);
        }
        models.insert(
            model_name.clone(),
            ModelDef {
                name: model_name.clone(),
                fields,
            },
        );
    }

    Ok(SchemaHandle { models })
}

pub fn parse_field(name: &str, spec: &str, span: Span) -> Result<FieldDef, RuntimeError> {
    let mut parts = spec.splitn(2, '@');
    let type_str = parts.next().unwrap_or("string").trim();
    let attrs_rest = parts.next().unwrap_or("").to_string();

    let ty = match type_str {
        "int" | "integer" => FieldType::Int,
        "float" | "real" | "double" => FieldType::Float,
        "string" | "text" | "str" => FieldType::Str,
        "bool" | "boolean" => FieldType::Bool,
        "datetime" => FieldType::Datetime,
        other => {
            return Err(schema_err(
                span,
                format!("unknown field type \"{}\" for field \"{}\"", other, name),
            ))
        }
    };

    let mut is_id = false;
    let mut is_unique = false;
    let mut nullable = true;
    let mut default_sql: Option<String> = None;

    for attr in attrs_rest.split('@') {
        let attr = attr.trim();
        if attr.is_empty() {
            continue;
        }
        if attr == "id" {
            is_id = true;
            nullable = false;
        } else if attr == "unique" {
            is_unique = true;
        } else if attr == "required" {
            nullable = false;
        } else if attr.starts_with("default(") && attr.ends_with(')') {
            let inner = &attr["default(".len()..attr.len() - 1];
            default_sql = Some(coerce_default_sql(inner));
        } else {
            return Err(schema_err(
                span,
                format!(
                    "unknown field attribute \"@{}\" on field \"{}\"",
                    attr, name
                ),
            ));
        }
    }

    Ok(FieldDef {
        name: name.to_string(),
        ty,
        is_id,
        is_unique,
        nullable,
        default_sql,
    })
}

/// Coerce a user-provided default string to a SQL literal.
fn coerce_default_sql(val: &str) -> String {
    if val.parse::<i64>().is_ok() || val.parse::<f64>().is_ok() {
        return val.to_string();
    }
    match val.to_uppercase().as_str() {
        "NULL" | "TRUE" | "FALSE" | "CURRENT_TIMESTAMP" | "CURRENT_DATE" | "CURRENT_TIME" => {
            val.to_uppercase()
        }
        _ => format!("'{}'", val.replace('\'', "''")),
    }
}

// ── SQL generation ─────────────────────────────────────────────────────────

/// Generate a `CREATE TABLE IF NOT EXISTS` statement for `model`.
pub fn create_table_sql(model: &ModelDef, dialect: Dialect) -> String {
    let mut col_defs: Vec<String> = Vec::new();
    let mut unique_constraints: Vec<String> = Vec::new();

    for field in &model.fields {
        if field.is_id {
            col_defs.push(format!(
                "  \"{}\" {}",
                field.name,
                dialect.autoincrement_pk()
            ));
        } else {
            let sql_ty = field.ty.sql_type(dialect);
            let mut col = format!("  \"{}\" {}", field.name, sql_ty);
            if !field.nullable {
                col.push_str(" NOT NULL");
            }
            if let Some(ref dv) = field.default_sql {
                col.push_str(&format!(" DEFAULT {}", dv));
            }
            col_defs.push(col);
            if field.is_unique {
                unique_constraints.push(format!("  UNIQUE (\"{}\")", field.name));
            }
        }
    }
    col_defs.extend(unique_constraints);

    format!(
        "CREATE TABLE IF NOT EXISTS \"{}\" (\n{}\n)",
        model.name,
        col_defs.join(",\n")
    )
}

fn schema_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E2833_NMODEL_SCHEMA, msg.into())
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn dummy_span() -> Span {
        Span::dummy()
    }

    #[test]
    fn parse_field_int_id() {
        let f = parse_field("id", "int@id", dummy_span()).unwrap();
        assert_eq!(f.name, "id");
        assert!(matches!(f.ty, FieldType::Int));
        assert!(f.is_id);
        assert!(!f.nullable);
    }

    #[test]
    fn parse_field_string_unique() {
        let f = parse_field("email", "string@unique", dummy_span()).unwrap();
        assert_eq!(f.ty, FieldType::Str);
        assert!(f.is_unique);
        assert!(!f.is_id);
    }

    #[test]
    fn parse_field_with_default() {
        let f = parse_field("status", "string@default(active)", dummy_span()).unwrap();
        assert_eq!(f.default_sql.as_deref(), Some("'active'"));
    }

    #[test]
    fn parse_field_bool() {
        let f = parse_field("active", "bool", dummy_span()).unwrap();
        assert!(matches!(f.ty, FieldType::Bool));
        assert!(f.nullable);
    }

    #[test]
    fn parse_field_unknown_type_err() {
        assert!(parse_field("x", "uuid@id", dummy_span()).is_err());
    }

    #[test]
    fn parse_field_unknown_attr_err() {
        assert!(parse_field("x", "string@encrypt", dummy_span()).is_err());
    }

    #[test]
    fn create_table_sqlite() {
        let model = ModelDef {
            name: "User".to_string(),
            fields: vec![
                FieldDef {
                    name: "id".to_string(),
                    ty: FieldType::Int,
                    is_id: true,
                    is_unique: false,
                    nullable: false,
                    default_sql: None,
                },
                FieldDef {
                    name: "email".to_string(),
                    ty: FieldType::Str,
                    is_id: false,
                    is_unique: true,
                    nullable: false,
                    default_sql: None,
                },
            ],
        };
        let sql = create_table_sql(&model, Dialect::Sqlite);
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS \"User\""));
        assert!(sql.contains("AUTOINCREMENT"));
        assert!(sql.contains("UNIQUE (\"email\")"));
    }

    #[test]
    fn create_table_pg() {
        let model = ModelDef {
            name: "Post".to_string(),
            fields: vec![
                FieldDef {
                    name: "id".to_string(),
                    ty: FieldType::Int,
                    is_id: true,
                    is_unique: false,
                    nullable: false,
                    default_sql: None,
                },
                FieldDef {
                    name: "active".to_string(),
                    ty: FieldType::Bool,
                    is_id: false,
                    is_unique: false,
                    nullable: true,
                    default_sql: Some("TRUE".to_string()),
                },
            ],
        };
        let sql = create_table_sql(&model, Dialect::Pg);
        assert!(sql.contains("GENERATED ALWAYS AS IDENTITY"));
        assert!(sql.contains("BOOLEAN"));
        assert!(sql.contains("DEFAULT TRUE"));
    }

    #[test]
    fn coerce_default_number() {
        assert_eq!(coerce_default_sql("42"), "42");
        assert_eq!(coerce_default_sql("3.14"), "3.14");
    }

    #[test]
    fn coerce_default_keyword() {
        assert_eq!(coerce_default_sql("CURRENT_TIMESTAMP"), "CURRENT_TIMESTAMP");
        assert_eq!(coerce_default_sql("true"), "TRUE");
    }

    #[test]
    fn coerce_default_string_literal() {
        assert_eq!(coerce_default_sql("hello"), "'hello'");
        assert_eq!(coerce_default_sql("it's"), "'it''s'");
    }
}
