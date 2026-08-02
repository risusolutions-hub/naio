//! Parallel batch MIME detection.

use crate::detector::Detector;
use crate::error::MimeResult;
use crate::extmap::MimeRegistry;
use crate::guess::{from_bytes, sniff_path, SniffOpts};
use crate::magic::CustomMagic;
use crate::types::{GuessTypeResult, MimeMatch};
use niao_parallel;
use std::path::{Path, PathBuf};

pub fn parallel_from_bytes(
    batches: &[Vec<u8>],
    custom: &[CustomMagic],
    threads: usize,
) -> Vec<Option<MimeMatch>> {
    niao_parallel::map(batches, threads, |data| from_bytes(data, custom))
}

pub fn parallel_sniff_paths(
    paths: &[PathBuf],
    registry: &MimeRegistry,
    opts: &SniffOpts,
    custom: &[CustomMagic],
    threads: usize,
) -> Vec<MimeResult<Option<MimeMatch>>> {
    niao_parallel::map(paths, threads, |p| sniff_path(p, registry, opts, custom))
}

pub fn parallel_guess_types(
    filenames: &[String],
    registry: &MimeRegistry,
    strict: bool,
    threads: usize,
) -> Vec<GuessTypeResult> {
    niao_parallel::map(filenames, threads, |name| registry.guess_type(name, strict))
}

pub fn parallel_detect(
    detector: &Detector,
    paths: &[impl AsRef<Path>],
    threads: usize,
) -> Vec<MimeResult<Option<MimeMatch>>> {
    let path_bufs: Vec<PathBuf> = paths.iter().map(|p| p.as_ref().to_path_buf()).collect();
    parallel_sniff_paths(
        &path_bufs,
        &detector.registry,
        &detector.sniff_opts,
        &detector.custom_magic,
        threads,
    )
}
