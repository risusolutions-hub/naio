//! Load sklearn reference fixtures from embedded JSON (no serde dependency).

use crate::error::{BoostError, BoostResult};

pub struct SklearnFixtures {
    pub reg_x: Vec<f64>,
    pub reg_y: Vec<f64>,
    pub sk_rmse: f64,
    pub bin_x: Vec<f64>,
    pub bin_y: Vec<f64>,
    pub sk_logloss: f64,
    pub sk_auc: f64,
}

pub fn load_sklearn_fixtures() -> BoostResult<SklearnFixtures> {
    let text = include_str!("../tests/sklearn_fixtures.json");
    Ok(SklearnFixtures {
        reg_x: parse_f64_array(text, "reg_x")?,
        reg_y: parse_f64_array(text, "reg_y")?,
        sk_rmse: parse_f64_field(text, "sk_rmse")?,
        bin_x: parse_f64_array(text, "bin_x")?,
        bin_y: parse_f64_array(text, "bin_y")?,
        sk_logloss: parse_f64_field(text, "sk_logloss")?,
        sk_auc: parse_f64_field(text, "sk_auc")?,
    })
}

fn parse_f64_field(text: &str, key: &str) -> BoostResult<f64> {
    let needle = format!("\"{key}\":");
    let i = text.find(&needle).ok_or_else(|| BoostError::Error(format!("missing {key}")))?;
    let rest = &text[i + needle.len()..];
    let end = rest.find([',', '}']).unwrap_or(rest.len());
    rest[..end]
        .trim()
        .parse::<f64>()
        .map_err(|e| BoostError::Error(e.to_string()))
}

fn parse_f64_array(text: &str, key: &str) -> BoostResult<Vec<f64>> {
    let needle = format!("\"{key}\":");
    let i = text.find(&needle).ok_or_else(|| BoostError::Error(format!("missing {key}")))?;
    let rest = &text[i + needle.len()..];
    let start = rest.find('[').ok_or_else(|| BoostError::Error(format!("missing [{key}")))?;
    let end = rest[start..]
        .find(']')
        .ok_or_else(|| BoostError::Error(format!("missing ]{key}")))?;
    let inner = &rest[start + 1..start + end];
    let mut out = Vec::new();
    for part in inner.split(',') {
        let p = part.trim();
        if !p.is_empty() {
            out.push(
                p.parse::<f64>()
                    .map_err(|e| BoostError::Error(e.to_string()))?,
            );
        }
    }
    Ok(out)
}
