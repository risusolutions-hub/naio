//! Native nscaffold standard library — generate CRUD routes, nmodel schema,
//! SQL migration, and ntest stubs from a struct spec object.
//!
//! Import with `import "nscaffold"` (or `import "std/nscaffold"`).

use crate::nmodel::schema::{parse_field, FieldType, ModelDef};
use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::collections::HashMap;
use std::rc::Rc;

const E3250_NSCAFFOLD_ARITY: u32 = 3250;
const E3251_NSCAFFOLD_ERROR: u32 = 3251;
const E3252_NSCAFFOLD_TYPE: u32 = 3252;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3250_NSCAFFOLD_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3252_NSCAFFOLD_TYPE, msg.into())
}

fn scaffold_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3251_NSCAFFOLD_ERROR, "nscaffold_error", msg.into(), span)
}

fn spec_object(args: &[ValueRef], span: Span) -> NiaoResult<HashMap<String, ValueRef>> {
    match &*args[0].borrow() {
        Value::Object(m) => Ok(m.clone()),
        other => Err(type_err(
            span,
            format!(
                "nscaffold spec must be an object, got {}",
                other.type_name()
            ),
        )),
    }
}

fn spec_str(map: &HashMap<String, ValueRef>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.clone()),
        _ => None,
    })
}

fn snake_case(s: &str) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

fn pluralize(s: &str) -> String {
    if s.ends_with('s') {
        format!("{s}es")
    } else if s.ends_with('y') && s.len() > 1 {
        format!("{}ies", &s[..s.len() - 1])
    } else {
        format!("{s}s")
    }
}

// ---------------------------------------------------------------------------
// Spec parsing
// ---------------------------------------------------------------------------

struct ScaffoldSpec {
    name: String,
    table: String,
    path: String,
    fields: HashMap<String, String>,
}

fn parse_spec(map: &HashMap<String, ValueRef>, span: Span) -> Result<ScaffoldSpec, ValueRef> {
    let name = spec_str(map, "name").ok_or_else(|| {
        scaffold_err(span, "spec missing 'name' (struct/model name)")
    })?;
    let table = spec_str(map, "table").unwrap_or_else(|| pluralize(&snake_case(&name)));
    let path = spec_str(map, "path").unwrap_or_else(|| format!("/{}", table));
    let fields_ref = map.get("fields").ok_or_else(|| {
        scaffold_err(span, "spec missing 'fields' object")
    })?;
    let fields_borrow = fields_ref.borrow();
    let fields_obj = match &*fields_borrow {
        Value::Object(f) => f.clone(),
        other => {
            return Err(scaffold_err(
                span,
                format!("spec.fields must be an object, got {}", other.type_name()),
            ));
        }
    };
    let mut fields = HashMap::new();
    for (k, v) in fields_obj {
        match &*v.borrow() {
            Value::String(s) => {
                fields.insert(k.clone(), s.clone());
            }
            other => {
                return Err(scaffold_err(
                    span,
                    format!("field '{k}' spec must be a string, got {}", other.type_name()),
                ));
            }
        }
    }
    if fields.is_empty() {
        return Err(scaffold_err(span, "spec.fields must not be empty"));
    }
    Ok(ScaffoldSpec {
        name,
        table,
        path,
        fields,
    })
}

fn to_model_def(spec: &ScaffoldSpec, span: Span) -> Result<ModelDef, ValueRef> {
    let mut field_defs = Vec::new();
    for (fname, fspec) in &spec.fields {
        match parse_field(fname, fspec, span) {
            Ok(f) => field_defs.push(f),
            Err(e) => return Err(scaffold_err(span, e.to_string())),
        }
    }
    Ok(ModelDef {
        name: spec.name.clone(),
        fields: field_defs,
    })
}

fn model_schema_object(spec: &ScaffoldSpec) -> ValueRef {
    let mut field_map = HashMap::new();
    for (k, v) in &spec.fields {
        field_map.insert(k.clone(), Value::String(v.clone()).ref_cell());
    }
    let mut model = HashMap::new();
    model.insert("fields".to_string(), Value::Object(field_map).ref_cell());
    let mut models = HashMap::new();
    models.insert(spec.name.clone(), Value::Object(model).ref_cell());
    let mut root = HashMap::new();
    root.insert("models".to_string(), Value::Object(models).ref_cell());
    Value::Object(root).ref_cell()
}

