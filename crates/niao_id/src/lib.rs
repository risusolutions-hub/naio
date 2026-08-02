//! `niao_id` — high-performance ID generation for Niao (`nid` stdlib).

mod entropy;
pub mod hashids;
pub mod nanoid;
pub mod snowflake;
pub mod ulid;
pub mod uuid_ext;

pub use hashids::{Hashids, HashidsError, DEFAULT_ALPHABET as HASHIDS_DEFAULT_ALPHABET};
pub use nanoid::{
    nanoid, nanoid_bulk, nanoid_fast, nanoid_size, nanoid_with, NanoidError,
    DEFAULT_ALPHABET as NANOID_DEFAULT_ALPHABET, DEFAULT_SIZE as NANOID_DEFAULT_SIZE,
};
pub use niao_codec::UuidError;
pub use snowflake::{
    parse as snowflake_parse, SnowflakeError, SnowflakeGenerator, SnowflakeParts,
    DEFAULT_EPOCH_MS as SNOWFLAKE_DEFAULT_EPOCH, MAX_DATACENTER_ID, MAX_WORKER_ID,
};
pub use ulid::{MonotonicUlid, Ulid, UlidError};
pub use uuid_ext::{
    from_bytes as uuid_from_bytes, is_valid as uuid_is_valid, parse as uuid_parse,
    timestamp_ms as uuid_timestamp_ms, to_bytes as uuid_to_bytes, uuid4, uuid6,
    uuid6_from_timestamp, uuid7,
};

pub use niao_codec::Uuid;
