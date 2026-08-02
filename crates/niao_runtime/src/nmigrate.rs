//! Native nmigrate standard library — schema diff from nmodel-style struct
//! definitions to SQL migration statements (SQLite / PostgreSQL).
//!
//! Import with `import "nmigrate"` (or `import "std/nmigrate"`).

use crate::{NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

const E3260_NMIGRATE_ARITY: u32 = 3260;
const E3261_NMIGRATE_ERROR: u32 = 3261;
const E3262_NMIGRATE_TYPE: u32 = 3262;

// ---------------------------------------------------------------------------
// Embedded nmodel-compatible schema DSL (self-contained; nmodel::dialect is private)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dialect {
    Sqlite,
    Pg,
}

impl Dialect {
    fn bool_type(self) -> &'static str {
        match self {
            Dialect::Sqlite => "INTEGER",
            Dialect::Pg => "BOOLEAN",
        }
    }

    fn datetime_type(self) -> &'static str {
        match self {
            Dialect::Sqlite => "TEXT",
            Dialect::Pg => "TIMESTAMPTZ",
        }
    }

    fn autoincrement_pk(self) -> &'static str {
        match self {
            Dialect::Sqlite => "INTEGER PRIMARY KEY AUTOINCREMENT",
            Dialect::Pg => "INTEGER PRIMARY KEY GENERATED ALWAYS AS IDENTITY",
        }
    }
}

