use std::fmt;

#[derive(Debug, Clone)]
pub enum JwtError {
    Format,
    Base64,
    Json(String),
    Algorithm,
    Signature,
    Expired,
    NotBefore,
    Immature,
    Audience,
    Issuer,
    Subject,
    Key(String),
    Jwks(String),
    Fetch(String),
    Message(String),
}

impl JwtError {
    pub fn message(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for JwtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Format => write!(f, "invalid JWT format"),
            Self::Base64 => write!(f, "invalid base64url segment"),
            Self::Json(e) => write!(f, "invalid JWT JSON: {e}"),
            Self::Algorithm => write!(f, "unsupported or forbidden JWT algorithm"),
            Self::Signature => write!(f, "invalid JWT signature"),
            Self::Expired => write!(f, "JWT expired"),
            Self::NotBefore => write!(f, "JWT not yet valid (nbf)"),
            Self::Immature => write!(f, "JWT issued in the future (iat)"),
            Self::Audience => write!(f, "invalid JWT audience"),
            Self::Issuer => write!(f, "invalid JWT issuer"),
            Self::Subject => write!(f, "invalid JWT subject"),
            Self::Key(s) => write!(f, "invalid key: {s}"),
            Self::Jwks(s) => write!(f, "invalid JWKS: {s}"),
            Self::Fetch(s) => write!(f, "JWKS fetch failed: {s}"),
            Self::Message(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for JwtError {}

impl From<jsonwebtoken::errors::Error> for JwtError {
    fn from(e: jsonwebtoken::errors::Error) -> Self {
        use jsonwebtoken::errors::ErrorKind;
        match e.kind() {
            ErrorKind::InvalidToken => Self::Format,
            ErrorKind::InvalidSignature => Self::Signature,
            ErrorKind::ExpiredSignature => Self::Expired,
            ErrorKind::ImmatureSignature => Self::NotBefore,
            ErrorKind::InvalidAlgorithmName => Self::Algorithm,
            ErrorKind::InvalidAudience => Self::Audience,
            ErrorKind::InvalidIssuer => Self::Issuer,
            ErrorKind::InvalidSubject => Self::Subject,
            ErrorKind::Base64(_) => Self::Base64,
            ErrorKind::Json(err) => Self::Json(err.to_string()),
            other => Self::Message(format!("{other:?}")),
        }
    }
}
