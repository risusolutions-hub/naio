use crate::error::{ProtoError, ProtoResult};
use crate::schema::ProtoSchema;
use crate::value::{
    dynamic_to_json_value, dynamic_to_niao, json_value_to_dynamic, niao_to_dynamic,
};
use prost::Message;
use prost_reflect::{DynamicMessage, MessageDescriptor, ReflectMessage, Value};
use serde_json::Value as JsonValue;

/// A mutable protobuf message backed by `prost-reflect`.
#[derive(Clone)]
pub struct ProtoMessage {
    pub(crate) inner: DynamicMessage,
}

impl ProtoMessage {
    pub fn new(schema: &ProtoSchema, full_name: &str) -> ProtoResult<Self> {
        let desc = schema.message_descriptor(full_name)?;
        Ok(Self {
            inner: DynamicMessage::new(desc),
        })
    }

    pub fn decode(schema: &ProtoSchema, full_name: &str, data: &[u8]) -> ProtoResult<Self> {
        let desc = schema.message_descriptor(full_name)?;
        DynamicMessage::decode(desc, data)
            .map(|inner| Self { inner })
            .map_err(|e| ProtoError::Decode(e.to_string()))
    }

    pub fn encode(&self) -> ProtoResult<Vec<u8>> {
        let mut buf = Vec::with_capacity(self.inner.encoded_len());
        self.inner
            .encode(&mut buf)
            .map_err(|e| ProtoError::Encode(e.to_string()))?;
        Ok(buf)
    }

    pub fn merge_bytes(&mut self, data: &[u8]) -> ProtoResult<()> {
        self.inner
            .merge(data)
            .map_err(|e| ProtoError::Decode(e.to_string()))
    }

    pub fn merge(&mut self, other: &ProtoMessage) -> ProtoResult<()> {
        if self.descriptor().full_name() != other.descriptor().full_name() {
            return Err(ProtoError::Type(format!(
                "cannot merge {} into {}",
                other.descriptor().full_name(),
                self.descriptor().full_name()
            )));
        }
        for field in other.descriptor().fields() {
            if other.inner.has_field(&field) {
                let val = other.inner.get_field(&field).as_ref().clone();
                self.inner.set_field(&field, val);
            }
        }
        Ok(())
    }

    pub fn clear(&mut self) {
        self.inner.clear();
    }

    pub fn clone_msg(&self) -> ProtoResult<Self> {
        let bytes = self.encode()?;
        Ok(Self {
            inner: DynamicMessage::decode(self.descriptor(), bytes.as_slice())
                .map_err(|e| ProtoError::Decode(e.to_string()))?,
        })
    }

    pub fn descriptor(&self) -> MessageDescriptor {
        self.inner.descriptor().clone()
    }

    pub fn full_name(&self) -> String {
        self.descriptor().full_name().to_string()
    }

    pub fn has_field(&self, name: &str) -> ProtoResult<bool> {
        let field = self
            .descriptor()
            .get_field_by_name(name)
            .ok_or_else(|| ProtoError::Schema(format!("unknown field: {name}")))?;
        Ok(self.inner.has_field(&field))
    }

    pub fn get_field_json(&self, name: &str) -> ProtoResult<JsonValue> {
        let field = self
            .descriptor()
            .get_field_by_name(name)
            .ok_or_else(|| ProtoError::Schema(format!("unknown field: {name}")))?;
        if !self.inner.has_field(&field) {
            return Ok(JsonValue::Null);
        }
        let val = self.inner.get_field(&field);
        dynamic_to_json_value(val.as_ref())
    }

    pub fn set_field_json(&mut self, name: &str, value: &JsonValue) -> ProtoResult<()> {
        let field = self
            .descriptor()
            .get_field_by_name(name)
            .ok_or_else(|| ProtoError::Schema(format!("unknown field: {name}")))?;
        if value.is_null() {
            self.inner.clear_field(&field);
            return Ok(());
        }
        let val = json_value_to_dynamic(&field, value)?;
        self.inner.set_field(&field, val);
        Ok(())
    }

    pub fn set_dynamic(&mut self, name: &str, value: Value) -> ProtoResult<()> {
        let field = self
            .descriptor()
            .get_field_by_name(name)
            .ok_or_else(|| ProtoError::Schema(format!("unknown field: {name}")))?;
        self.inner.set_field(&field, value);
        Ok(())
    }

    pub fn get_dynamic(&self, name: &str) -> ProtoResult<Value> {
        let field = self
            .descriptor()
            .get_field_by_name(name)
            .ok_or_else(|| ProtoError::Schema(format!("unknown field: {name}")))?;
        if !self.inner.has_field(&field) {
            return Ok(default_for_field(&field));
        }
        Ok(self.inner.get_field(&field).as_ref().clone())
    }

    pub fn fields_set(&self) -> Vec<String> {
        self.descriptor()
            .fields()
            .filter(|f| self.inner.has_field(&f))
            .map(|f| f.name().to_string())
            .collect()
    }

    pub fn to_json(&self, pretty: bool) -> ProtoResult<String> {
        let opts = prost_reflect::SerializeOptions::new().skip_default_fields(false);
        let mut buf = Vec::new();
        let mut ser = serde_json::Serializer::new(&mut buf);
        self.inner
            .serialize_with_options(&mut ser, &opts)
            .map_err(|e| ProtoError::Json(e.to_string()))?;
        let text = String::from_utf8(buf).map_err(|e| ProtoError::Json(e.to_string()))?;
        if pretty {
            let val: JsonValue =
                serde_json::from_str(&text).map_err(|e| ProtoError::Json(e.to_string()))?;
            serde_json::to_string_pretty(&val).map_err(|e| ProtoError::Json(e.to_string()))
        } else {
            Ok(text)
        }
    }

    pub fn from_json(schema: &ProtoSchema, full_name: &str, text: &str) -> ProtoResult<Self> {
        let desc = schema.message_descriptor(full_name)?;
        let opts = prost_reflect::DeserializeOptions::new();
        let mut de = serde_json::Deserializer::from_str(text);
        let inner = DynamicMessage::deserialize_with_options(desc, &mut de, &opts)
            .map_err(|e| ProtoError::Json(e.to_string()))?;
        de.end().map_err(|e| ProtoError::Json(e.to_string()))?;
        Ok(Self { inner })
    }

    pub fn apply_niao_fields(
        &mut self,
        fields: &std::collections::HashMap<String, crate::value::NiaoFieldValue>,
    ) -> ProtoResult<()> {
        for (name, val) in fields {
            let field = self
                .descriptor()
                .get_field_by_name(name)
                .ok_or_else(|| ProtoError::Schema(format!("unknown field: {name}")))?;
            let dynamic = niao_to_dynamic(&field, val)?;
            self.inner.set_field(&field, dynamic);
        }
        Ok(())
    }

    pub fn to_niao_map(
        &self,
    ) -> ProtoResult<std::collections::HashMap<String, crate::value::NiaoFieldValue>> {
        let mut out = std::collections::HashMap::new();
        for field in self.descriptor().fields() {
            if self.inner.has_field(&field) {
                let val = self.inner.get_field(&field);
                out.insert(field.name().to_string(), dynamic_to_niao(val.as_ref())?);
            }
        }
        Ok(out)
    }
}

fn default_for_field(field: &prost_reflect::FieldDescriptor) -> Value {
    match field.cardinality() {
        prost_reflect::Cardinality::Repeated => Value::List(Vec::new()),
        _ => Value::Bool(false),
    }
}
