//! Zero-dependency encoding utilities: base64, hex, UUID, dotenv.

pub mod base64;
pub mod dotenv;
pub mod hex;
pub mod uuid;

pub use base64::{Alphabet, Base64Config, DecodeError, EncodeError};
pub use dotenv::{load_dotenv, parse_dotenv, parse_dotenv_reader};
pub use hex::{decode as hex_decode, encode as hex_encode, HexError};
pub use uuid::{Uuid, UuidError};
