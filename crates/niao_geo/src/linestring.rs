//! LineString length and interpolation along geodesic segments.

use crate::error::{GeoError, GeoResult};
use crate::haversine::{destination, haversine_m, validate_distance_m};
use crate::point::Coord;

pub type LineString = Vec<Coord>;

pub fn validate_linestring(line: &LineString) -> GeoResult<()> {
    if line.len() < 2 {
        return Err(GeoError::Parse("linestring needs at least 2 points".into()));
    }
    for c in line {
        crate::point::validate_lon_lat(c.lon, c.lat)?;
    }
    Ok(())
}

/// Total geodesic length in meters.
///
/// >>> use niao_geo::{Coord, linestring_length_m};
/// >>> let line = vec![Coord::new(0.0, 0.0).unwrap(), Coord::new(1.0, 0.0).unwrap()];
/// >>> (linestring_length_m(&line) / 1000.0).round() as i64
/// 111
pub fn linestring_length_m(line: &LineString) -> f64 {
    if line.len() < 2 {
        return 0.0;
    }
    line.windows(2).map(|w| haversine_m(w[0], w[1])).sum()
}

/// Point at distance `d` meters from the start along the line.
pub fn point_at_distance(line: &LineString, distance_m: f64) -> GeoResult<Coord> {
    validate_distance_m(distance_m)?;
    if line.is_empty() {
        return Err(GeoError::EmptyGeometry);
    }
    if distance_m == 0.0 {
        return Ok(line[0]);
    }
    let total = linestring_length_m(line);
    if distance_m >= total {
        return Ok(*line.last().unwrap());
    }
    let mut acc = 0.0;
    for w in line.windows(2) {
        let seg = haversine_m(w[0], w[1]);
        if acc + seg >= distance_m {
            let remain = distance_m - acc;
            let brng = crate::haversine::bearing_deg(w[0], w[1]);
            return Ok(destination(w[0], brng, remain));
        }
        acc += seg;
    }
    Ok(*line.last().unwrap())
}
