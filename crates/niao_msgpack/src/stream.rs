use crate::error::MsgpackError;
use crate::options::{PackOptions, UnpackOptions};
use crate::pack::msg_to_rmpv;
use crate::unpack::rmpv_to_msg;
use crate::value::MsgValue;
use crate::MAX_BYTES;
use std::io::Cursor;

/// Incremental MessagePack encoder (Python `Packer`).
#[derive(Debug, Clone)]
pub struct Packer {
    buf: Vec<u8>,
    opts: PackOptions,
}

impl Packer {
    pub fn new(opts: PackOptions) -> Self {
        Self {
            buf: Vec::with_capacity(4096),
            opts,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(PackOptions::default())
    }

    pub fn options(&self) -> &PackOptions {
        &self.opts
    }

    pub fn bytes(&self) -> &[u8] {
        &self.buf
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn reset(&mut self) {
        self.buf.clear();
    }

    pub fn pack(&mut self, value: &MsgValue) -> Result<(), MsgpackError> {
        let rmpv = msg_to_rmpv(value, &self.opts, 0)?;
        rmpv::encode::write_value(&mut self.buf, &rmpv)?;
        if self.buf.len() > MAX_BYTES {
            return Err(MsgpackError::TooLarge(self.buf.len()));
        }
        Ok(())
    }

    pub fn finish(mut self) -> Vec<u8> {
        std::mem::take(&mut self.buf)
    }
}

/// Incremental MessagePack decoder (Python `Unpacker`).
#[derive(Debug, Clone)]
pub struct Unpacker {
    data: Vec<u8>,
    pos: usize,
    opts: UnpackOptions,
}

impl Unpacker {
    pub fn new(opts: UnpackOptions) -> Self {
        Self {
            data: Vec::new(),
            pos: 0,
            opts,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(UnpackOptions::default())
    }

    pub fn from_bytes(data: Vec<u8>, opts: UnpackOptions) -> Self {
        Self { data, pos: 0, opts }
    }

    pub fn options(&self) -> &UnpackOptions {
        &self.opts
    }

    pub fn tell(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn buffered(&self) -> usize {
        self.data.len()
    }

    pub fn reset(&mut self) {
        self.data.clear();
        self.pos = 0;
    }

    pub fn feed(&mut self, chunk: &[u8]) -> Result<(), MsgpackError> {
        if self.data.len() + chunk.len() > MAX_BYTES {
            return Err(MsgpackError::TooLarge(self.data.len() + chunk.len()));
        }
        self.data.extend_from_slice(chunk);
        Ok(())
    }

    /// Decode the next value. Returns `Ok(None)` when more input is needed.
    pub fn next(&mut self) -> Result<Option<MsgValue>, MsgpackError> {
        if self.pos >= self.data.len() {
            return Ok(None);
        }
        let slice = &self.data[self.pos..];
        let mut cursor = Cursor::new(slice);
        let value = match rmpv::decode::read_value(&mut cursor) {
            Ok(v) => v,
            Err(e) => {
                let incomplete = match &e {
                    rmpv::decode::Error::InvalidDataRead(io)
                        if io.kind() == std::io::ErrorKind::UnexpectedEof =>
                    {
                        true
                    }
                    rmpv::decode::Error::InvalidMarkerRead(io)
                        if io.kind() == std::io::ErrorKind::UnexpectedEof =>
                    {
                        true
                    }
                    other if other.to_string().contains("failed to fill whole buffer") => true,
                    _ => false,
                };
                if incomplete {
                    return Ok(None);
                }
                return Err(e.into());
            }
        };
        let consumed = cursor.position() as usize;
        self.pos += consumed;
        Ok(Some(rmpv_to_msg(value, &self.opts, 0)?))
    }

    /// Drain and decode every complete value currently buffered.
    pub fn read_all(&mut self) -> Result<Vec<MsgValue>, MsgpackError> {
        let mut out = Vec::new();
        while let Some(v) = self.next()? {
            out.push(v);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::pack;
    use crate::unpack::unpack_all;
    use crate::value::MsgValue;

    #[test]
    fn packer_stream() {
        let mut p = Packer::with_defaults();
        p.pack(&MsgValue::Int(1)).unwrap();
        p.pack(&MsgValue::Int(2)).unwrap();
        let bytes = p.finish();
        let values = unpack_all(&bytes, &UnpackOptions::default()).unwrap();
        assert_eq!(values.len(), 2);
    }

    #[test]
    fn unpacker_incremental() {
        let bytes = pack(
            &MsgValue::Array(vec![MsgValue::Int(1), MsgValue::Int(2)]),
            &PackOptions::default(),
        )
        .unwrap();
        let mut u = Unpacker::with_defaults();
        u.feed(&bytes[..2]).unwrap();
        assert!(u.next().unwrap().is_none());
        u.feed(&bytes[2..]).unwrap();
        let v = u.next().unwrap().expect("value");
        match v {
            MsgValue::Array(items) => assert_eq!(items.len(), 2),
            other => panic!("{other:?}"),
        }
    }
}
