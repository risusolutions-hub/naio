use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OAuthError {
    Config(String),
    Http(String),
    Parse(String),
    Token(String),
    State(String),
    Pkce(String),
    IdToken(String),
    Discovery(String),
    Revocation(String),
    Introspection(String),
    Userinfo(String),
}

impl OAuthError {
    pub fn message(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for OAuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(s) => write!(f, "config: {s}"),
            Self::Http(s) => write!(f, "http: {s}"),
            Self::Parse(s) => write!(f, "parse: {s}"),
            Self::Token(s) => write!(f, "token: {s}"),
            Self::State(s) => write!(f, "state: {s}"),
            Self::Pkce(s) => write!(f, "pkce: {s}"),
            Self::IdToken(s) => write!(f, "id_token: {s}"),
            Self::Discovery(s) => write!(f, "discovery: {s}"),
            Self::Revocation(s) => write!(f, "revocation: {s}"),
            Self::Introspection(s) => write!(f, "introspection: {s}"),
            Self::Userinfo(s) => write!(f, "userinfo: {s}"),
        }
    }
}

impl std::error::Error for OAuthError {}

pub type OAuthResult<T> = Result<T, OAuthError>;
