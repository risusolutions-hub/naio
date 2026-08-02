//! Bridge between `serde` and `niao_json_core::Value`.

use crate::object::Object;
use crate::value::Value;
use crate::Number;
use serde::de::{self, DeserializeOwned, MapAccess, SeqAccess, Visitor};
use serde::ser::{Error as SerTrait, Serialize, Serializer};
use serde::Serialize as _;
use std::fmt;

pub fn parse_json<T: DeserializeOwned>(s: &str) -> Result<T, String> {
    let value = crate::parse(s).map_err(|e| e.to_string())?;
    from_value(&value)
}

pub fn deserialize_to_value<'de, D: de::Deserializer<'de>>(
    deserializer: D,
) -> Result<Value, D::Error> {
    deserializer.deserialize_any(ValueSeed)
}

struct ValueSeed;

impl<'de> de::Visitor<'de> for ValueSeed {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("any JSON value")
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
        Ok(Value::Bool(v))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
        Ok(Value::Number(Number::I64(v)))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
        Ok(Value::Number(Number::U64(v)))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
        Ok(Value::Number(Number::F64(v)))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        Ok(Value::String(v.to_string()))
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
        Ok(Value::String(v))
    }

    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut items = Vec::new();
        while let Some(v) = seq.next_element_seed(ValueSeed)? {
            items.push(v);
        }
        Ok(Value::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut object = Object::new();
        while let Some(key) = map.next_key::<String>()? {
            let value = map.next_value_seed(ValueSeed)?;
            object.insert(key, value);
        }
        Ok(Value::Object(object))
    }
}

impl<'de> de::DeserializeSeed<'de> for ValueSeed {
    type Value = Value;

    fn deserialize<D: de::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_any(ValueSeed)
    }
}

pub fn from_value<T: DeserializeOwned>(value: &Value) -> Result<T, String> {
    T::deserialize(ValueDeserializer { value }).map_err(|e| e.to_string())
}

pub fn to_value<T: Serialize + ?Sized>(value: &T) -> Result<Value, String> {
    value.serialize(ValueSerializer).map_err(|e| e.0)
}

#[derive(Debug)]
struct SerError(String);

impl fmt::Display for SerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for SerError {}

impl serde::ser::Error for SerError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        SerError(msg.to_string())
    }
}

pub fn to_string_pretty_value<T: Serialize>(value: &T) -> Result<String, String> {
    let v = to_value(value)?;
    Ok(crate::write::to_string_pretty(&v, 2))
}

pub fn to_vec_value<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let v = to_value(value)?;
    Ok(crate::write::to_vec(&v))
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

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.value {
            Value::Null => visitor.visit_none(),
            _ => visitor.visit_some(ValueDeserializer { value: self.value }),
        }
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string
        bytes byte_buf unit unit_struct newtype_struct seq tuple
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

struct ValueSerializer;

impl Serializer for ValueSerializer {
    type Ok = Value;
    type Error = SerError;
    type SerializeSeq = SerializeSeq;
    type SerializeTuple = SerializeSeq;
    type SerializeTupleStruct = SerializeSeq;
    type SerializeTupleVariant = SerializeTupleVariant;
    type SerializeMap = SerializeMap;
    type SerializeStruct = SerializeMap;
    type SerializeStructVariant = SerializeStructVariant;

    fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Bool(v))
    }

    fn serialize_i8(self, v: i8) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Number(Number::I64(v as i64)))
    }

    fn serialize_i16(self, v: i16) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Number(Number::I64(v as i64)))
    }

    fn serialize_i32(self, v: i32) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Number(Number::I64(v as i64)))
    }

    fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Number(Number::I64(v)))
    }

    fn serialize_u8(self, v: u8) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Number(Number::U64(v as u64)))
    }

    fn serialize_u16(self, v: u16) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Number(Number::U64(v as u64)))
    }

    fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Number(Number::U64(v as u64)))
    }

    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Number(Number::U64(v)))
    }

    fn serialize_f32(self, v: f32) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Number(Number::F64(v as f64)))
    }

    fn serialize_f64(self, v: f64) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Number(Number::F64(v)))
    }

    fn serialize_char(self, v: char) -> Result<Self::Ok, Self::Error> {
        Ok(Value::String(v.to_string()))
    }

    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
        Ok(Value::String(v.to_string()))
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
        let items = v
            .iter()
            .map(|b| Value::Number(Number::U64(*b as u64)))
            .collect();
        Ok(Value::Array(items))
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Null)
    }

    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<Self::Ok, Self::Error> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Null)
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Null)
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        Ok(Value::String(variant.to_string()))
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        let mut map = Object::new();
        map.insert(
            variant.to_string(),
            to_value(value).map_err(SerTrait::custom)?,
        );
        Ok(Value::Object(map))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Ok(SerializeSeq {
            items: Vec::with_capacity(len.unwrap_or(0)),
        })
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Ok(SerializeTupleVariant {
            variant: variant.to_string(),
            items: Vec::with_capacity(len),
        })
    }

    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(SerializeMap {
            map: Object::new(),
            next_key: None,
            len: len.unwrap_or(0),
        })
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        self.serialize_map(Some(len))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        Ok(SerializeStructVariant {
            variant: variant.to_string(),
            map: Object::new(),
            next_key: None,
            len,
        })
    }
}

struct SerializeSeq {
    items: Vec<Value>,
}

impl serde::ser::SerializeSeq for SerializeSeq {
    type Ok = Value;
    type Error = SerError;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        self.items.push(to_value(value).map_err(SerTrait::custom)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Array(self.items))
    }
}

impl serde::ser::SerializeTuple for SerializeSeq {
    type Ok = Value;
    type Error = SerError;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        self.items.push(to_value(value).map_err(SerTrait::custom)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Array(self.items))
    }
}

impl serde::ser::SerializeTupleStruct for SerializeSeq {
    type Ok = Value;
    type Error = SerError;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        self.items.push(to_value(value).map_err(SerTrait::custom)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Array(self.items))
    }
}

struct SerializeTupleVariant {
    variant: String,
    items: Vec<Value>,
}

impl serde::ser::SerializeTupleVariant for SerializeTupleVariant {
    type Ok = Value;
    type Error = SerError;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        self.items.push(to_value(value).map_err(SerTrait::custom)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        let mut map = Object::new();
        map.insert(self.variant, Value::Array(self.items));
        Ok(Value::Object(map))
    }
}

struct SerializeMap {
    map: Object,
    next_key: Option<String>,
    len: usize,
}

impl serde::ser::SerializeMap for SerializeMap {
    type Ok = Value;
    type Error = SerError;

    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), Self::Error> {
        let key = to_value(key).map_err(SerTrait::custom)?;
        self.next_key = Some(
            key.as_str()
                .map(str::to_string)
                .ok_or_else(|| SerError("JSON map keys must be strings".to_string()))?,
        );
        Ok(())
    }

    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        let key = self
            .next_key
            .take()
            .ok_or_else(|| SerError("serialize_value without key".to_string()))?;
        self.map
            .insert(key, to_value(value).map_err(SerTrait::custom)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Object(self.map))
    }
}

impl serde::ser::SerializeStruct for SerializeMap {
    type Ok = Value;
    type Error = SerError;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error> {
        self.map
            .insert(key.to_string(), to_value(value).map_err(SerTrait::custom)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Object(self.map))
    }
}

struct SerializeStructVariant {
    variant: String,
    map: Object,
    next_key: Option<String>,
    len: usize,
}

impl serde::ser::SerializeStructVariant for SerializeStructVariant {
    type Ok = Value;
    type Error = SerError;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error> {
        self.map
            .insert(key.to_string(), to_value(value).map_err(SerTrait::custom)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        let mut outer = Object::new();
        outer.insert(self.variant, Value::Object(self.map));
        Ok(Value::Object(outer))
    }
}
