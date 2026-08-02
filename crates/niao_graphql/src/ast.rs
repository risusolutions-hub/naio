//! GraphQL AST nodes (query documents and SDL).

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Parsed GraphQL executable document.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub definitions: Vec<Definition>,
    /// Original source kept for extract/print fidelity when unchanged.
    pub source: String,
}

/// Top-level document definition.
#[derive(Debug, Clone, PartialEq)]
pub enum Definition {
    Operation(OperationDefinition),
    Fragment(FragmentDefinition),
}

/// Operation kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OperationType {
    Query,
    Mutation,
    Subscription,
}

impl OperationType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Query => "query",
            Self::Mutation => "mutation",
            Self::Subscription => "subscription",
        }
    }
}

/// Named or anonymous operation.
#[derive(Debug, Clone, PartialEq)]
pub struct OperationDefinition {
    pub operation: OperationType,
    pub name: Option<String>,
    pub variables: Vec<VariableDefinition>,
    pub directives: Vec<Directive>,
    pub selection_set: SelectionSet,
}

/// `$var: Type! = default`
#[derive(Debug, Clone, PartialEq)]
pub struct VariableDefinition {
    pub name: String,
    pub ty: TypeNode,
    pub default_value: Option<ValueNode>,
    pub directives: Vec<Directive>,
}

/// Fragment definition.
#[derive(Debug, Clone, PartialEq)]
pub struct FragmentDefinition {
    pub name: String,
    pub type_condition: String,
    pub directives: Vec<Directive>,
    pub selection_set: SelectionSet,
}

/// Selection set `{ ... }`
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SelectionSet {
    pub selections: Vec<Selection>,
}

/// Field, fragment spread, or inline fragment.
#[derive(Debug, Clone, PartialEq)]
pub enum Selection {
    Field(Field),
    FragmentSpread(FragmentSpread),
    InlineFragment(InlineFragment),
}

/// Field selection.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub alias: Option<String>,
    pub name: String,
    pub arguments: Vec<Argument>,
    pub directives: Vec<Directive>,
    pub selection_set: Option<SelectionSet>,
}

impl Field {
    pub fn response_key(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.name)
    }
}

/// `...FragmentName`
#[derive(Debug, Clone, PartialEq)]
pub struct FragmentSpread {
    pub name: String,
    pub directives: Vec<Directive>,
}

/// `... on Type { ... }`
#[derive(Debug, Clone, PartialEq)]
pub struct InlineFragment {
    pub type_condition: Option<String>,
    pub directives: Vec<Directive>,
    pub selection_set: SelectionSet,
}

/// `@name(args)`
#[derive(Debug, Clone, PartialEq)]
pub struct Directive {
    pub name: String,
    pub arguments: Vec<Argument>,
}

/// `name: value`
#[derive(Debug, Clone, PartialEq)]
pub struct Argument {
    pub name: String,
    pub value: ValueNode,
}

/// GraphQL type reference.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeNode {
    Named(String),
    List(Box<TypeNode>),
    NonNull(Box<TypeNode>),
}

impl TypeNode {
    pub fn named_inner(&self) -> &str {
        match self {
            TypeNode::Named(n) => n,
            TypeNode::List(t) | TypeNode::NonNull(t) => t.named_inner(),
        }
    }

    pub fn is_non_null(&self) -> bool {
        matches!(self, TypeNode::NonNull(_))
    }
}

/// Literal / variable value in a document.
#[derive(Debug, Clone, PartialEq)]
pub enum ValueNode {
    Variable(String),
    Int(i64),
    Float(f64),
    String(String),
    Boolean(bool),
    Null,
    Enum(String),
    List(Vec<ValueNode>),
    Object(Vec<(String, ValueNode)>),
}

impl ValueNode {
    pub fn to_json(&self, variables: &serde_json::Map<String, JsonValue>) -> Option<JsonValue> {
        match self {
            ValueNode::Variable(name) => variables.get(name).cloned(),
            ValueNode::Int(n) => Some(JsonValue::Number((*n).into())),
            ValueNode::Float(f) => serde_json::Number::from_f64(*f).map(JsonValue::Number),
            ValueNode::String(s) => Some(JsonValue::String(s.clone())),
            ValueNode::Boolean(b) => Some(JsonValue::Bool(*b)),
            ValueNode::Null => Some(JsonValue::Null),
            ValueNode::Enum(e) => Some(JsonValue::String(e.clone())),
            ValueNode::List(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(item.to_json(variables)?);
                }
                Some(JsonValue::Array(out))
            }
            ValueNode::Object(fields) => {
                let mut map = serde_json::Map::new();
                for (k, v) in fields {
                    map.insert(k.clone(), v.to_json(variables)?);
                }
                Some(JsonValue::Object(map))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Schema SDL AST
// ---------------------------------------------------------------------------

/// Parsed GraphQL schema document (SDL).
#[derive(Debug, Clone, PartialEq)]
pub struct SchemaDocument {
    pub definitions: Vec<TypeSystemDefinition>,
    pub source: String,
}

/// Schema / type system definition.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeSystemDefinition {
    Schema(SchemaDefinition),
    Scalar(ScalarTypeDefinition),
    Object(ObjectTypeDefinition),
    Interface(InterfaceTypeDefinition),
    Union(UnionTypeDefinition),
    Enum(EnumTypeDefinition),
    InputObject(InputObjectTypeDefinition),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SchemaDefinition {
    pub description: Option<String>,
    pub directives: Vec<Directive>,
    pub query: Option<String>,
    pub mutation: Option<String>,
    pub subscription: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScalarTypeDefinition {
    pub description: Option<String>,
    pub name: String,
    pub directives: Vec<Directive>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectTypeDefinition {
    pub description: Option<String>,
    pub name: String,
    pub implements: Vec<String>,
    pub directives: Vec<Directive>,
    pub fields: Vec<FieldDefinition>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceTypeDefinition {
    pub description: Option<String>,
    pub name: String,
    pub implements: Vec<String>,
    pub directives: Vec<Directive>,
    pub fields: Vec<FieldDefinition>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnionTypeDefinition {
    pub description: Option<String>,
    pub name: String,
    pub directives: Vec<Directive>,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumTypeDefinition {
    pub description: Option<String>,
    pub name: String,
    pub directives: Vec<Directive>,
    pub values: Vec<EnumValueDefinition>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumValueDefinition {
    pub description: Option<String>,
    pub name: String,
    pub directives: Vec<Directive>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InputObjectTypeDefinition {
    pub description: Option<String>,
    pub name: String,
    pub directives: Vec<Directive>,
    pub fields: Vec<InputValueDefinition>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldDefinition {
    pub description: Option<String>,
    pub name: String,
    pub arguments: Vec<InputValueDefinition>,
    pub ty: TypeNode,
    pub directives: Vec<Directive>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InputValueDefinition {
    pub description: Option<String>,
    pub name: String,
    pub ty: TypeNode,
    pub default_value: Option<ValueNode>,
    pub directives: Vec<Directive>,
}
