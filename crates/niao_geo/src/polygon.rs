//! Polygon rings, point-in-polygon, area, perimeter, centroid.

use crate::bbox::Bbox;
use crate::error::{GeoError, GeoResult};
use crate::haversine::{haversine_m, EARTH_RADIUS_M};
use crate::point::Coord;

pub type Ring = Vec<Coord>;

#[derive(Debug, Clone, PartialEq)]
pub struct Polygon {
    pub exterior: Ring,
    pub holes: Vec<Ring>,
}

impl Polygon {
    /// Build polygon from exterior ring and optional holes.
    ///
    /// >>> use niao_geo::{Coord, Polygon};
    /// >>> let ring = vec![
    /// ...     Coord::new(0.0, 0.0).unwrap(),
    /// ...     Coord::new(1.0, 0.0).unwrap(),
    /// ...     Coord::new(1.0, 1.0).unwrap(),
    /// ...     Coord::new(0.0, 1.0).unwrap(),
    /// ...     Coord::new(0.0, 0.0).unwrap(),
    /// ... ];
    /// >>> let p = Polygon::new(ring, vec![]).unwrap();
    /// >>> p.contains(Coord::new(0.5, 0.5).unwrap())
    /// true
    pub fn new(exterior: Ring, holes: Vec<Ring>) -> GeoResult<Self> {
        validate_ring(&exterior, true)?;
        for h in &holes {
            validate_ring(h, true)?;
        }
        Ok(Self { exterior, holes })
    }

    pub fn contains(&self, p: Coord) -> bool {
        if !point_in_ring(p, &self.exterior) {
            return false;
        }
        !self.holes.iter().any(|h| point_in_ring(p, h))
    }

    pub fn bbox(&self) -> GeoResult<Bbox> {
        Bbox::from_points(&self.exterior)
    }

    pub fn centroid(&self) -> GeoResult<Coord> {
        ring_centroid(&self.exterior)
    }

    /// Spherical polygon area in square meters (exterior minus holes).
    pub fn area_m2(&self) -> f64 {
        let ext = ring_area_m2(&self.exterior);
        let holes: f64 = self.holes.iter().map(ring_area_m2).sum();
        (ext - holes).abs()
    }

    /// Geodesic perimeter in meters (exterior + holes).
    pub fn perimeter_m(&self) -> f64 {
        ring_perimeter_m(&self.exterior) + self.holes.iter().map(ring_perimeter_m).sum::<f64>()
    }

    pub fn ring_count(&self) -> usize {
        1 + self.holes.len()
    }

    pub fn exterior_point_count(&self) -> usize {
        self.exterior.len()
    }
}

pub fn validate_ring(ring: &Ring, require_closed: bool) -> GeoResult<()> {
    if ring.len() < 3 {
        return Err(GeoError::RingTooShort);
    }
    let distinct = ring
        .iter()
        .take(ring.len().saturating_sub(1))
        .collect::<Vec<_>>();
    if distinct.len() < 3 {
        return Err(GeoError::RingTooShort);
    }
    if require_closed {
        let first = ring[0];
        let last = ring[ring.len() - 1];
        if (first.lon - last.lon).abs() > 1e-10 || (first.lat - last.lat).abs() > 1e-10 {
            return Err(GeoError::Parse(
                "polygon ring must be closed (first == last)".into(),
            ));
        }
    }
    for c in ring {
        crate::point::validate_lon_lat(c.lon, c.lat)?;
    }
    Ok(())
}

/// Ray-casting point-in-ring test (lon=x, lat=y).
pub fn point_in_ring(p: Coord, ring: &Ring) -> bool {
    let n = ring.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let (mut j, mut i) = (n - 1, 0usize);
    while i < n {
        let xi = ring[i].lon;
        let yi = ring[i].lat;
        let xj = ring[j].lon;
        let yj = ring[j].lat;
        let intersect = ((yi > p.lat) != (yj > p.lat))
            && (p.lon < (xj - xi) * (p.lat - yi) / (yj - yi + f64::EPSILON) + xi);
        if intersect {
            inside = !inside;
        }
        j = i;
        i += 1;
    }
    inside
}

fn ring_area_m2(ring: &Ring) -> f64 {
    if ring.len() < 3 {
        return 0.0;
    }
    let mut area = 0.0;
    let n = ring.len() - 1;
    for i in 0..n {
        let lon1 = ring[i].lon.to_radians();
        let lat1 = ring[i].lat.to_radians();
        let lon2 = ring[i + 1].lon.to_radians();
        let lat2 = ring[i + 1].lat.to_radians();
        area += (lon2 - lon1) * (2.0 + lat1.sin() + lat2.sin());
    }
    area.abs() * EARTH_RADIUS_M * EARTH_RADIUS_M / 2.0
}

fn ring_perimeter_m(ring: &Ring) -> f64 {
    if ring.len() < 2 {
        return 0.0;
    }
    let mut total = 0.0;
    for w in ring.windows(2) {
        total += haversine_m(w[0], w[1]);
    }
    total
}

fn ring_centroid(ring: &Ring) -> GeoResult<Coord> {
    if ring.len() < 3 {
        return Err(GeoError::RingTooShort);
    }
    let n = ring.len() - 1;
    let mut x = 0.0;
    let mut y = 0.0;
    let mut a = 0.0;
    for i in 0..n {
        let p1 = ring[i];
        let p2 = ring[i + 1];
        let f = p1.lon * p2.lat - p2.lon * p1.lat;
        a += f;
        x += (p1.lon + p2.lon) * f;
        y += (p1.lat + p2.lat) * f;
    }
    if a.abs() < f64::EPSILON {
        return Ok(ring[0]);
    }
    a *= 0.5;
    Ok(Coord {
        lon: x / (6.0 * a),
        lat: y / (6.0 * a),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_square() -> Polygon {
        let ring = vec![
            Coord::new(0.0, 0.0).unwrap(),
            Coord::new(1.0, 0.0).unwrap(),
            Coord::new(1.0, 1.0).unwrap(),
            Coord::new(0.0, 1.0).unwrap(),
            Coord::new(0.0, 0.0).unwrap(),
        ];
        Polygon::new(ring, vec![]).unwrap()
    }

    #[test]
    fn contains_center() {
        let p = unit_square();
        assert!(p.contains(Coord::new(0.5, 0.5).unwrap()));
        assert!(!p.contains(Coord::new(2.0, 2.0).unwrap()));
    }
}
