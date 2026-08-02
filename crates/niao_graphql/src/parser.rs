//! GraphQL recursive-descent parser for executable documents and SDL.

use crate::ast::*;
use crate::error::{GqlError, GqlResult};
use crate::lexer::{Lexer, Token, TokenKind};

struct Parser {
    tokens: Vec<Token>,
    idx: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, idx: 0 }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.idx]
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.peek().kind
    }

    fn at_name(&self, name: &str) -> bool {
        matches!(self.peek_kind(), TokenKind::Name(n) if n == name)
    }

    fn bump(&mut self) -> &Token {
        let t = &self.tokens[self.idx];
        if !matches!(t.kind, TokenKind::Eof) {
            self.idx += 1;
        }
        t
    }

    fn expect(&mut self, kind: TokenKind) -> GqlResult<&Token> {
        let tok = self.peek();
        if std::mem::discriminant(&tok.kind) == std::mem::discriminant(&kind) {
            Ok(self.bump())
        } else {
            Err(GqlError::parse(
                format!("expected {:?}, got {:?}", kind, tok.kind),
                tok.line,
                tok.column,
            ))
        }
    }

    fn expect_name(&mut self) -> GqlResult<String> {
        let tok = self.peek().clone();
        match &tok.kind {
            TokenKind::Name(n) => {
                self.bump();
                Ok(n.clone())
            }
            _ => Err(GqlError::parse(
                format!("expected name, got {:?}", tok.kind),
                tok.line,
                tok.column,
            )),
        }
    }

    fn unexpected(&self) -> GqlError {
        let tok = self.peek();
        GqlError::parse(
            format!("unexpected token {:?}", tok.kind),
            tok.line,
            tok.column,
        )
    }

    // ---- Document ----

    fn parse_document(&mut self) -> GqlResult<Document> {
        self.expect(TokenKind::Sof)?;
        let mut definitions = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::Eof) {
            definitions.push(self.parse_definition()?);
        }
        self.expect(TokenKind::Eof)?;
        if definitions.is_empty() {
            return Err(GqlError::new(
                "document must contain at least one definition",
            ));
        }
        Ok(Document {
            definitions,
            source: String::new(),
        })
    }

    fn parse_definition(&mut self) -> GqlResult<Definition> {
        if self.at_name("fragment") {
            return Ok(Definition::Fragment(self.parse_fragment_definition()?));
        }
        if matches!(self.peek_kind(), TokenKind::BraceL)
            || self.at_name("query")
            || self.at_name("mutation")
            || self.at_name("subscription")
        {
            return Ok(Definition::Operation(self.parse_operation_definition()?));
        }
        Err(self.unexpected())
    }

    fn parse_operation_definition(&mut self) -> GqlResult<OperationDefinition> {
        if matches!(self.peek_kind(), TokenKind::BraceL) {
            return Ok(OperationDefinition {
                operation: OperationType::Query,
                name: None,
                variables: Vec::new(),
                directives: Vec::new(),
                selection_set: self.parse_selection_set()?,
            });
        }
        let op_name = self.expect_name()?;
        let operation = match op_name.as_str() {
            "query" => OperationType::Query,
            "mutation" => OperationType::Mutation,
            "subscription" => OperationType::Subscription,
            _ => return Err(GqlError::new(format!("unknown operation type '{op_name}'"))),
        };
        let name = if matches!(self.peek_kind(), TokenKind::Name(_)) {
            Some(self.expect_name()?)
        } else {
            None
        };
        let variables = if matches!(self.peek_kind(), TokenKind::ParenL) {
            self.parse_variable_definitions()?
        } else {
            Vec::new()
        };
        let directives = self.parse_directives()?;
        let selection_set = self.parse_selection_set()?;
        Ok(OperationDefinition {
            operation,
            name,
            variables,
            directives,
            selection_set,
        })
    }

    fn parse_variable_definitions(&mut self) -> GqlResult<Vec<VariableDefinition>> {
        self.expect(TokenKind::ParenL)?;
        let mut vars = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::ParenR) {
            vars.push(self.parse_variable_definition()?);
        }
        self.expect(TokenKind::ParenR)?;
        Ok(vars)
    }

    fn parse_variable_definition(&mut self) -> GqlResult<VariableDefinition> {
        self.expect(TokenKind::Dollar)?;
        let name = self.expect_name()?;
        self.expect(TokenKind::Colon)?;
        let ty = self.parse_type()?;
        let default_value = if matches!(self.peek_kind(), TokenKind::Equals) {
            self.bump();
            Some(self.parse_value(false)?)
        } else {
            None
        };
        let directives = self.parse_directives()?;
        Ok(VariableDefinition {
            name,
            ty,
            default_value,
            directives,
        })
    }

    fn parse_fragment_definition(&mut self) -> GqlResult<FragmentDefinition> {
        self.expect_name()?; // fragment
        let name = self.expect_name()?;
        if name == "on" {
            return Err(GqlError::new("fragment name cannot be 'on'"));
        }
        if !self.at_name("on") {
            return Err(self.unexpected());
        }
        self.bump();
        let type_condition = self.expect_name()?;
        let directives = self.parse_directives()?;
        let selection_set = self.parse_selection_set()?;
        Ok(FragmentDefinition {
            name,
            type_condition,
            directives,
            selection_set,
        })
    }

    fn parse_selection_set(&mut self) -> GqlResult<SelectionSet> {
        self.expect(TokenKind::BraceL)?;
        let mut selections = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::BraceR) {
            selections.push(self.parse_selection()?);
        }
        self.expect(TokenKind::BraceR)?;
        if selections.is_empty() {
            return Err(GqlError::new("selection set must not be empty"));
        }
        Ok(SelectionSet { selections })
    }

    fn parse_selection(&mut self) -> GqlResult<Selection> {
        if matches!(self.peek_kind(), TokenKind::Spread) {
            self.bump();
            if self.at_name("on") || matches!(self.peek_kind(), TokenKind::At | TokenKind::BraceL) {
                return Ok(Selection::InlineFragment(self.parse_inline_fragment()?));
            }
            return Ok(Selection::FragmentSpread(self.parse_fragment_spread()?));
        }
        Ok(Selection::Field(self.parse_field()?))
    }

    fn parse_field(&mut self) -> GqlResult<Field> {
        let name_or_alias = self.expect_name()?;
        let (alias, name) = if matches!(self.peek_kind(), TokenKind::Colon) {
            self.bump();
            (Some(name_or_alias), self.expect_name()?)
        } else {
            (None, name_or_alias)
        };
        let arguments = if matches!(self.peek_kind(), TokenKind::ParenL) {
            self.parse_arguments()?
        } else {
            Vec::new()
        };
        let directives = self.parse_directives()?;
        let selection_set = if matches!(self.peek_kind(), TokenKind::BraceL) {
            Some(self.parse_selection_set()?)
        } else {
            None
        };
        Ok(Field {
            alias,
            name,
            arguments,
            directives,
            selection_set,
        })
    }

    fn parse_fragment_spread(&mut self) -> GqlResult<FragmentSpread> {
        let name = self.expect_name()?;
        let directives = self.parse_directives()?;
        Ok(FragmentSpread { name, directives })
    }

    fn parse_inline_fragment(&mut self) -> GqlResult<InlineFragment> {
        let type_condition = if self.at_name("on") {
            self.bump();
            Some(self.expect_name()?)
        } else {
            None
        };
        let directives = self.parse_directives()?;
        let selection_set = self.parse_selection_set()?;
        Ok(InlineFragment {
            type_condition,
            directives,
            selection_set,
        })
    }

    fn parse_arguments(&mut self) -> GqlResult<Vec<Argument>> {
        self.expect(TokenKind::ParenL)?;
        let mut args = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::ParenR) {
            let name = self.expect_name()?;
            self.expect(TokenKind::Colon)?;
            let value = self.parse_value(true)?;
            args.push(Argument { name, value });
        }
        self.expect(TokenKind::ParenR)?;
        Ok(args)
    }

    fn parse_directives(&mut self) -> GqlResult<Vec<Directive>> {
        let mut dirs = Vec::new();
        while matches!(self.peek_kind(), TokenKind::At) {
            self.bump();
            let name = self.expect_name()?;
            let arguments = if matches!(self.peek_kind(), TokenKind::ParenL) {
                self.parse_arguments()?
            } else {
                Vec::new()
            };
            dirs.push(Directive { name, arguments });
        }
        Ok(dirs)
    }

    fn parse_type(&mut self) -> GqlResult<TypeNode> {
        let ty = if matches!(self.peek_kind(), TokenKind::BracketL) {
            self.bump();
            let inner = self.parse_type()?;
            self.expect(TokenKind::BracketR)?;
            TypeNode::List(Box::new(inner))
        } else {
            TypeNode::Named(self.expect_name()?)
        };
        if matches!(self.peek_kind(), TokenKind::Bang) {
            self.bump();
            Ok(TypeNode::NonNull(Box::new(ty)))
        } else {
            Ok(ty)
        }
    }

    fn parse_value(&mut self, const_only: bool) -> GqlResult<ValueNode> {
        let _ = const_only;
        let tok = self.peek().clone();
        match tok.kind {
            TokenKind::BracketL => {
                self.bump();
                let mut items = Vec::new();
                while !matches!(self.peek_kind(), TokenKind::BracketR) {
                    items.push(self.parse_value(const_only)?);
                }
                self.expect(TokenKind::BracketR)?;
                Ok(ValueNode::List(items))
            }
            TokenKind::BraceL => {
                self.bump();
                let mut fields = Vec::new();
                while !matches!(self.peek_kind(), TokenKind::BraceR) {
                    let name = self.expect_name()?;
                    self.expect(TokenKind::Colon)?;
                    let value = self.parse_value(const_only)?;
                    fields.push((name, value));
                }
                self.expect(TokenKind::BraceR)?;
                Ok(ValueNode::Object(fields))
            }
            TokenKind::Dollar => {
                self.bump();
                let name = self.expect_name()?;
                Ok(ValueNode::Variable(name))
            }
            TokenKind::Int(s) => {
                self.bump();
                let n: i64 = s.parse().map_err(|_| {
                    GqlError::at(format!("invalid int '{s}'"), tok.line, tok.column)
                })?;
                Ok(ValueNode::Int(n))
            }
            TokenKind::Float(s) => {
                self.bump();
                let n: f64 = s.parse().map_err(|_| {
                    GqlError::at(format!("invalid float '{s}'"), tok.line, tok.column)
                })?;
                Ok(ValueNode::Float(n))
            }
            TokenKind::String(s) | TokenKind::BlockString(s) => {
                self.bump();
                Ok(ValueNode::String(s))
            }
            TokenKind::Name(n) => {
                self.bump();
                match n.as_str() {
                    "true" => Ok(ValueNode::Boolean(true)),
                    "false" => Ok(ValueNode::Boolean(false)),
                    "null" => Ok(ValueNode::Null),
                    _ => Ok(ValueNode::Enum(n)),
                }
            }
            _ => Err(self.unexpected()),
        }
    }

    // ---- Schema ----

    fn parse_schema_document(&mut self) -> GqlResult<SchemaDocument> {
        self.expect(TokenKind::Sof)?;
        let mut definitions = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::Eof) {
            definitions.push(self.parse_type_system_definition()?);
        }
        self.expect(TokenKind::Eof)?;
        if definitions.is_empty() {
            return Err(GqlError::new("schema must contain at least one definition"));
        }
        Ok(SchemaDocument {
            definitions,
            source: String::new(),
        })
    }

    fn parse_description(&mut self) -> Option<String> {
        match self.peek_kind() {
            TokenKind::String(s) | TokenKind::BlockString(s) => {
                let out = s.clone();
                self.bump();
                Some(out)
            }
            _ => None,
        }
    }

    fn parse_type_system_definition(&mut self) -> GqlResult<TypeSystemDefinition> {
        let description = self.parse_description();
        if self.at_name("schema") {
            return Ok(TypeSystemDefinition::Schema(
                self.parse_schema_definition(description)?,
            ));
        }
        if self.at_name("scalar") {
            return Ok(TypeSystemDefinition::Scalar(
                self.parse_scalar(description)?,
            ));
        }
        if self.at_name("type") {
            return Ok(TypeSystemDefinition::Object(
                self.parse_object_type(description)?,
            ));
        }
        if self.at_name("interface") {
            return Ok(TypeSystemDefinition::Interface(
                self.parse_interface(description)?,
            ));
        }
        if self.at_name("union") {
            return Ok(TypeSystemDefinition::Union(self.parse_union(description)?));
        }
        if self.at_name("enum") {
            return Ok(TypeSystemDefinition::Enum(self.parse_enum(description)?));
        }
        if self.at_name("input") {
            return Ok(TypeSystemDefinition::InputObject(
                self.parse_input_object(description)?,
            ));
        }
        Err(self.unexpected())
    }

    fn parse_schema_definition(
        &mut self,
        description: Option<String>,
    ) -> GqlResult<SchemaDefinition> {
        self.expect_name()?; // schema
        let directives = self.parse_directives()?;
        self.expect(TokenKind::BraceL)?;
        let mut query = None;
        let mut mutation = None;
        let mut subscription = None;
        while !matches!(self.peek_kind(), TokenKind::BraceR) {
            let op = self.expect_name()?;
            self.expect(TokenKind::Colon)?;
            let ty = self.expect_name()?;
            match op.as_str() {
                "query" => query = Some(ty),
                "mutation" => mutation = Some(ty),
                "subscription" => subscription = Some(ty),
                other => return Err(GqlError::new(format!("unknown schema operation '{other}'"))),
            }
        }
        self.expect(TokenKind::BraceR)?;
        Ok(SchemaDefinition {
            description,
            directives,
            query,
            mutation,
            subscription,
        })
    }

    fn parse_scalar(&mut self, description: Option<String>) -> GqlResult<ScalarTypeDefinition> {
        self.expect_name()?; // scalar
        let name = self.expect_name()?;
        let directives = self.parse_directives()?;
        Ok(ScalarTypeDefinition {
            description,
            name,
            directives,
        })
    }

    fn parse_implements(&mut self) -> GqlResult<Vec<String>> {
        if !self.at_name("implements") {
            return Ok(Vec::new());
        }
        self.bump();
        let mut names = Vec::new();
        // optional leading &
        if matches!(self.peek_kind(), TokenKind::Amp) {
            self.bump();
        }
        names.push(self.expect_name()?);
        while matches!(self.peek_kind(), TokenKind::Amp) {
            self.bump();
            names.push(self.expect_name()?);
        }
        Ok(names)
    }

    fn parse_object_type(
        &mut self,
        description: Option<String>,
    ) -> GqlResult<ObjectTypeDefinition> {
        self.expect_name()?; // type
        let name = self.expect_name()?;
        let implements = self.parse_implements()?;
        let directives = self.parse_directives()?;
        let fields = if matches!(self.peek_kind(), TokenKind::BraceL) {
            self.parse_fields_definition()?
        } else {
            Vec::new()
        };
        Ok(ObjectTypeDefinition {
            description,
            name,
            implements,
            directives,
            fields,
        })
    }

    fn parse_interface(
        &mut self,
        description: Option<String>,
    ) -> GqlResult<InterfaceTypeDefinition> {
        self.expect_name()?; // interface
        let name = self.expect_name()?;
        let implements = self.parse_implements()?;
        let directives = self.parse_directives()?;
        let fields = if matches!(self.peek_kind(), TokenKind::BraceL) {
            self.parse_fields_definition()?
        } else {
            Vec::new()
        };
        Ok(InterfaceTypeDefinition {
            description,
            name,
            implements,
            directives,
            fields,
        })
    }

    fn parse_fields_definition(&mut self) -> GqlResult<Vec<FieldDefinition>> {
        self.expect(TokenKind::BraceL)?;
        let mut fields = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::BraceR) {
            fields.push(self.parse_field_definition()?);
        }
        self.expect(TokenKind::BraceR)?;
        Ok(fields)
    }

    fn parse_field_definition(&mut self) -> GqlResult<FieldDefinition> {
        let description = self.parse_description();
        let name = self.expect_name()?;
        let arguments = if matches!(self.peek_kind(), TokenKind::ParenL) {
            self.parse_argument_defs()?
        } else {
            Vec::new()
        };
        self.expect(TokenKind::Colon)?;
        let ty = self.parse_type()?;
        let directives = self.parse_directives()?;
        Ok(FieldDefinition {
            description,
            name,
            arguments,
            ty,
            directives,
        })
    }

    fn parse_argument_defs(&mut self) -> GqlResult<Vec<InputValueDefinition>> {
        self.expect(TokenKind::ParenL)?;
        let mut args = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::ParenR) {
            args.push(self.parse_input_value_def()?);
        }
        self.expect(TokenKind::ParenR)?;
        Ok(args)
    }

    fn parse_input_value_def(&mut self) -> GqlResult<InputValueDefinition> {
        let description = self.parse_description();
        let name = self.expect_name()?;
        self.expect(TokenKind::Colon)?;
        let ty = self.parse_type()?;
        let default_value = if matches!(self.peek_kind(), TokenKind::Equals) {
            self.bump();
            Some(self.parse_value(true)?)
        } else {
            None
        };
        let directives = self.parse_directives()?;
        Ok(InputValueDefinition {
            description,
            name,
            ty,
            default_value,
            directives,
        })
    }

    fn parse_union(&mut self, description: Option<String>) -> GqlResult<UnionTypeDefinition> {
        self.expect_name()?; // union
        let name = self.expect_name()?;
        let directives = self.parse_directives()?;
        let mut members = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Equals) {
            self.bump();
            if matches!(self.peek_kind(), TokenKind::Pipe) {
                self.bump();
            }
            members.push(self.expect_name()?);
            while matches!(self.peek_kind(), TokenKind::Pipe) {
                self.bump();
                members.push(self.expect_name()?);
            }
        }
        Ok(UnionTypeDefinition {
            description,
            name,
            directives,
            members,
        })
    }

    fn parse_enum(&mut self, description: Option<String>) -> GqlResult<EnumTypeDefinition> {
        self.expect_name()?; // enum
        let name = self.expect_name()?;
        let directives = self.parse_directives()?;
        let mut values = Vec::new();
        if matches!(self.peek_kind(), TokenKind::BraceL) {
            self.bump();
            while !matches!(self.peek_kind(), TokenKind::BraceR) {
                let desc = self.parse_description();
                let vname = self.expect_name()?;
                let dir = self.parse_directives()?;
                values.push(EnumValueDefinition {
                    description: desc,
                    name: vname,
                    directives: dir,
                });
            }
            self.expect(TokenKind::BraceR)?;
        }
        Ok(EnumTypeDefinition {
            description,
            name,
            directives,
            values,
        })
    }

    fn parse_input_object(
        &mut self,
        description: Option<String>,
    ) -> GqlResult<InputObjectTypeDefinition> {
        self.expect_name()?; // input
        let name = self.expect_name()?;
        let directives = self.parse_directives()?;
        let mut fields = Vec::new();
        if matches!(self.peek_kind(), TokenKind::BraceL) {
            self.bump();
            while !matches!(self.peek_kind(), TokenKind::BraceR) {
                fields.push(self.parse_input_value_def()?);
            }
            self.expect(TokenKind::BraceR)?;
        }
        Ok(InputObjectTypeDefinition {
            description,
            name,
            directives,
            fields,
        })
    }
}

