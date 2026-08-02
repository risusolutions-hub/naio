/// Maximum input/output size (64 MiB guard).
pub const MAX_BYTES: usize = 64 * 1024 * 1024;

/// MessagePack timestamp extension type (-1).
pub const TIMESTAMP_EXT: i8 = -1;

/// Options for packing Niao values to MessagePack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackOptions {
    /// Encode strings as bin type (Python `use_bin_type`, default `true`).
    pub use_bin_type: bool,
    /// Use 32-bit floats when values fit (Python `use_single_float`).
    pub use_single_float: bool,
    /// Encode `{sec, nsec}` / `{seconds, nanoseconds}` objects as timestamp ext.
    pub timestamp: bool,
    /// Serialize integers larger than 64 bits as decimal strings.
    pub bigint_as_string: bool,
}

impl Default for PackOptions {
    fn default() -> Self {
        Self {
            use_bin_type: false,
            use_single_float: false,
            timestamp: true,
            bigint_as_string: true,
        }
    }
}

/// Options for unpacking MessagePack into Niao values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnpackOptions {
    /// Require string keys in maps (Python `strict_map_key`, default `true`).
    pub strict_map_key: bool,
    /// Decode str format as raw bytes instead of UTF-8 strings (Python `raw`).
    pub raw: bool,
    /// Decode timestamp ext (-1) to `{sec, nsec}` objects.
    pub timestamp: bool,
    /// Parse decimal strings that look like big integers back to BigInt.
    pub bigint_as_string: bool,
    /// Maximum nesting depth while decoding (DoS guard).
    pub max_depth: usize,
}

impl Default for UnpackOptions {
    fn default() -> Self {
        Self {
            strict_map_key: true,
            raw: false,
            timestamp: true,
            bigint_as_string: true,
            max_depth: 512,
        }
    }
}
