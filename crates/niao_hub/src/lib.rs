//! Model/dataset hub downloads for Niao (~huggingface-hub subset).
//!
//! HF Hub integration via in-tree `hf-hub`, resumable direct URLs, cache paths,
//! and SHA-256/SHA-512 checksums.

mod checksum;
mod direct;
mod error;
mod hf;

pub use checksum::{hash_bytes, hash_file, verify_bytes, verify_file, HashAlgo};
pub use direct::{download_url, DirectOpts, DirectResult};
pub use error::{HubError, HubResult};
pub use hf::{
    cache_dir_from_env, default_cache_dir, file_sha256, verify_path, DownloadResult, HubClient,
    HubConfig, HubRepo, SnapshotOpts, SnapshotResult, VERSION,
};