/// Parse an executable GraphQL document.
pub fn parse_document(source: &str) -> GqlResult<Document> {
    if source.len() > crate::MAX_SOURCE {
        return Err(GqlError::new(format!(
            "source exceeds maximum size of {} bytes",
            crate::MAX_SOURCE
        )));
    }
    let tokens = Lexer::new(source).tokenize()?;
    let mut parser = Parser::new(tokens);
    let mut doc = parser.parse_document()?;
    doc.source = source.to_string();
    Ok(doc)
}

/// Parse a GraphQL schema (SDL) document.
pub fn parse_schema(source: &str) -> GqlResult<SchemaDocument> {
    if source.len() > crate::MAX_SOURCE {
        return Err(GqlError::new(format!(
            "source exceeds maximum size of {} bytes",
            crate::MAX_SOURCE
        )));
    }
    let tokens = Lexer::new(source).tokenize()?;
    let mut parser = Parser::new(tokens);
    let mut doc = parser.parse_schema_document()?;
    doc.source = source.to_string();
    Ok(doc)
}

/// True when `source` parses as an executable document.
pub fn is_document(source: &str) -> bool {
    parse_document(source).is_ok()
}

/// True when `source` parses as SDL.
pub fn is_schema(source: &str) -> bool {
    parse_schema(source).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_anonymous_query() {
        let doc = parse_document("{ hero { name } }").unwrap();
        assert_eq!(doc.definitions.len(), 1);
    }

    #[test]
    fn parse_named_with_vars_and_fragment() {
        let src = r#"
query Hero($id: ID!) {
  hero(id: $id) { ...HeroFields }
}
fragment HeroFields on Character {
  name
  friends { name }
}
"#;
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.definitions.len(), 2);
    }

    #[test]
    fn parse_schema_sdl() {
        let src = r#"
schema { query: Query }
type Query {
  hero(id: ID!): Character
}
type Character {
  name: String!
  friends: [Character!]
}
"#;
        let schema = parse_schema(src).unwrap();
        assert!(schema.definitions.len() >= 3);
    }

    #[test]
    fn reject_empty() {
        assert!(parse_document("").is_err());
        assert!(parse_document("   ").is_err());
    }

    #[test]
    fn reject_malformed() {
        assert!(parse_document("{").is_err());
        assert!(parse_document("{ hero { name }").is_err());
    }
}
