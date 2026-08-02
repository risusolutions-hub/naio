//! Parallel multi-file dataset reads.

use crate::data::{read_dataset_values, DynData};
use crate::dataset::dataset;
use crate::error::{Hdf5Error, Hdf5Result};
use hdf5_metno::file::File;
use niao_parallel::map as parallel_map;

/// Read the same dataset path from many files in parallel.
pub fn parallel_read(
    paths: &[String],
    dset_path: &str,
    threads: usize,
) -> Vec<Hdf5Result<DynData>> {
    parallel_map(paths, threads.max(1), |path| {
        let file = File::open(path).map_err(Hdf5Error::from)?;
        let ds = dataset(&file, dset_path)?;
        read_dataset_values(&ds, None)
    })
}