fn gen_routes(spec: &ScaffoldSpec) -> String {
    let name = &spec.name;
    let path = &spec.path;
    let id_path = format!("{path}/{{id}}");
    format!(
        r#"// CRUD routes for {name}
get "{path}" {{
    let rows = nmodel.find_many(c, "{name}", {{}})
    return {{status: 200, body: rows}}
}}

get "{id_path}" {{
    let row = nmodel.find_unique(c, "{name}", {{where: {{id: req.params.id}}}})
    if row == nil {{
        return {{status: 404, body: {{error: "not found"}}}}
    }}
    return {{status: 200, body: row}}
}}

post "{path}" {{
    let row = nmodel.create(c, "{name}", req.body)
    return {{status: 201, body: row}}
}}

put "{id_path}" {{
    let row = nmodel.update(c, "{name}", {{
        where: {{id: req.params.id}},
        data: req.body
    }})
    return {{status: 200, body: row}}
}}

delete "{id_path}" {{
    let n = nmodel.delete(c, "{name}", {{where: {{id: req.params.id}}}})
    return {{status: 200, body: {{deleted: n}}}}
}}
"#
    )
}

fn sqlite_create_table(model: &ModelDef) -> String {
    let mut col_defs: Vec<String> = Vec::new();
    let mut unique_constraints: Vec<String> = Vec::new();
    for field in &model.fields {
        if field.is_id {
            col_defs.push(format!(
                "  \"{}\" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL",
                field.name
            ));
        } else {
            let sql_ty = match field.ty {
                FieldType::Int => "INTEGER",
                FieldType::Float => "REAL",
                FieldType::Str => "TEXT",
                FieldType::Bool => "INTEGER",
                FieldType::Datetime => "TEXT",
            };
            let mut col = format!("  \"{}\" {}", field.name, sql_ty);
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

fn gen_migration(spec: &ScaffoldSpec, span: Span) -> Result<String, ValueRef> {
    let model = to_model_def(spec, span)?;
    Ok(sqlite_create_table(&model))
}

fn gen_tests(spec: &ScaffoldSpec) -> String {
    let name = &spec.name;
    let table = &spec.table;
    format!(
        r#"import "ntest"
import "nmodel"
import "nsqlite"

fn test_{table}_crud() {{
    let db = nsqlite.open(":memory:")
    let s = nmodel.schema({{
        models: {{
            {name}: {{
                fields: {fields_literal}
            }}
        }}
    }})
    let c = nmodel.bind(s, db)
    nmodel.migrate(c)

    let row = nmodel.create(c, "{name}", {{}})
    ntest.assert_not_error(row)

    let found = nmodel.find_unique(c, "{name}", {{where: {{id: row.id}}}})
    ntest.assert_not_error(found)
}}

ntest.case("{name} CRUD scaffold", test_{table}_crud)
"#,
        fields_literal = fields_literal(&spec.fields),
    )
}

fn fields_literal(fields: &HashMap<String, String>) -> String {
    let mut keys: Vec<&String> = fields.keys().collect();
    keys.sort();
    let parts: Vec<String> = keys
        .into_iter()
        .map(|k| format!("{k}: \"{}\"", fields.get(k).unwrap().replace('"', "\\\"")))
        .collect();
    format!("{{{}}}", parts.join(", "))
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn nscaffold_routes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nscaffold_routes", span)?;
    let map = spec_object(args, span)?;
    let spec = match parse_spec(&map, span) {
        Ok(s) => s,
        Err(e) => return Ok(e),
    };
    Ok(Value::String(gen_routes(&spec)).ref_cell())
}

fn nscaffold_model(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nscaffold_model", span)?;
    let map = spec_object(args, span)?;
    let spec = match parse_spec(&map, span) {
        Ok(s) => s,
        Err(e) => return Ok(e),
    };
    Ok(model_schema_object(&spec))
}

fn nscaffold_migration(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nscaffold_migration", span)?;
    let map = spec_object(args, span)?;
    let spec = match parse_spec(&map, span) {
        Ok(s) => s,
        Err(e) => return Ok(e),
    };
    match gen_migration(&spec, span) {
        Ok(sql) => Ok(Value::String(sql).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nscaffold_tests(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nscaffold_tests", span)?;
    let map = spec_object(args, span)?;
    let spec = match parse_spec(&map, span) {
        Ok(s) => s,
        Err(e) => return Ok(e),
    };
    Ok(Value::String(gen_tests(&spec)).ref_cell())
}

fn nscaffold_crud(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nscaffold_crud", span)?;
    let map = spec_object(args, span)?;
    let spec = match parse_spec(&map, span) {
        Ok(s) => s,
        Err(e) => return Ok(e),
    };
    let migration = match gen_migration(&spec, span) {
        Ok(m) => m,
        Err(e) => return Ok(e),
    };
    let mut out = HashMap::new();
    out.insert("name".to_string(), Value::String(spec.name.clone()).ref_cell());
    out.insert("table".to_string(), Value::String(spec.table.clone()).ref_cell());
    out.insert("path".to_string(), Value::String(spec.path.clone()).ref_cell());
    out.insert("routes".to_string(), Value::String(gen_routes(&spec)).ref_cell());
    out.insert("model".to_string(), model_schema_object(&spec));
    out.insert("migration".to_string(), Value::String(migration).ref_cell());
    out.insert("tests".to_string(), Value::String(gen_tests(&spec)).ref_cell());
    Ok(Value::Object(out).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nscaffold_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nscaffold_fns![
    ("nscaffold_crud", "crud", nscaffold_crud),
    ("nscaffold_routes", "routes", nscaffold_routes),
    ("nscaffold_model", "model", nscaffold_model),
    ("nscaffold_migration", "migration", nscaffold_migration),
    ("nscaffold_tests", "tests", nscaffold_tests),
];

fn all_builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nscaffold";
pub const MODULE_PATHS: &[&str] = &["nscaffold", "std/nscaffold"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::dummy()
    }

    fn user_spec() -> ValueRef {
        let mut fields = HashMap::new();
        fields.insert("id".to_string(), Value::String("int@id".into()).ref_cell());
        fields.insert("name".to_string(), Value::String("string@required".into()).ref_cell());
        fields.insert("email".to_string(), Value::String("string@unique@required".into()).ref_cell());
        let mut spec = HashMap::new();
        spec.insert("name".to_string(), Value::String("User".into()).ref_cell());
        spec.insert("fields".to_string(), Value::Object(fields).ref_cell());
        Value::Object(spec).ref_cell()
    }

    #[test]
    fn crud_bundle_has_all_artifacts() {
        let r = nscaffold_crud(&[user_spec()], span()).unwrap();
        let r_ref = r.borrow();
        match &*r_ref {
            Value::Object(m) => {
                for key in ["routes", "model", "migration", "tests"] {
                    assert!(m.contains_key(key), "missing {key}");
                }
                let mig_ref = m.get("migration").unwrap().borrow();
                match &*mig_ref {
                    Value::String(sql) => {
                        assert!(sql.contains("CREATE TABLE"));
                        assert!(sql.contains("User"));
                    }
                    other => panic!("expected migration string, got {other:?}"),
                }
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn routes_include_crud_verbs() {
        let r = nscaffold_routes(&[user_spec()], span()).unwrap();
        let r_ref = r.borrow();
        match &*r_ref {
            Value::String(s) => {
                assert!(s.contains("get \"/users\""));
                assert!(s.contains("post \"/users\""));
                assert!(s.contains("put \"/users/{id}\""));
                assert!(s.contains("delete \"/users/{id}\""));
            }
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn model_schema_shape() {
        let r = nscaffold_model(&[user_spec()], span()).unwrap();
        let r_ref = r.borrow();
        match &*r_ref {
            Value::Object(m) => {
                let models_ref = m.get("models").unwrap().borrow();
                match &*models_ref {
                    Value::Object(mm) => assert!(mm.contains_key("User")),
                    other => panic!("expected models object, got {other:?}"),
                }
            }
            other => panic!("expected object, got {other:?}"),
        }
    }
}
