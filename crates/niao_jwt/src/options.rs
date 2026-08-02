use crate::algo::parse_alg;
use jsonwebtoken::Validation;

#[derive(Debug, Clone)]
pub struct VerifyOptions {
    pub algorithms: Vec<String>,
    pub validate_exp: bool,
    pub validate_nbf: bool,
    pub validate_iat: bool,
    pub leeway: u64,
    pub audience: Option<String>,
    pub issuer: Option<String>,
    pub subject: Option<String>,
    pub required_claims: Vec<String>,
}

impl Default for VerifyOptions {
    fn default() -> Self {
        Self {
            algorithms: vec!["HS256".into()],
            validate_exp: true,
            validate_nbf: false,
            validate_iat: false,
            leeway: 0,
            audience: None,
            issuer: None,
            subject: None,
            required_claims: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SignOptions {
    pub alg: String,
    pub kid: Option<String>,
    pub typ: Option<String>,
    pub extra_header: Vec<(String, String)>,
}

impl Default for SignOptions {
    fn default() -> Self {
        Self {
            alg: "HS256".into(),
            kid: None,
            typ: Some("JWT".into()),
            extra_header: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FetchOptions {
    pub timeout_ms: u64,
    pub user_agent: Option<String>,
    pub max_bytes: usize,
}

impl FetchOptions {
    pub const DEFAULT_MAX_BYTES: usize = 1024 * 1024;
}

pub fn to_validation(opts: &VerifyOptions) -> Result<Validation, crate::error::JwtError> {
    let primary = opts
        .algorithms
        .first()
        .map(|s| s.as_str())
        .unwrap_or("HS256");
    let mut validation = Validation::new(parse_alg(primary)?);
    if opts.algorithms.len() > 1 {
        validation.algorithms = opts
            .algorithms
            .iter()
            .map(|s| parse_alg(s))
            .collect::<Result<Vec<_>, _>>()?;
    }
    validation.validate_exp = opts.validate_exp;
    validation.validate_nbf = opts.validate_nbf;
    validation.leeway = opts.leeway;
    validation.validate_aud = opts.audience.is_some();
    if let Some(aud) = &opts.audience {
        validation.set_audience(&[aud.as_str()]);
    }
    if let Some(iss) = &opts.issuer {
        validation.set_issuer(&[iss.as_str()]);
    }
    if let Some(sub) = &opts.subject {
        validation.sub = Some(sub.clone());
    }
    for claim in &opts.required_claims {
        validation.required_spec_claims.insert(claim.clone());
    }
    Ok(validation)
}
