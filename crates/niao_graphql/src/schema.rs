//! Compiled GraphQL schema (from SDL).

use crate::ast::*;
use crate::error::{GqlError, GqlResult};
use crate::parser::parse_schema;
use crate::printer::print_schema_document;
use std::collections::HashMap;

/// Built-in scalar names.
const BUILTIN_SCALARS: &[&str] = &["Int", "Float", "String", "Boolean", "ID"];

/// Runtime schema with indexed types.
#[derive(Debug, Clone)]
pub struct Schema {
    pub document: SchemaDocument,
    pub types: HashMap<String, SchemaType>,
    pub query_type: String,
    pub mutation_type: Option<String>,
    pub subscription_type: Option<String>,
}

#[derive(Debug, Clone)]
pub enum SchemaType {
    Scalar {
        name: String,
        description: Option<String>,
    },
    Object {
        name: String,
        description: Option<String>,
        implements: Vec<String>,
        fields: HashMap<String, FieldDefinition>,
    },
    Interface {
        name: String,
        description: Option<String>,
        implements: Vec<String>,
        fields: HashMap<String, FieldDefinition>,
    },
    Union {
        name: String,
        description: Option<String>,
        members: Vec<String>,
    },
    Enum {
        name: String,
        description: Option<String>,
        values: Vec<String>,
    },
    InputObject {
        name: String,
        description: Option<String>,
        fields: HashMap<String, InputValueDefinition>,
    },
}

impl SchemaType {
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Scalar { .. } => "SCALAR",
            Self::Object { .. } => "OBJECT",
            Self::Interface { .. } => "INTERFACE",
            Self::Union { .. } => "UNION",
            Self::Enum { .. } => "ENUM",
            Self::InputObject { .. } => "INPUT_OBJECT",
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Scalar { name, .. }
            | Self::Object { name, .. }
            | Self::Interface { name, .. }
            | Self::Union { name, .. }
            | Self::Enum { name, .. }
            | Self::InputObject { name, .. } => name,
        }
    }

    pub fn field(&self, name: &str) -> Option<&FieldDefinition> {
        match self {
            Self::Object { fields, .. } | Self::Interface { fields, .. } => fields.get(name),
            _ => None,
        }
    }
}

impl Schema {
    /// Build a schema from SDL text.
    pub fn parse(sdl: &str) -> GqlResult<Self> {
        let document = parse_schema(sdl)?;
        Self::from_document(document)
    }

    pub fn from_document(document: SchemaDocument) -> GqlResult<Self> {
        let mut types: HashMap<String, SchemaType> = HashMap::new();
        for name in BUILTIN_SCALARS {
            types.insert(
                (*name).to_string(),
                SchemaType::Scalar {
                    name: (*name).to_string(),
                    description: None,
                },
            );
        }

        let mut query_type = None;
        let mut mutation_type = None;
        let mut subscription_type = None;

        for def in &document.definitions {
            match def {
                TypeSystemDefinition::Schema(s) => {
                    query_type = s.query.clone();
                    mutation_type = s.mutation.clone();
                    subscription_type = s.subscription.clone();
                }
                TypeSystemDefinition::Scalar(s) => {
                    types.insert(
                        s.name.clone(),
                        SchemaType::Scalar {
                            name: s.name.clone(),
                            description: s.description.clone(),
                        },
                    );
                }
                TypeSystemDefinition::Object(o) => {
                    let mut fields = HashMap::new();
                    for f in &o.fields {
                        fields.insert(f.name.clone(), f.clone());
                    }
                    types.insert(
                        o.name.clone(),
                        SchemaType::Object {
                            name: o.name.clone(),
                            description: o.description.clone(),
                            implements: o.implements.clone(),
                            fields,
                        },
                    );
                    // Convention: type Query is the query root if schema block omitted
                    if o.name == "Query" && query_type.is_none() {
                        query_type = Some("Query".into());
                    }
                    if o.name == "Mutation" && mutation_type.is_none() {
                        mutation_type = Some("Mutation".into());
                    }
                    if o.name == "Subscription" && subscription_type.is_none() {
                        subscription_type = Some("Subscription".into());
                    }
                }
                TypeSystemDefinition::Interface(i) => {
                    let mut fields = HashMap::new();
                    for f in &i.fields {
                        fields.insert(f.name.clone(), f.clone());
                    }
                    types.insert(
                        i.name.clone(),
                        SchemaType::Interface {
                            name: i.name.clone(),
                            description: i.description.clone(),
                            implements: i.implements.clone(),
                            fields,
                        },
                    );
                }
                TypeSystemDefinition::Union(u) => {
                    types.insert(
                        u.name.clone(),
                        SchemaType::Union {
                            name: u.name.clone(),
                            description: u.description.clone(),
                            members: u.members.clone(),
                        },
                    );
                }
                TypeSystemDefinition::Enum(e) => {
                    types.insert(
                        e.name.clone(),
                        SchemaType::Enum {
                            name: e.name.clone(),
                            description: e.description.clone(),
                            values: e.values.iter().map(|v| v.name.clone()).collect(),
                        },
                    );
                }
                TypeSystemDefinition::InputObject(i) => {
                    let mut fields = HashMap::new();
                    for f in &i.fields {
                        fields.insert(f.name.clone(), f.clone());
                    }
                    types.insert(
                        i.name.clone(),
                        SchemaType::InputObject {
                            name: i.name.clone(),
                            description: i.description.clone(),
                            fields,
                        },
                    );
                }
            }
        }

        let query_type = query_type.ok_or_else(|| {
            GqlError::new(
                "schema must define a query root type (schema { query: ... } or type Query)",
            )
        })?;
        if !types.contains_key(&query_type) {
            return Err(GqlError::new(format!(
                "query root type '{query_type}' is not defined"
            )));
        }

        Ok(Schema {
            document,
            types,
            query_type,
            mutation_type,
            subscription_type,
        })
    }

