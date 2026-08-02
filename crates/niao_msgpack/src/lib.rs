//! MessagePack encode/decode and streaming for Niao (`nmsgpack`).
//!
//! Backed by [`rmp`] / [`rmpv`] (msgpack-rust). Supports bin/str types,
//! extension types, timestamps, streaming pack/unpack, and strict map keys.

mod error;
mod options;
mod pack;
mod stream;
mod unpack;
mod value;

pub use error::MsgpackError;
pub use options::{PackOptions, UnpackOptions, MAX_BYTES, TIMESTAMP_EXT};
pub use pack::{pack, pack_all, pack_ext, pack_timestamp};
pub use stream::{Packer, Unpacker};
pub use unpack::{is_valid, unpack, unpack_all};
pub use value::{ext_object, timestamp_object, MsgValue};

/// Pack with default options.
pub fn pack_default(value: &MsgValue) -> Result<Vec<u8>, MsgpackError> {
    pack(value, &PackOptions::default())
}

/// Unpack with default options.
pub fn unpack_default(data: &[u8]) -> Result<MsgValue, MsgpackError> {
    unpack(data, &UnpackOptions::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_object() {
        let value = MsgValue::Map(vec![
            (
                MsgValue::String("name".into()),
                MsgValue::String("neko".into()),
            ),
            (
                MsgValue::String("tags".into()),
                MsgValue::Array(vec![
                    MsgValue::String("fast".into()),
                    MsgValue::String("binary".into()),
                ]),
            ),
            (MsgValue::String("n".into()), MsgValue::Int(42)),
        ]);
        let bytes = pack_default(&value).unwrap();
        let out = unpack_default(&bytes).unwrap();
        assert_eq!(value, out);
    }

    #[test]
    fn ext_roundtrip() {
        let ext = MsgValue::Ext {
            code: 42,
            data: vec![1, 2, 3],
        };
        let bytes = pack_default(&ext).unwrap();
        let out = unpack_default(&bytes).unwrap();
        match out {
            MsgValue::Map(pairs) => {
                let code = pairs
                    .iter()
                    .find(|(k, _)| matches!(k, MsgValue::String(s) if s == "code"));
                assert!(code.is_some());
            }
            other => panic!("{other:?}"),
        }
    }
}
