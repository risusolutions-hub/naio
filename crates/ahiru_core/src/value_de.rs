//! Deserialize serde types from `niao_json_core::Value`.

use niao_json_core::Value;
use serde::de::{self, DeserializeOwned, MapAccess, SeqAccess, Visitor};

pub fn from_value<T: DeserializeOwned>(value: &Value) -> Result<T, String> {
    T::deserialize(ValueDeserializer { value }).map_err(|e| e.to_string())
}

struct ValueDeserializer<'a> {
    value: &'a Value,
}

impl<'de> de::Deserializer<'de> for ValueDeserializer<'_> {
    type Error = de::value::Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.value {
            Value::Null => visitor.visit_none(),
            Value::Bool(b) => visitor.visit_bool(*b),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    visitor.visit_i64(i)
                } else if let Some(u) = n.as_u64() {
                    visitor.visit_u64(u)
                } else {
                    visitor.visit_f64(n.as_f64().unwrap_or(0.0))
                }
            }
            Value::String(s) => visitor.visit_string(s.clone()),
            Value::Array(items) => {
                let mut de = SeqDe { iter: items.iter() };
                visitor.visit_seq(&mut de)
            }
            Value::Object(map) => {
                let entries: Vec<_> = map.iter().collect();
                let mut de = MapDe { entries, idx: 0 };
                visitor.visit_map(&mut de)
            }
        }
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

struct SeqDe<'a> {
    iter: std::slice::Iter<'a, Value>,
}

impl<'de> SeqAccess<'de> for SeqDe<'_> {
    type Error = de::value::Error;

    fn next_element_seed<T: de::DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Self::Error> {
        match self.iter.next() {
            Some(v) => seed.deserialize(ValueDeserializer { value: v }).map(Some),
            None => Ok(None),
        }
    }
}

struct MapDe<'a> {
    entries: Vec<(&'a str, &'a Value)>,
    idx: usize,
}

impl<'de> MapAccess<'de> for MapDe<'_> {
    type Error = de::value::Error;

    fn next_key_seed<K: de::DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Self::Error> {
        if self.idx >= self.entries.len() {
            return Ok(None);
        }
        let (k, _) = self.entries[self.idx];
        seed.deserialize(KeyDe { key: k }).map(Some)
    }

    fn next_value_seed<V: de::DeserializeSeed<'de>>(
        &mut self,
        seed: V,
    ) -> Result<V::Value, Self::Error> {
        let (_, v) = self.entries[self.idx];
        self.idx += 1;
        seed.deserialize(ValueDeserializer { value: v })
    }
}

struct KeyDe<'a> {
    key: &'a str,
}

impl<'de> de::Deserializer<'de> for KeyDe<'_> {
    type Error = de::value::Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        visitor.visit_str(self.key)
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        visitor.visit_str(self.key)
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        visitor.visit_string(self.key.to_string())
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        visitor.visit_str(self.key)
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char bytes byte_buf option unit
        unit_struct newtype_struct seq tuple tuple_struct map struct enum ignored_any
    }
}
