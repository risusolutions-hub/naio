use super::error::Error;
use std::fmt;

pub trait ToSql: fmt::Debug + Send + Sync {
    fn to_sql_opt(&self) -> Result<Option<String>, Error>;
}

pub trait FromSql<'a>: Sized {
    fn from_sql_nullable(raw: Option<&'a str>) -> Result<Self, Error>;
}

macro_rules! impl_scalar {
    ($ty:ty) => {
        impl ToSql for $ty {
            fn to_sql_opt(&self) -> Result<Option<String>, Error> {
                Ok(Some(self.to_string()))
            }
        }
    };
}

impl_scalar!(i32);
impl_scalar!(i64);
impl_scalar!(f64);

impl ToSql for bool {
    fn to_sql_opt(&self) -> Result<Option<String>, Error> {
        Ok(Some(if *self { "t".into() } else { "f".into() }))
    }
}

impl ToSql for String {
    fn to_sql_opt(&self) -> Result<Option<String>, Error> {
        Ok(Some(self.clone()))
    }
}

impl ToSql for &str {
    fn to_sql_opt(&self) -> Result<Option<String>, Error> {
        Ok(Some((*self).to_string()))
    }
}

impl ToSql for Vec<u8> {
    fn to_sql_opt(&self) -> Result<Option<String>, Error> {
        Ok(Some(format!("\\\\x{}", niao_codec::hex::encode(self))))
    }
}

impl ToSql for niao_json_core::Value {
    fn to_sql_opt(&self) -> Result<Option<String>, Error> {
        Ok(Some(niao_json_core::to_string(self)))
    }
}

impl ToSql for Vec<String> {
    fn to_sql_opt(&self) -> Result<Option<String>, Error> {
        let inner: Vec<String> = self
            .iter()
            .map(|s| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")))
            .collect();
        Ok(Some(format!("{{{}}}", inner.join(","))))
    }
}

impl<T: ToSql> ToSql for Option<T> {
    fn to_sql_opt(&self) -> Result<Option<String>, Error> {
        match self {
            Some(v) => v.to_sql_opt(),
            None => Ok(None),
        }
    }
}

macro_rules! impl_from {
    ($ty:ty, $parse:expr) => {
        impl<'a> FromSql<'a> for $ty {
            fn from_sql_nullable(raw: Option<&'a str>) -> Result<Self, Error> {
                match raw {
                    Some(s) => $parse(s),
                    None => Err(Error::msg("unexpected NULL")),
                }
            }
        }
        impl<'a> FromSql<'a> for Option<$ty> {
            fn from_sql_nullable(raw: Option<&'a str>) -> Result<Self, Error> {
                match raw {
                    Some(s) => Ok(Some($parse(s)?)),
                    None => Ok(None),
                }
            }
        }
    };
}

impl_from!(i32, |s: &str| s
    .parse()
    .map_err(|e: std::num::ParseIntError| Error::msg(e.to_string())));
impl_from!(i64, |s: &str| s
    .parse()
    .map_err(|e: std::num::ParseIntError| Error::msg(e.to_string())));
impl_from!(f64, |s: &str| s
    .parse()
    .map_err(|e: std::num::ParseFloatError| Error::msg(e.to_string())));
impl_from!(bool, |s: &str| -> Result<bool, Error> {
    Ok(s == "t" || s.eq_ignore_ascii_case("true"))
});
impl_from!(String, |s: &str| -> Result<String, Error> {
    Ok(s.to_string())
});

impl<'a> FromSql<'a> for &'a str {
    fn from_sql_nullable(raw: Option<&'a str>) -> Result<Self, Error> {
        raw.ok_or_else(|| Error::msg("unexpected NULL"))
    }
}

impl<'a> FromSql<'a> for Vec<u8> {
    fn from_sql_nullable(raw: Option<&'a str>) -> Result<Self, Error> {
        let s = raw.ok_or_else(|| Error::msg("unexpected NULL"))?;
        if let Some(hex) = s.strip_prefix("\\x") {
            niao_codec::hex::decode(hex).map_err(|e| Error::msg(e.to_string()))
        } else {
            Ok(s.as_bytes().to_vec())
        }
    }
}

impl<'a> FromSql<'a> for Option<Vec<u8>> {
    fn from_sql_nullable(raw: Option<&'a str>) -> Result<Self, Error> {
        match raw {
            Some(s) => Ok(Some(Vec::<u8>::from_sql_nullable(Some(s))?)),
            None => Ok(None),
        }
    }
}

impl<'a> FromSql<'a> for niao_json_core::Value {
    fn from_sql_nullable(raw: Option<&'a str>) -> Result<Self, Error> {
        let s = raw.ok_or_else(|| Error::msg("unexpected NULL"))?;
        niao_json_core::parse(s).map_err(|e| Error::msg(e.to_string()))
    }
}

impl<'a> FromSql<'a> for Option<niao_json_core::Value> {
    fn from_sql_nullable(raw: Option<&'a str>) -> Result<Self, Error> {
        match raw {
            Some(s) => Ok(Some(niao_json_core::Value::from_sql_nullable(Some(s))?)),
            None => Ok(None),
        }
    }
}

impl<'a> FromSql<'a> for Vec<String> {
    fn from_sql_nullable(raw: Option<&'a str>) -> Result<Self, Error> {
        let s = raw.ok_or_else(|| Error::msg("unexpected NULL"))?;
        if !s.starts_with('{') || !s.ends_with('}') {
            return Err(Error::msg("expected postgres array text"));
        }
        let inner = &s[1..s.len() - 1];
        if inner.is_empty() {
            return Ok(Vec::new());
        }
        Ok(inner
            .split(',')
            .map(|x| x.trim_matches('"').to_string())
            .collect())
    }
}

impl<'a> FromSql<'a> for Option<Vec<String>> {
    fn from_sql_nullable(raw: Option<&'a str>) -> Result<Self, Error> {
        match raw {
            Some(s) => Ok(Some(Vec::<String>::from_sql_nullable(Some(s))?)),
            None => Ok(None),
        }
    }
}
