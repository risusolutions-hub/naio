//! ndataset — tabular dataset loading, splits, shuffling, and batch iteration
//! for Niao (~HuggingFace datasets / PyTorch DataLoader subset).
//!
//! Error block: 4120–4125.

pub mod dataset;
pub mod error;
pub mod io;
pub mod loader;

pub use dataset::{from_row_maps, split_ratios, Dataset, SplitOutput};
pub use error::{
    DatasetError, DatasetResult, E4120_NDATASET_ARITY, E4121_NDATASET_ERROR, E4122_NDATASET_TYPE,
    E4123_NDATASET_INVALID_HANDLE, E4124_NDATASET_COLUMN, E4125_NDATASET_INDEX,
};
pub use io::{load_csv, load_json, load_jsonl, parse_jsonl_text};
pub use loader::BatchLoader;
