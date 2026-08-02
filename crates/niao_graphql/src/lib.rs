//! GraphQL document/schema parse, validate, and execute for Niao (`ngraphql`).
//!
//! Zero GraphQL-crate dependency: hand-rolled lexer/parser matching the GraphQL
//! October 2021 executable + SDL subset needed for clients and local servers.
//! Map-based execution (~graphene/strawberry default property resolvers).

mod ast;
mod error;
mod execute;
mod fragment;
mod lexer;
mod parser;
mod printer;
mod request;
mod schema;
mod validate;

pub use ast::{
    Definition, Document, Field, FragmentDefinition, OperationDefinition, OperationType,
    SchemaDocument, Selection, SelectionSet, TypeNode, ValueNode,
};
pub use error::{GqlError, GqlResult};
pub use execute::{execute, execute_doc, ExecutionResult};
pub use fragment::{list_fragments, list_operations, spread_fragments, variable_names};
pub use parser::{is_document, is_schema, parse_document, parse_schema};
pub use printer::{minify_document, print_document, print_schema_document};
pub use request::{
    fragment_summary, gql, minify, operation_summary, request, request_json, vars_from_json,
};
pub use schema::{type_to_string, Schema, SchemaType};
pub use validate::{validate, ValidationResult};

/// Maximum source size (16 MiB guard).
pub const MAX_SOURCE: usize = 16 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SDL: &str = r#"
schema { query: Query mutation: Mutation }
type Query {
  hero(id: ID): Character
  heroes: [Character!]!
  hello: String!
}
type Mutation {
  setMessage(message: String!): String!
}
type Character {
  id: ID!
  name: String!
  friends: [Character!]
}
"#;

    #[test]
    fn end_to_end_execute() {
        let schema = Schema::parse(SDL).unwrap();
        let root = json!({
            "hello": "world",
            "hero": [
                {"id": "1", "name": "Luke", "friends": [{"id": "2", "name": "Leia", "friends": []}]},
                {"id": "2", "name": "Leia", "friends": []}
            ],
            "heroes": [
                {"id": "1", "name": "Luke", "friends": []},
                {"id": "2", "name": "Leia", "friends": []}
            ]
        });
        let q = r#"
query GetHero($id: ID) {
  hello
  hero(id: $id) {
    name
    friends { name }
  }
}
"#;
        let mut vars = serde_json::Map::new();
        vars.insert("id".into(), json!("1"));
        let result = execute(&schema, q, &root, &vars, None).unwrap();
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        let data = result.data.unwrap();
        assert_eq!(data["hello"], "world");
        assert_eq!(data["hero"]["name"], "Luke");
        assert_eq!(data["hero"]["friends"][0]["name"], "Leia");
    }

    #[test]
    fn fragments_and_validate() {
        let schema = Schema::parse(SDL).unwrap();
        let q = r#"
query {
  heroes { ...CharFields }
}
fragment CharFields on Character {
  name
}
"#;
        let doc = parse_document(q).unwrap();
        let v = validate(&doc, &schema, None).unwrap();
        assert!(v.ok, "{:?}", v.errors);
        let spread = spread_fragments(&doc).unwrap();
        assert!(list_fragments(&spread).is_empty());
    }

    #[test]
    fn request_body() {
        let body = request_json("{ hello }", None, None).unwrap();
        assert!(body.contains("\"query\""));
    }

    #[test]
    fn skip_include_directives() {
        let schema = Schema::parse(SDL).unwrap();
        let root = json!({"hello": "world", "heroes": []});
        let q = r#"query ($show: Boolean!) { hello @include(if: $show) }"#;
        let mut vars = serde_json::Map::new();
        vars.insert("show".into(), json!(false));
        let result = execute(&schema, q, &root, &vars, None).unwrap();
        let data = result.data.unwrap();
        assert!(data.get("hello").is_none());
    }
}
