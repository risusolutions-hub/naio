//! Haversine distance, bearing, and destination on the WGS84 sphere.

use crate::error::{GeoError, GeoResult};
use crate::point::Coord;

/// WGS84 mean Earth radius in meters.
pub const EARTH_RADIUS_M: f64 = 6_371_008.8;

/// Great-circle distance in meters between two points.
///
/// >>> use niao_geo::{Coord, haversine_m};
/// >>> let nyc = Coord::new(-73.9857, 40.7484).unwrap();
/// >>> let lon = Coord::new(-0.1276, 51.5072).unwrap();
/// >>> (haversine_m(nyc, lon) / 1000.0).round() as i64
/// 5570
pub fn haversine_m(a: Coord, b: Coord) -> f64 {
    let lat1 = a.lat.to_radians();
    let lat2 = b.lat.to_radians();
    let dlat = (b.lat - a.lat).to_radians();
    let dlon = (b.lon - a.lon).to_radians();
    let h = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * h.sqrt().asin()
}

/// Initial bearing from `from` to `to` in degrees [0, 360).
///
/// >>> use niao_geo::{Coord, bearing_deg};
/// >>> let a = Coord::new(0.0, 0.0).unwrap();
/// >>> let b = Coord::new(0.0, 1.0).unwrap();
/// >>> bearing_deg(a, b).round() as i64
/// 0
pub fn bearing_deg(from: Coord, to: Coord) -> f64 {
    let lat1 = from.lat.to_radians();
    let lat2 = to.lat.to_radians();
    let dlon = (to.lon - from.lon).to_radians();
    let y = dlon.sin() * lat2.cos();
    let x = lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * dlon.cos();
    let brng = y.atan2(x).to_degrees();
    (brng + 360.0) % 360.0
}

/// Destination point given start, initial bearing (degrees), and distance (meters).
///
/// >>> use niao_geo::{Coord, destination};
/// >>> let p = Coord::new(0.0, 0.0).unwrap();
/// >>> let d = destination(p, 90.0, 111_319.0);
/// >>> (d.lon.round() as i64, d.lat.round() as i64)
/// (1, 0)
pub fn destination(from: Coord, bearing_deg: f64, distance_m: f64) -> Coord {
    if distance_m == 0.0 {
        return from;
    }
    let lat1 = from.lat.to_radians();
    let lon1 = from.lon.to_radians();
    let brng = bearing_deg.to_radians();
    let ang = distance_m / EARTH_RADIUS_M;
    let sin_lat2 = lat1.sin() * ang.cos() + lat1.cos() * ang.sin() * brng.cos();
    let lat2 = sin_lat2.asin();
    let lon2 =
        lon1 + (brng.sin() * ang.sin() * lat1.cos()).atan2(ang.cos() - lat1.sin() * sin_lat2);
    Coord {
        lon: lon2.to_degrees(),
        lat: lat2.to_degrees(),
    }
}

/// Midpoint along the great-circle arc.
pub fn midpoint(a: Coord, b: Coord) -> Coord {
    let lat1 = a.lat.to_radians();
    let lon1 = a.lon.to_radians();
    let lat2 = b.lat.to_radians();
    let dlon = (b.lon - a.lon).to_radians();
    let bx = lat2.cos() * dlon.cos();
    let by = lat2.cos() * dlon.sin();
    let lat3 = (lat1.sin() + lat2.sin()).atan2(((lat1.cos() + bx).powi(2) + by.powi(2)).sqrt());
    let lon3 = lon1 + by.atan2(lat1.cos() + bx);
    Coord {
        lon: lon3.to_degrees(),
        lat: lat3.to_degrees(),
    }
}

pub fn haversine_km(a: Coord, b: Coord) -> f64 {
    haversine_m(a, b) / 1000.0
}

pub fn validate_distance_m(d: f64) -> GeoResult<()> {
    if !d.is_finite() || d < 0.0 {
        return Err(GeoError::OutOfRange(format!(
            "distance {d} must be finite and >= 0"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nyc_london_distance() {
        let nyc = Coord::new(-73.9857, 40.7484).unwrap();
        let lon = Coord::new(-0.1276, 51.5072).unwrap();
        let km = haversine_km(nyc, lon);
        assert!((5500.0..5600.0).contains(&km));
    }
}
