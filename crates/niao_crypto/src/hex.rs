//! Hex helpers for tests and runtime.

pub fn encode(data: &[u8]) -> String {
    niao_codec::hex::encode(data)
}

pub fn decode(s: &str) -> Result<Vec<u8>, String> {
    niao_codec::hex::decode(s).map_err(|e| e.to_string())
}