    pub fn type_names(&self) -> Vec<String> {
        let mut names: Vec<_> = self.types.keys().cloned().collect();
        names.sort();
        names
    }

    pub fn has_type(&self, name: &str) -> bool {
        self.types.contains_key(name)
    }

    pub fn get_type(&self, name: &str) -> Option<&SchemaType> {
        self.types.get(name)
    }

    pub fn print(&self) -> String {
        print_schema_document(&self.document)
    }

    /// Describe a type as a JSON-friendly structure summary.
    pub fn describe_type(&self, name: &str) -> GqlResult<serde_json::Value> {
        let ty = self
            .get_type(name)
            .ok_or_else(|| GqlError::new(format!("unknown type '{name}'")))?;
        let mut obj = serde_json::Map::new();
        obj.insert(
            "kind".into(),
            serde_json::Value::String(ty.kind_name().into()),
        );
        obj.insert("name".into(), serde_json::Value::String(ty.name().into()));
        match ty {
            SchemaType::Object {
                description,
                implements,
                fields,
                ..
            }
            | SchemaType::Interface {
                description,
                implements,
                fields,
                ..
            } => {
                if let Some(d) = description {
                    obj.insert("description".into(), serde_json::Value::String(d.clone()));
                }
                obj.insert(
                    "implements".into(),
                    serde_json::Value::Array(
                        implements
                            .iter()
                            .map(|s| serde_json::Value::String(s.clone()))
                            .collect(),
                    ),
                );
                let mut field_list = Vec::new();
                let mut names: Vec<_> = fields.keys().collect();
                names.sort();
                for fname in names {
                    let f = &fields[fname];
                    let mut fm = serde_json::Map::new();
                    fm.insert("name".into(), serde_json::Value::String(f.name.clone()));
                    fm.insert(
                        "type".into(),
                        serde_json::Value::String(type_to_string(&f.ty)),
                    );
                    field_list.push(serde_json::Value::Object(fm));
                }
                obj.insert("fields".into(), serde_json::Value::Array(field_list));
            }
            SchemaType::Enum {
                description,
                values,
                ..
            } => {
                if let Some(d) = description {
                    obj.insert("description".into(), serde_json::Value::String(d.clone()));
                }
                obj.insert(
                    "values".into(),
                    serde_json::Value::Array(
                        values
                            .iter()
                            .map(|s| serde_json::Value::String(s.clone()))
                            .collect(),
                    ),
                );
            }
            SchemaType::Union {
                description,
                members,
                ..
            } => {
                if let Some(d) = description {
                    obj.insert("description".into(), serde_json::Value::String(d.clone()));
                }
                obj.insert(
                    "members".into(),
                    serde_json::Value::Array(
                        members
                            .iter()
                            .map(|s| serde_json::Value::String(s.clone()))
                            .collect(),
                    ),
                );
            }
            SchemaType::InputObject {
                description,
                fields,
                ..
            } => {
                if let Some(d) = description {
                    obj.insert("description".into(), serde_json::Value::String(d.clone()));
                }
                let mut field_list = Vec::new();
                let mut names: Vec<_> = fields.keys().collect();
                names.sort();
                for fname in names {
                    let f = &fields[fname];
                    let mut fm = serde_json::Map::new();
                    fm.insert("name".into(), serde_json::Value::String(f.name.clone()));
                    fm.insert(
                        "type".into(),
                        serde_json::Value::String(type_to_string(&f.ty)),
                    );
                    field_list.push(serde_json::Value::Object(fm));
                }
                obj.insert("fields".into(), serde_json::Value::Array(field_list));
            }
            SchemaType::Scalar { description, .. } => {
                if let Some(d) = description {
                    obj.insert("description".into(), serde_json::Value::String(d.clone()));
                }
            }
        }
        Ok(serde_json::Value::Object(obj))
    }
}

pub fn type_to_string(ty: &TypeNode) -> String {
    match ty {
        TypeNode::Named(n) => n.clone(),
        TypeNode::List(inner) => format!("[{}]", type_to_string(inner)),
        TypeNode::NonNull(inner) => format!("{}!", type_to_string(inner)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_from_convention_query() {
        let sdl = r#"
type Query {
  hello: String!
}
"#;
        let schema = Schema::parse(sdl).unwrap();
        assert_eq!(schema.query_type, "Query");
        assert!(schema.has_type("Query"));
        assert!(schema.has_type("String"));
    }
}
