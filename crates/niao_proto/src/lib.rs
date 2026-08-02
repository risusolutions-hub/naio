//! Protocol Buffers codec + codegen for Niao (`nproto`).
//!
//! Dynamic encode/decode via [`prost-reflect`], `.proto` compilation via [`protox`],
//! canonical JSON mapping, wire introspection, and Niao source codegen.

mod codegen;
mod compile;
mod error;
mod message;
mod schema;
mod value;
mod wire;

pub use codegen::{codegen, CodegenOptions};
pub use compile::{compile_files, compile_source, load_descriptor_set};
pub use error::{ProtoError, ProtoResult};
pub use message::ProtoMessage;
pub use schema::{FieldInfo, MessageInfo, OneofInfo, ProtoSchema};
pub use value::{dynamic_to_niao, niao_to_dynamic, NiaoFieldValue, ProtoMessageRef};
pub use wire::{decode_raw, decode_varint, encode_tag, encode_varint, RawField, RawValue};

/// Maximum encoded message / descriptor blob size (64 MiB guard).
pub const MAX_BYTES: usize = 64 * 1024 * 1024;

/// Validate that bytes look like a `FileDescriptorSet`.
pub fn valid_descriptor_set(bytes: &[u8]) -> bool {
    use prost::Message;
    prost_types::FileDescriptorSet::decode(bytes).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn end_to_end() {
        let src = r#"
syntax = "proto3";
package test;
message Echo { string text = 1; int32 n = 2; }
"#;
        let schema = compile_source("echo.proto", src, &[]).unwrap();
        let mut msg = ProtoMessage::new(&schema, "test.Echo").unwrap();
        msg.set_field_json("text", &serde_json::json!("hi"))
            .unwrap();
        msg.set_field_json("n", &serde_json::json!(7)).unwrap();
        let bytes = msg.encode().unwrap();
        let raw = decode_raw(&bytes).unwrap();
        assert_eq!(raw.len(), 2);
        let decoded = ProtoMessage::decode(&schema, "test.Echo", &bytes).unwrap();
        assert_eq!(
            decoded.get_field_json("text").unwrap(),
            serde_json::json!("hi")
        );
    }
}
