//! Password strength estimation and policy validation.

use crate::common::{char_classes, is_common_password, shannon_entropy};
use crate::error::{check_password_len, PassResult};

#[derive(Debug, Clone)]
pub struct Policy {
    pub min_length: usize,
    pub max_length: usize,
    pub min_upper: usize,
    pub min_lower: usize,
    pub min_digit: usize,
    pub min_special: usize,
    pub min_entropy: f64,
    pub min_score: u8,
    pub forbid_common: bool,
    pub forbid_sequential: bool,
    pub forbid_repeated: bool,
    pub forbidden: Vec<String>,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            min_length: 8,
            max_length: 128,
            min_upper: 0,
            min_lower: 0,
            min_digit: 0,
            min_special: 0,
            min_entropy: 0.0,
            min_score: 0,
            forbid_common: true,
            forbid_sequential: true,
            forbid_repeated: true,
            forbidden: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CharClasses {
    pub upper: usize,
    pub lower: usize,
    pub digit: usize,
    pub special: usize,
    pub other: usize,
}

#[derive(Debug, Clone)]
pub struct StrengthReport {
    pub ok: bool,
    pub score: u8,
    pub entropy: f64,
    pub length: usize,
    pub issues: Vec<String>,
    pub classes: CharClasses,
}

impl Policy {
    pub fn validate(&self, password: &str) -> StrengthReport {
        let mut issues = Vec::new();
        let length = password.chars().count();
        let classes = char_classes(password);
        let entropy = estimate_entropy(password, &classes);

        if length < self.min_length {
            issues.push(format!("too short (min {})", self.min_length));
        }
        if length > self.max_length {
            issues.push(format!("too long (max {})", self.max_length));
        }
        if classes.upper < self.min_upper {
            issues.push(format!("needs {} uppercase letter(s)", self.min_upper));
        }
        if classes.lower < self.min_lower {
            issues.push(format!("needs {} lowercase letter(s)", self.min_lower));
        }
        if classes.digit < self.min_digit {
            issues.push(format!("needs {} digit(s)", self.min_digit));
        }
        if classes.special < self.min_special {
            issues.push(format!("needs {} special character(s)", self.min_special));
        }
        if entropy < self.min_entropy {
            issues.push(format!(
                "entropy {:.1} below minimum {:.1}",
                entropy, self.min_entropy
            ));
        }

        let score = score_password(password, &classes, entropy);
        if score < self.min_score {
            issues.push(format!("score {score} below minimum {}", self.min_score));
        }
        if self.forbid_common && is_common_password(password) {
            issues.push("password is too common".into());
        }
        if self.forbid_sequential && has_sequential_run(password, 4) {
            issues.push("contains sequential characters".into());
        }
        if self.forbid_repeated && has_repeated_run(password, 4) {
            issues.push("contains repeated characters".into());
        }
        for word in &self.forbidden {
            if !word.is_empty()
                && password
                    .to_ascii_lowercase()
                    .contains(&word.to_ascii_lowercase())
            {
                issues.push(format!("contains forbidden word '{word}'"));
            }
        }

        StrengthReport {
            ok: issues.is_empty(),
            score,
            entropy,
            length,
            issues,
            classes,
        }
    }
}

pub fn check_strength(password: &str) -> PassResult<StrengthReport> {
    check_password_len(password)?;
    Ok(Policy::default().validate(password))
}

pub fn estimate_entropy(password: &str, classes: &CharClasses) -> f64 {
    let shannon = shannon_entropy(password);
    let pool = (classes.upper > 0) as u32 * 26
        + (classes.lower > 0) as u32 * 26
        + (classes.digit > 0) as u32 * 10
        + (classes.special > 0) as u32 * 32
        + (classes.other > 0) as u32 * 64;
    let pool_entropy = if pool > 0 {
        (password.chars().count() as f64) * (pool as f64).log2()
    } else {
        0.0
    };
    shannon.max(pool_entropy * 0.6)
}

pub fn score_password(password: &str, classes: &CharClasses, entropy: f64) -> u8 {
    let len = password.chars().count();
    let mut score: i32 = 0;

    if len >= 8 {
        score += 1;
    }
    if len >= 12 {
        score += 1;
    }
    if len >= 16 {
        score += 1;
    }

    let kinds = (classes.upper > 0) as i32
        + (classes.lower > 0) as i32
        + (classes.digit > 0) as i32
        + (classes.special > 0) as i32;
    score += kinds - 1;

    if entropy >= 28.0 {
        score += 1;
    }
    if entropy >= 45.0 {
        score += 1;
    }

    if is_common_password(password) {
        score -= 3;
    }
    if has_sequential_run(password, 4) {
        score -= 1;
    }
    if has_repeated_run(password, 4) {
        score -= 1;
    }

    score.clamp(0, 4) as u8
}

fn has_sequential_run(password: &str, min_len: usize) -> bool {
    let bytes = password.as_bytes();
    if bytes.len() < min_len {
        return false;
    }
    let mut run = 1usize;
    for w in bytes.windows(2) {
        let a = w[0];
        let b = w[1];
        if a.is_ascii_alphanumeric() && b.is_ascii_alphanumeric() {
            if b == a.wrapping_add(1) || b == a.wrapping_sub(1) {
                run += 1;
                if run >= min_len {
                    return true;
                }
                continue;
            }
        }
        run = 1;
    }
    false
}

fn has_repeated_run(password: &str, min_len: usize) -> bool {
    let mut run = 1usize;
    let mut prev = '\0';
    for ch in password.chars() {
        if ch == prev {
            run += 1;
            if run >= min_len {
                return true;
            }
        } else {
            run = 1;
            prev = ch;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weak_password_fails_default() {
        let r = check_strength("password").unwrap();
        assert!(!r.ok);
        assert!(r.score <= 1);
    }

    #[test]
    fn strong_password_ok() {
        let p = Policy {
            min_length: 12,
            min_upper: 1,
            min_lower: 1,
            min_digit: 1,
            min_special: 1,
            min_score: 3,
            ..Default::default()
        };
        let r = p.validate("Tr0ub4dor&3Extra!");
        assert!(r.ok);
        assert!(r.score >= 3);
    }
}
