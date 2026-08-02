use crate::error::{ProtoError, ProtoResult};
use prost_reflect::{EnumDescriptor, FieldDescriptor, Kind, MessageDescriptor, OneofDescriptor};
use prost_types::FileDescriptorSet;

/// Compiled protobuf schema (descriptor pool + raw file descriptor set).
#[derive(Clone)]
pub struct ProtoSchema {
    pub(crate) pool: prost_reflect::DescriptorPool,
    pub(crate) fds: FileDescriptorSet,
}

impl ProtoSchema {
    /// Full protobuf name of every message type in the schema.
    pub fn message_names(&self) -> Vec<String> {
        self.pool
            .all_messages()
            .map(|d| d.full_name().to_string())
            .collect()
    }

    /// Full protobuf name of every enum type in the schema.
    pub fn enum_names(&self) -> Vec<String> {
        self.pool
            .all_enums()
            .map(|d| d.full_name().to_string())
            .collect()
    }

    /// Service RPC names (`package.Service/Method`).
    pub fn service_methods(&self) -> Vec<String> {
        let mut out = Vec::new();
        for svc in self.pool.services() {
            let svc_name = svc.full_name();
            for method in svc.methods() {
                out.push(format!("{svc_name}/{}", method.name()));
            }
        }
        out
    }

    pub fn message_descriptor(&self, full_name: &str) -> ProtoResult<MessageDescriptor> {
        self.pool
            .get_message_by_name(full_name)
            .ok_or_else(|| ProtoError::Schema(format!("unknown message: {full_name}")))
    }

    pub fn enum_descriptor(&self, full_name: &str) -> ProtoResult<EnumDescriptor> {
        self.pool
            .get_enum_by_name(full_name)
            .ok_or_else(|| ProtoError::Schema(format!("unknown enum: {full_name}")))
    }

    /// Structured metadata for a message type (fields, oneofs, nested types).
    pub fn describe_message(&self, full_name: &str) -> ProtoResult<MessageInfo> {
        let desc = self.message_descriptor(full_name)?;
        Ok(MessageInfo::from_descriptor(&desc))
    }

    /// Export the underlying `FileDescriptorSet` as encoded bytes.
    pub fn encode_descriptor_set(&self) -> ProtoResult<Vec<u8>> {
        use prost::Message;
        let mut buf = Vec::new();
        prost::Message::encode(&self.fds, &mut buf)
            .map_err(|e| ProtoError::Encode(e.to_string()))?;
        Ok(buf)
    }
}

#[derive(Debug, Clone)]
pub struct MessageInfo {
    pub name: String,
    pub full_name: String,
    pub fields: Vec<FieldInfo>,
    pub oneofs: Vec<OneofInfo>,
}

#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub name: String,
    pub number: i32,
    pub kind: String,
    pub label: String,
    pub json_name: String,
    pub message_type: Option<String>,
    pub enum_type: Option<String>,
    pub map_key_type: Option<String>,
    pub map_value_type: Option<String>,
    pub default_value: Option<String>,
    pub oneof: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OneofInfo {
    pub name: String,
    pub fields: Vec<String>,
}

impl MessageInfo {
    fn from_descriptor(desc: &MessageDescriptor) -> Self {
        let fields = desc
            .fields()
            .map(|f| FieldInfo::from_descriptor(&f))
            .collect();
        let oneofs = desc
            .oneofs()
            .map(|o| OneofInfo::from_descriptor(&o))
            .collect();
        Self {
            name: desc.name().to_string(),
            full_name: desc.full_name().to_string(),
            fields,
            oneofs,
        }
    }
}

impl FieldInfo {
    fn from_descriptor(field: &FieldDescriptor) -> Self {
        let kind = kind_name(field.kind());
        let label = match field.cardinality() {
            prost_reflect::Cardinality::Optional => "optional",
            prost_reflect::Cardinality::Required => "required",
            prost_reflect::Cardinality::Repeated => "repeated",
        }
        .to_string();
        let (map_key_type, map_value_type) = if field.is_map() {
            let kind = field.kind();
            let entry = kind
                .as_message()
                .expect("map field should reference map entry message");
            let key = entry
                .get_field_by_name("key")
                .map(|f| kind_name(f.kind()))
                .unwrap_or_else(|| "unknown".into());
            let val = entry
                .get_field_by_name("value")
                .map(|f| kind_name(f.kind()))
                .unwrap_or_else(|| "unknown".into());
            (Some(key), Some(val))
        } else {
            (None, None)
        };
        Self {
            name: field.name().to_string(),
            number: field.number() as i32,
            kind,
            label,
            json_name: field.json_name().to_string(),
            message_type: field.kind().as_message().map(|m| m.full_name().to_string()),
            enum_type: field.kind().as_enum().map(|e| e.full_name().to_string()),
            map_key_type,
            map_value_type,
            oneof: field.containing_oneof().map(|o| o.name().to_string()),
            default_value: None,
        }
    }
}

impl OneofInfo {
    fn from_descriptor(oneof: &OneofDescriptor) -> Self {
        Self {
            name: oneof.name().to_string(),
            fields: oneof.fields().map(|f| f.name().to_string()).collect(),
        }
    }
}

fn kind_name(kind: Kind) -> String {
    match kind {
        Kind::Double => "double".into(),
        Kind::Float => "float".into(),
        Kind::Int64 => "int64".into(),
        Kind::Uint64 => "uint64".into(),
        Kind::Int32 => "int32".into(),
        Kind::Fixed64 => "fixed64".into(),
        Kind::Fixed32 => "fixed32".into(),
        Kind::Bool => "bool".into(),
        Kind::String => "string".into(),
        Kind::Bytes => "bytes".into(),
        Kind::Uint32 => "uint32".into(),
        Kind::Sfixed32 => "sfixed32".into(),
        Kind::Sfixed64 => "sfixed64".into(),
        Kind::Sint32 => "sint32".into(),
        Kind::Sint64 => "sint64".into(),
        Kind::Enum(e) => format!("enum({})", e.full_name()),
        Kind::Message(m) => format!("message({})", m.full_name()),
    }
}
