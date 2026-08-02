//! Parallel batch geodesic distance.

use crate::haversine::haversine_m;
use crate::point::Coord;
use rayon::prelude::*;

/// Haversine distances from `origin` to each candidate (meters), parallel when large.
///
/// >>> use niao_geo::{batch_haversine_m, Coord};
/// >>> let o = Coord::new(0.0, 0.0).unwrap();
/// >>> let pts = vec![Coord::new(1.0, 0.0).unwrap(), Coord::new(0.0, 1.0).unwrap()];
/// >>> let d = batch_haversine_m(o, &pts);
/// >>> d.len()
/// 2
pub fn batch_haversine_m(origin: Coord, targets: &[Coord]) -> Vec<f64> {
    if targets.len() < 1024 {
        targets.iter().map(|t| haversine_m(origin, *t)).collect()
    } else {
        targets
            .par_iter()
            .map(|t| haversine_m(origin, *t))
            .collect()
    }
}

/// Naive single-threaded baseline for benchmarks.
pub fn batch_haversine_m_naive(origin: Coord, targets: &[Coord]) -> Vec<f64> {
    targets.iter().map(|t| haversine_m(origin, *t)).collect()
}
