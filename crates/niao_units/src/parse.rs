use crate::error::{UnitError, UnitResult};
use crate::registry::Registry;
use crate::unit::Unit;

/// Parse a unit expression such as `m/s^2`, `kg*m/s**2`, or `mile/hour`.
pub fn parse_unit_expr(input: &str, registry: &Registry) -> UnitResult<Unit> {
    let s = normalize(input);
    if s.is_empty() {
        return Err(UnitError::EmptyInput);
    }
    parse_product(&s, registry)
}

/// Parse a bare unit name or alias.
pub fn parse_unit_name(input: &str, registry: &Registry) -> UnitResult<Unit> {
    let s = normalize(input);
    if s.is_empty() {
        return Err(UnitError::EmptyInput);
    }
    registry.lookup(&s)
}

/// Parse `5 m`, `5*m`, `5.5 meters`, or `1.2e3 km/h`.
pub fn parse_quantity(input: &str, registry: &Registry) -> UnitResult<(f64, Unit)> {
    let s = normalize(input);
    if s.is_empty() {
        return Err(UnitError::EmptyInput);
    }
    let (num, rest) = parse_number_prefix(&s)?;
    let rest = rest.trim();
    if rest.is_empty() {
        return Ok((num, Unit::dimensionless()));
    }
    let unit = if let Some(rest) = rest.strip_prefix('*') {
        parse_product(rest.trim(), registry)?
    } else {
        parse_product(rest, registry)?
    };
    Ok((num, unit))
}

fn normalize(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.trim().chars() {
        match ch {
            '×' | '·' => out.push('*'),
            '÷' => out.push('/'),
            'µ' | 'μ' => {
                out.push('u');
            }
            '°' => out.push_str("deg"),
            'Ω' => {
                out.push_str("ohm");
            }
            _ => out.push(if ch == ' ' { ' ' } else { ch }),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_number_prefix(s: &str) -> UnitResult<(f64, &str)> {
    let bytes = s.as_bytes();
    let mut i = 0;
    if bytes.first() == Some(&b'-') || bytes.first() == Some(&b'+') {
        i += 1;
    }
    let start = i;
    let mut saw_dot = false;
    let mut saw_exp = false;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_digit() {
            i += 1;
        } else if b == b'.' && !saw_dot && !saw_exp {
            saw_dot = true;
            i += 1;
        } else if (b == b'e' || b == b'E') && i > start && !saw_exp {
            saw_exp = true;
            i += 1;
            if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
                i += 1;
            }
        } else {
            break;
        }
    }
    if i == start || (i == start + 1 && (bytes[start] == b'+' || bytes[start] == b'-')) {
        return Err(UnitError::Parse(format!(
            "expected number at start of '{s}'"
        )));
    }
    let num_s = &s[..i];
    let num: f64 = num_s
        .parse()
        .map_err(|_| UnitError::Parse(format!("invalid number '{num_s}'")))?;
    Ok((num, &s[i..]))
}

fn parse_product(s: &str, registry: &Registry) -> UnitResult<Unit> {
    let parts = split_top_level(s, '/');
    if parts.is_empty() {
        return Err(UnitError::EmptyInput);
    }
    let mut numer = parse_sum(parts[0].trim(), registry)?;
    for den in parts.iter().skip(1) {
        let d = parse_sum(den.trim(), registry)?;
        numer = numer.div(&d)?;
    }
    Ok(numer)
}

fn parse_sum(s: &str, registry: &Registry) -> UnitResult<Unit> {
    let mut acc: Option<Unit> = None;
    for part in split_top_level(s, '*') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let u = parse_power(part, registry)?;
        acc = Some(match acc {
            None => u,
            Some(a) => a.mul(&u)?,
        });
    }
    acc.ok_or(UnitError::EmptyInput)
}

fn parse_power(s: &str, registry: &Registry) -> UnitResult<Unit> {
    let s = s.trim();
    if let Some((base, exp_s)) = s.split_once("**") {
        let base_u = parse_atom(base.trim(), registry)?;
        let exp = parse_int_exp(exp_s.trim())?;
        return base_u.pow(exp);
    }
    if let Some((base, exp_s)) = s.split_once('^') {
        let base_u = parse_atom(base.trim(), registry)?;
        let exp = parse_int_exp(exp_s.trim())?;
        return base_u.pow(exp);
    }
    parse_atom(s, registry)
}

fn parse_int_exp(s: &str) -> UnitResult<i32> {
    if s.is_empty() {
        return Err(UnitError::InvalidExponent);
    }
    let v: i32 = s.parse().map_err(|_| UnitError::InvalidExponent)?;
    Ok(v)
}

fn parse_atom(s: &str, registry: &Registry) -> UnitResult<Unit> {
    if s.is_empty() {
        return Err(UnitError::EmptyInput);
    }
    if let Ok((num, rest)) = parse_number_prefix(s) {
        let rest = rest.trim();
        if rest.is_empty() {
            let mut u = Unit::dimensionless();
            u.scale = num;
            return Ok(u);
        }
        let mut u = parse_atom(rest, registry)?;
        if !u.affine.is_multiplicative() {
            return Err(UnitError::Parse(
                "numeric scale cannot be applied to affine units".into(),
            ));
        }
        u.scale *= num;
        return Ok(u);
    }
    if s == "1" {
        return Ok(Unit::dimensionless());
    }
    if let Some(rest) = s.strip_prefix('(') {
        let end = rest
            .rfind(')')
            .ok_or_else(|| UnitError::Parse("unclosed '(' in unit".into()))?;
        let inner = &rest[..end];
        return parse_product(inner, registry);
    }
    registry.lookup(s)
}

fn split_top_level(s: &str, sep: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            c if c == sep && depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_compound() {
        let reg = Registry::default();
        let u = parse_unit_expr("kg*m/s^2", &reg).unwrap();
        assert_eq!(u.dimension.m, 1);
        assert_eq!(u.dimension.l, 1);
        assert_eq!(u.dimension.t, -2);
    }

    #[test]
    fn parse_quantity_with_space() {
        let reg = Registry::default();
        let (n, u) = parse_quantity("5.5 m", &reg).unwrap();
        assert!((n - 5.5).abs() < 1e-12);
        assert_eq!(u.symbol, "m");
    }
}