fn parse_dialect_name(s: &str, span: Span) -> Result<Dialect, RuntimeError> {
    match s.to_lowercase().as_str() {
        "sqlite" => Ok(Dialect::Sqlite),
        "pg" | "postgres" | "postgresql" => Ok(Dialect::Pg),
        other => Err(RuntimeError::at(
            span,
            E3261_NMIGRATE_ERROR,
            format!("unknown dialect \"{other}\" (use \"sqlite\" or \"pg\")"),
        )),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldType {
    Int,
    Float,
    Str,
    Bool,
    Datetime,
}

impl FieldType {
    fn sql_type(self, dialect: Dialect) -> &'static str {
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
struct FieldDef {
    name: String,
    ty: FieldType,
    is_id: bool,
    is_unique: bool,
    nullable: bool,
    default_sql: Option<String>,
}

#[derive(Debug, Clone)]
struct ModelDef {
    name: String,
    fields: Vec<FieldDef>,
}

#[derive(Debug, Clone)]
struct SchemaHandle {
    models: HashMap<String, ModelDef>,
}

fn schema_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3261_NMIGRATE_ERROR, msg.into())
}

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

fn parse_field(name: &str, spec: &str, span: Span) -> Result<FieldDef, RuntimeError> {
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
                format!("unknown field type \"{other}\" for field \"{name}\""),
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
                format!("unknown field attribute \"@{attr}\" on field \"{name}\""),
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

fn parse_schema(val: &ValueRef, span: Span) -> Result<SchemaHandle, RuntimeError> {
    let borrowed = val.borrow();
    let obj = match &*borrowed {
        Value::Object(m) => m,
        other => {
            return Err(schema_err(
                span,
                format!("schema expects object, got {}", other.type_name()),
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
                        "model \"{model_name}\" must be object, got {}",
                        other.type_name()
                    ),
                ))
            }
        };
        let fields_ref = model_obj.get("fields").ok_or_else(|| {
            schema_err(
                span,
                format!("model \"{model_name}\" missing \"fields\" key"),
            )
        })?;
        let fields_borrow = fields_ref.borrow();
        let fields_obj = match &*fields_borrow {
            Value::Object(m) => m,
            other => {
                return Err(schema_err(
                    span,
                    format!(
                        "model \"{model_name}\".fields must be object, got {}",
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
                            "field spec for \"{model_name}.{field_name}\" must be string, got {}",
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

fn create_table_sql(model: &ModelDef, dialect: Dialect) -> String {
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
            let mut col = format!("  \"{}\" {sql_ty}", field.name);
            if !field.nullable {
                col.push_str(" NOT NULL");
            }
            if let Some(ref dv) = field.default_sql {
                col.push_str(&format!(" DEFAULT {dv}"));
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

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3262_NMIGRATE_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3260_NMIGRATE_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(
    args: &[ValueRef],
    min: usize,
    max: usize,
    name: &str,
    span: Span,
) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3260_NMIGRATE_ARITY,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
        ));
    }
    Ok(())
}

fn string_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn dialect_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Dialect> {
    parse_dialect_name(&string_arg(args, idx, name, span)?, span)
}

fn default_dialect(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Dialect> {
    if args.len() > idx {
        dialect_arg(args, idx, name, span)
    } else {
        Ok(Dialect::Sqlite)
    }
}

// ---------------------------------------------------------------------------
// Diff model
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
enum ChangeKind {
    CreateTable,
    DropTable,
    AddColumn,
    DropColumn,
    AlterColumn,
}

#[derive(Clone, Debug)]
struct Change {
    kind: ChangeKind,
    table: String,
    column: Option<String>,
    field: Option<FieldDef>,
    from_type: Option<String>,
    to_type: Option<String>,
}

fn field_map(model: &ModelDef) -> HashMap<&str, &FieldDef> {
    model.fields.iter().map(|f| (f.name.as_str(), f)).collect()
}

fn field_type_sql(field: &FieldDef, dialect: Dialect) -> String {
    if field.is_id {
        return dialect.autoincrement_pk().to_string();
    }
    let mut col = field.ty.sql_type(dialect).to_string();
    if !field.nullable {
        col.push_str(" NOT NULL");
    }
    if let Some(ref dv) = field.default_sql {
        col.push_str(&format!(" DEFAULT {dv}"));
    }
    col
}

fn column_add_sql(table: &str, field: &FieldDef, dialect: Dialect) -> String {
    format!(
        "ALTER TABLE \"{table}\" ADD COLUMN \"{}\" {};",
        field.name,
        field_type_sql(field, dialect)
    )
}

fn change_to_sql(change: &Change, dialect: Dialect) -> Vec<String> {
    match change.kind {
        ChangeKind::CreateTable => vec![],
        ChangeKind::DropTable => {
            vec![format!("DROP TABLE IF EXISTS \"{}\";", change.table)]
        }
        ChangeKind::AddColumn => {
            if let Some(ref field) = change.field {
                vec![column_add_sql(&change.table, field, dialect)]
            } else {
                vec![]
            }
        }
        ChangeKind::DropColumn => {
            if let Some(ref col) = change.column {
                vec![format!(
                    "ALTER TABLE \"{}\" DROP COLUMN \"{}\";",
                    change.table, col
                )]
            } else {
                vec![]
            }
        }
        ChangeKind::AlterColumn => {
            if let (Some(ref col), Some(ref to)) = (&change.column, &change.to_type) {
                let from = change.from_type.as_deref().unwrap_or("?");
                match dialect {
                    Dialect::Pg => vec![format!(
                        "ALTER TABLE \"{}\" ALTER COLUMN \"{}\" TYPE {};",
                        change.table, col, to
                    )],
                    Dialect::Sqlite => vec![format!(
                        "-- SQLite: manual migration required to change \"{}\".\"{}\" from {from} to {to}",
                        change.table, col
                    )],
                }
            } else {
                vec![]
            }
        }
    }
}

fn diff_schemas(old: &SchemaHandle, new: &SchemaHandle) -> (Vec<Change>, HashMap<String, i64>) {
    let mut changes = Vec::new();
    let mut summary: HashMap<String, i64> = HashMap::new();

    let old_names: HashSet<_> = old.models.keys().collect();
    let new_names: HashSet<_> = new.models.keys().collect();

    for name in new_names.difference(&old_names) {
        let model = &new.models[*name];
        changes.push(Change {
            kind: ChangeKind::CreateTable,
            table: model.name.clone(),
            column: None,
            field: None,
            from_type: None,
            to_type: None,
        });
        *summary.entry("create_table".into()).or_insert(0) += 1;
    }

    for name in old_names.difference(&new_names) {
        changes.push(Change {
            kind: ChangeKind::DropTable,
            table: (*name).clone(),
            column: None,
            field: None,
            from_type: None,
            to_type: None,
        });
        *summary.entry("drop_table".into()).or_insert(0) += 1;
    }

    for name in old_names.intersection(&new_names) {
        let old_model = &old.models[*name];
        let new_model = &new.models[*name];
        let old_fields = field_map(old_model);
        let new_fields = field_map(new_model);

        for (fname, new_field) in &new_fields {
            if !old_fields.contains_key(fname) {
                changes.push(Change {
                    kind: ChangeKind::AddColumn,
                    table: old_model.name.clone(),
                    column: Some(fname.to_string()),
                    field: Some((*new_field).clone()),
                    from_type: None,
                    to_type: None,
                });
                *summary.entry("add_column".into()).or_insert(0) += 1;
            }
        }

        for (fname, old_field) in &old_fields {
            if !new_fields.contains_key(fname) {
                if old_field.is_id {
                    continue;
                }
                changes.push(Change {
                    kind: ChangeKind::DropColumn,
                    table: old_model.name.clone(),
                    column: Some(fname.to_string()),
                    field: None,
                    from_type: None,
                    to_type: None,
                });
                *summary.entry("drop_column".into()).or_insert(0) += 1;
            }
        }

        for (fname, new_field) in &new_fields {
            if let Some(old_field) = old_fields.get(fname) {
                if old_field.ty != new_field.ty {
                    changes.push(Change {
                        kind: ChangeKind::AlterColumn,
                        table: old_model.name.clone(),
                        column: Some(fname.to_string()),
                        field: None,
                        from_type: Some(old_field.ty.sql_type(Dialect::Sqlite).to_string()),
                        to_type: Some(new_field.ty.sql_type(Dialect::Sqlite).to_string()),
                    });
                    *summary.entry("alter_column".into()).or_insert(0) += 1;
                }
            }
        }
    }

    (changes, summary)
}

fn sql_for_changes(
    changes: &[Change],
    old: &SchemaHandle,
    new: &SchemaHandle,
    dialect: Dialect,
) -> Vec<String> {
    let mut out = Vec::new();
    for ch in changes {
        match ch.kind {
            ChangeKind::CreateTable => {
                if let Some(model) = new.models.get(&ch.table) {
                    out.push(create_table_sql(model, dialect));
                }
            }
            ChangeKind::DropTable => out.extend(change_to_sql(ch, dialect)),
            ChangeKind::AddColumn => out.extend(change_to_sql(ch, dialect)),
            ChangeKind::DropColumn => out.extend(change_to_sql(ch, dialect)),
            ChangeKind::AlterColumn => out.extend(change_to_sql(ch, dialect)),
        }
    }
    let _ = old;
    out
}

fn change_to_value(ch: &Change) -> ValueRef {
    let mut map = HashMap::new();
    let kind = match ch.kind {
        ChangeKind::CreateTable => "create_table",
        ChangeKind::DropTable => "drop_table",
        ChangeKind::AddColumn => "add_column",
        ChangeKind::DropColumn => "drop_column",
        ChangeKind::AlterColumn => "alter_column",
    };
    map.insert("kind".to_string(), Value::String(kind.into()).ref_cell());
    map.insert(
        "table".to_string(),
        Value::String(ch.table.clone()).ref_cell(),
    );
    if let Some(ref col) = ch.column {
        map.insert("column".to_string(), Value::String(col.clone()).ref_cell());
    }
    if let Some(ref from) = ch.from_type {
        map.insert(
            "from_type".to_string(),
            Value::String(from.clone()).ref_cell(),
        );
    }
    if let Some(ref to) = ch.to_type {
        map.insert("to_type".to_string(), Value::String(to.clone()).ref_cell());
    }
    if let Some(ref field) = ch.field {
        if ch.kind != ChangeKind::CreateTable {
            let mut fmap = HashMap::new();
            fmap.insert(
                "name".to_string(),
                Value::String(field.name.clone()).ref_cell(),
            );
            let ty = match field.ty {
                FieldType::Int => "int",
                FieldType::Float => "float",
                FieldType::Str => "string",
                FieldType::Bool => "bool",
                FieldType::Datetime => "datetime",
            };
            fmap.insert("type".to_string(), Value::String(ty.into()).ref_cell());
            map.insert("field".to_string(), Value::Object(fmap).ref_cell());
        }
    }
    Value::Object(map).ref_cell()
}

fn summary_to_value(summary: &HashMap<String, i64>) -> ValueRef {
    let mut map = HashMap::new();
    for (k, v) in summary {
        map.insert(k.clone(), Value::Int(*v).ref_cell());
    }
    Value::Object(map).ref_cell()
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// nmigrate_diff(old_schema, new_schema) → {changes, summary}
fn nmigrate_diff(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmigrate_diff", span)?;
    let old = parse_schema(&args[0], span)?;
    let new = parse_schema(&args[1], span)?;
    let (changes, summary) = diff_schemas(&old, &new);
    let mut out = HashMap::new();
    out.insert(
        "changes".to_string(),
        Value::Array(changes.iter().map(change_to_value).collect()).ref_cell(),
    );
    out.insert("summary".to_string(), summary_to_value(&summary));
    Ok(Value::Object(out).ref_cell())
}

/// nmigrate_sql(changes, dialect?) → [sql, ...]
fn nmigrate_sql(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nmigrate_sql", span)?;
    let dialect = default_dialect(args, 2, "nmigrate_sql", span)?;
    let old_schema = parse_schema(&args[0], span)?;
    let new_schema = parse_schema(&args[1], span)?;
    let (changes, _) = diff_schemas(&old_schema, &new_schema);
    let sql = sql_for_changes(&changes, &old_schema, &new_schema, dialect);
    Ok(Value::Array(
        sql.into_iter()
            .map(|s| Value::String(s).ref_cell())
            .collect(),
    )
    .ref_cell())
}

/// nmigrate_plan(old_schema, new_schema, dialect?) → {changes, summary, sql}
fn nmigrate_plan(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nmigrate_plan", span)?;
    let dialect = default_dialect(args, 2, "nmigrate_plan", span)?;
    let old = parse_schema(&args[0], span)?;
    let new = parse_schema(&args[1], span)?;
    let (changes, summary) = diff_schemas(&old, &new);
    let sql = sql_for_changes(&changes, &old, &new, dialect);
    let mut out = HashMap::new();
    out.insert(
        "changes".to_string(),
        Value::Array(changes.iter().map(change_to_value).collect()).ref_cell(),
    );
    out.insert("summary".to_string(), summary_to_value(&summary));
    out.insert(
        "sql".to_string(),
        Value::Array(
            sql.into_iter()
                .map(|s| Value::String(s).ref_cell())
                .collect(),
        )
        .ref_cell(),
    );
    Ok(Value::Object(out).ref_cell())
}

/// nmigrate_dialect(name) → normalized dialect string or error
fn nmigrate_dialect(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmigrate_dialect", span)?;
    let d = dialect_arg(args, 0, "nmigrate_dialect", span)?;
    let name = match d {
        Dialect::Sqlite => "sqlite",
        Dialect::Pg => "pg",
    };
    Ok(Value::String(name.into()).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nmigrate_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nmigrate_fns![
    ("nmigrate_diff", "diff", nmigrate_diff),
    ("nmigrate_sql", "sql", nmigrate_sql),
    ("nmigrate_plan", "plan", nmigrate_plan),
    ("nmigrate_dialect", "dialect", nmigrate_dialect),
];

fn all_builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs()
        .into_iter()
        .map(|(flat, _, f)| (flat, f))
        .collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nmigrate";
pub const MODULE_PATHS: &[&str] = &["nmigrate", "std/nmigrate"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    fn schema_obj(models: HashMap<String, HashMap<String, ValueRef>>) -> ValueRef {
        let mut outer = HashMap::new();
        let mut model_map = HashMap::new();
        for (model_name, fields) in models {
            let mut field_obj = HashMap::new();
            for (fname, spec) in fields {
                field_obj.insert(fname, spec);
            }
            model_map.insert(model_name, Value::Object(field_obj).ref_cell());
        }
        outer.insert("models".to_string(), Value::Object(model_map).ref_cell());
        Value::Object(outer).ref_cell()
    }

    fn spec(s: &str) -> ValueRef {
        Value::String(s.into()).ref_cell()
    }

    #[test]
    fn diff_add_table_and_column() {
        let old = schema_obj(HashMap::from([(
            "User".into(),
            HashMap::from([
                ("id".into(), spec("int@id")),
                ("name".into(), spec("string@required")),
            ]),
        )]));
        let new = schema_obj(HashMap::from([
            (
                "User".into(),
                HashMap::from([
                    ("id".into(), spec("int@id")),
                    ("name".into(), spec("string@required")),
                    ("email".into(), spec("string@unique")),
                ]),
            ),
            (
                "Post".into(),
                HashMap::from([
                    ("id".into(), spec("int@id")),
                    ("title".into(), spec("string@required")),
                ]),
            ),
        ]));

        let plan = nmigrate_plan(&[old, new], span()).unwrap();
        let plan_b = plan.borrow();
        match &*plan_b {
            Value::Object(map) => {
                let summary_b = map.get("summary").unwrap().borrow();
                match &*summary_b {
                    Value::Object(s) => {
                        assert_eq!(s.get("create_table").unwrap().borrow().to_string(), "1");
                        assert_eq!(s.get("add_column").unwrap().borrow().to_string(), "1");
                    }
                    other => panic!("expected summary object, got {other:?}"),
                }
                let sql_b = map.get("sql").unwrap().borrow();
                match &*sql_b {
                    Value::Array(sql) => {
                        assert!(sql.len() >= 2);
                        let joined: String = sql
                            .iter()
                            .map(|s| s.borrow().to_string())
                            .collect::<Vec<_>>()
                            .join("\n");
                        assert!(joined.contains("CREATE TABLE IF NOT EXISTS \"Post\""));
                        assert!(joined.contains("ADD COLUMN \"email\""));
                    }
                    other => panic!("expected sql array, got {other:?}"),
                }
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn diff_drop_table() {
        let old = schema_obj(HashMap::from([(
            "Legacy".into(),
            HashMap::from([("id".into(), spec("int@id"))]),
        )]));
        let new = schema_obj(HashMap::new());
        let plan = nmigrate_plan(&[old, new], span()).unwrap();
        let plan_b = plan.borrow();
        match &*plan_b {
            Value::Object(map) => {
                let sql_b = map.get("sql").unwrap().borrow();
                match &*sql_b {
                    Value::Array(sql) => {
                        assert_eq!(sql.len(), 1);
                        assert!(sql[0].borrow().to_string().contains("DROP TABLE"));
                    }
                    other => panic!("expected sql array, got {other:?}"),
                }
            }
            other => panic!("expected object, got {other:?}"),
        }
    }
}
