//! Query AST and parser (Whoosh-inspired subset).

use crate::tokenize::tokenize;

/// A parsed query node.
#[derive(Debug, Clone, PartialEq)]
pub enum Query {
    /// Single term in an optional field (None = default / all indexed fields).
    Term {
        field: Option<String>,
        term: String,
    },
    /// Prefix term (`foo*`).
    Prefix {
        field: Option<String>,
        prefix: String,
    },
    /// Ordered phrase of terms.
    Phrase {
        field: Option<String>,
        terms: Vec<String>,
    },
    And(Vec<Query>),
    Or(Vec<Query>),
    Not(Box<Query>),
}

/// Parse a Whoosh-like query string.
///
/// Supports: bare terms, `field:term`, `"phrase query"`, `prefix*`,
/// `AND` / `OR` / `NOT` (case-insensitive), and parentheses.
pub fn parse(input: &str) -> Query {
    let tokens = lex(input);
    if tokens.is_empty() {
        return Query::And(vec![]);
    }
    let mut i = 0;
    parse_or(&tokens, &mut i)
}

fn parse_or(tokens: &[Tok], i: &mut usize) -> Query {
    let mut parts = vec![parse_and(tokens, i)];
    while matches!(tokens.get(*i), Some(Tok::Or)) {
        *i += 1;
        parts.push(parse_and(tokens, i));
    }
    if parts.len() == 1 {
        parts.pop().unwrap()
    } else {
        Query::Or(parts)
    }
}

fn parse_and(tokens: &[Tok], i: &mut usize) -> Query {
    let mut parts = vec![parse_not(tokens, i)];
    loop {
        match tokens.get(*i) {
            Some(Tok::And) => {
                *i += 1;
                parts.push(parse_not(tokens, i));
            }
            Some(Tok::Or) | Some(Tok::RParen) | None => break,
            Some(_) => {
                // Implicit AND between adjacent terms.
                parts.push(parse_not(tokens, i));
            }
        }
    }
    if parts.len() == 1 {
        parts.pop().unwrap()
    } else {
        Query::And(parts)
    }
}

fn parse_not(tokens: &[Tok], i: &mut usize) -> Query {
    if matches!(tokens.get(*i), Some(Tok::Not)) {
        *i += 1;
        Query::Not(Box::new(parse_not(tokens, i)))
    } else {
        parse_primary(tokens, i)
    }
}

fn parse_primary(tokens: &[Tok], i: &mut usize) -> Query {
    match tokens.get(*i).cloned() {
        Some(Tok::LParen) => {
            *i += 1;
            let q = parse_or(tokens, i);
            if matches!(tokens.get(*i), Some(Tok::RParen)) {
                *i += 1;
            }
            q
        }
        Some(Tok::Phrase(s)) => {
            *i += 1;
            let terms = tokenize(&s);
            Query::Phrase { field: None, terms }
        }
        Some(Tok::FieldedPhrase { field, phrase }) => {
            *i += 1;
            Query::Phrase {
                field: Some(field),
                terms: tokenize(&phrase),
            }
        }
        Some(Tok::Word(w)) => {
            *i += 1;
            word_to_query(None, &w)
        }
        Some(Tok::Fielded { field, value }) => {
            *i += 1;
            word_to_query(Some(field), &value)
        }
        _ => {
            *i += 1;
            Query::And(vec![])
        }
    }
}

fn word_to_query(field: Option<String>, raw: &str) -> Query {
    let lower: String = raw.chars().flat_map(|c| c.to_lowercase()).collect();
    if let Some(stripped) = lower.strip_suffix('*') {
        Query::Prefix {
            field,
            prefix: stripped.to_string(),
        }
    } else {
        let terms = tokenize(&lower);
        if terms.len() == 1 {
            Query::Term {
                field,
                term: terms[0].clone(),
            }
        } else if terms.is_empty() {
            Query::And(vec![])
        } else {
            Query::Phrase { field, terms }
        }
    }
}

#[derive(Debug, Clone)]
enum Tok {
    Word(String),
    Phrase(String),
    Fielded { field: String, value: String },
    FieldedPhrase { field: String, phrase: String },
    And,
    Or,
    Not,
    LParen,
    RParen,
}

fn lex(input: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '(' => {
                out.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                out.push(Tok::RParen);
                i += 1;
            }
            '"' => {
                i += 1;
                let start = i;
                while i < chars.len() && chars[i] != '"' {
                    i += 1;
                }
                let phrase: String = chars[start..i].iter().collect();
                if i < chars.len() {
                    i += 1;
                }
                out.push(Tok::Phrase(phrase));
            }
            _ => {
                let start = i;
                while i < chars.len()
                    && !chars[i].is_whitespace()
                    && chars[i] != '('
                    && chars[i] != ')'
                {
                    if chars[i] == '"' {
                        break;
                    }
                    i += 1;
                }
                let raw: String = chars[start..i].iter().collect();
                let upper = raw.to_ascii_uppercase();
                match upper.as_str() {
                    "AND" => out.push(Tok::And),
                    "OR" => out.push(Tok::Or),
                    "NOT" => out.push(Tok::Not),
                    _ => {
                        if let Some((field, rest)) = split_field(&raw) {
                            if rest.starts_with('"') && rest.ends_with('"') && rest.len() >= 2 {
                                let phrase = rest[1..rest.len() - 1].to_string();
                                out.push(Tok::FieldedPhrase { field, phrase });
                            } else {
                                out.push(Tok::Fielded { field, value: rest });
                            }
                        } else {
                            out.push(Tok::Word(raw));
                        }
                    }
                }
            }
        }
    }
    out
}

fn split_field(raw: &str) -> Option<(String, String)> {
    let idx = raw.find(':')?;
    if idx == 0 {
        return None;
    }
    let field = raw[..idx].to_string();
    let value = raw[idx + 1..].to_string();
    if field.is_empty() || value.is_empty() {
        None
    } else {
        Some((field, value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_and() {
        let q = parse("quick brown");
        match q {
            Query::And(parts) => assert_eq!(parts.len(), 2),
            _ => panic!("expected And"),
        }
    }

    #[test]
    fn phrase() {
        let q = parse(r#""quick brown""#);
        match q {
            Query::Phrase { terms, .. } => {
                assert_eq!(terms, vec!["quick".to_string(), "brown".to_string()]);
            }
            _ => panic!("expected Phrase"),
        }
    }

    #[test]
    fn prefix_and_field() {
        let q = parse("title:cat*");
        match q {
            Query::Prefix { field, prefix } => {
                assert_eq!(field.as_deref(), Some("title"));
                assert_eq!(prefix, "cat");
            }
            _ => panic!("expected Prefix"),
        }
    }

    #[test]
    fn boolean_or() {
        let q = parse("cats OR dogs");
        assert!(matches!(q, Query::Or(_)));
    }
}
