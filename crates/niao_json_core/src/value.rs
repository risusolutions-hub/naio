use crate::object::Object;
use crate::Number;
use std::borrow::Cow;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<Value>),
    Object(Object),
}

impl Value {
    #[inline]
    pub fn null() -> Self {
        Self::Null
    }

    #[inline]
    pub fn bool(v: bool) -> Self {
        Self::Bool(v)
    }

    #[inline]
    pub fn int(v: i64) -> Self {
        Self::Number(Number::I64(v))
    }

    #[inline]
    pub fn float(v: f64) -> Self {
        Self::Number(Number::F64(v))
    }

    #[inline]
    pub fn string(s: impl Into<String>) -> Self {
        Self::String(s.into())
    }

    #[inline]
    pub fn array(items: Vec<Value>) -> Self {
        Self::Array(items)
    }

    #[inline]
    pub fn object(map: Object) -> Self {
        Self::Object(map)
    }

    #[inline]
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    #[inline]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    #[inline]
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Number(n) => n.as_i64(),
            _ => None,
        }
    }

    #[inline]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(n) => n.as_f64(),
            _ => None,
        }
    }

    #[inline]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    #[inline]
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Self::Array(a) => Some(a),
            _ => None,
        }
    }

    #[inline]
    pub fn as_object(&self) -> Option<&Object> {
        match self {
            Self::Object(o) => Some(o),
            _ => None,
        }
    }

    #[inline]
    pub fn as_object_mut(&mut self) -> Option<&mut Object> {
        match self {
            Self::Object(o) => Some(o),
            _ => None,
        }
    }

    #[inline]
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.as_object()?.get(key)
    }
}

/// Borrowed string slice when JSON had no escapes (zero-copy parse path).
pub type StrSlice<'a> = Cow<'a, str>;

impl<'a> From<&'a str> for Value {
    fn from(s: &'a str) -> Self {
        Self::String(s.to_string())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}

impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Self::Number(Number::I64(n))
    }
}

impl From<i32> for Value {
    fn from(n: i32) -> Self {
        Self::Number(Number::I64(n as i64))
    }
}

impl From<u64> for Value {
    fn from(n: u64) -> Self {
        Self::Number(Number::U64(n))
    }
}

impl From<f64> for Value {
    fn from(n: f64) -> Self {
        Self::Number(Number::F64(n))
    }
}
