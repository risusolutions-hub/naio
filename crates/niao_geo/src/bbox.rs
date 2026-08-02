//! Axis-aligned bounding boxes in geographic coordinates.

use crate::error::{GeoError, GeoResult};
use crate::haversine::EARTH_RADIUS_M;
use crate::point::Coord;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bbox {
    pub min_lon: f64,
    pub min_lat: f64,
    pub max_lon: f64,
    pub max_lat: f64,
}

impl Bbox {
    /// Create a bbox from corner coordinates.
    ///
    /// >>> use niao_geo::Bbox;
    /// >>> let b = Bbox::new(-1.0, -1.0, 1.0, 1.0).unwrap();
    /// >>> (b.min_lon, b.max_lat)
    /// (-1.0, 1.0)
    pub fn new(min_lon: f64, min_lat: f64, max_lon: f64, max_lat: f64) -> GeoResult<Self> {
        if !min_lon.is_finite()
            || !min_lat.is_finite()
            || !max_lon.is_finite()
            || !max_lat.is_finite()
        {
            return Err(GeoError::InvalidCoord("non-finite bbox value".into()));
        }
        if min_lon > max_lon || min_lat > max_lat {
            return Err(GeoError::InvalidCoord(
                "min corner must be <= max corner".into(),
            ));
        }
        Ok(Self {
            min_lon,
            min_lat,
            max_lon,
            max_lat,
        })
    }

    pub fn from_points(points: &[Coord]) -> GeoResult<Self> {
        if points.is_empty() {
            return Err(GeoError::EmptyGeometry);
        }
        let mut min_lon = points[0].lon;
        let mut max_lon = points[0].lon;
        let mut min_lat = points[0].lat;
        let mut max_lat = points[0].lat;
        for p in &points[1..] {
            min_lon = min_lon.min(p.lon);
            max_lon = max_lon.max(p.lon);
            min_lat = min_lat.min(p.lat);
            max_lat = max_lat.max(p.lat);
        }
        Self::new(min_lon, min_lat, max_lon, max_lat)
    }

    pub fn contains_point(&self, p: Coord) -> bool {
        p.lon >= self.min_lon
            && p.lon <= self.max_lon
            && p.lat >= self.min_lat
            && p.lat <= self.max_lat
    }

    pub fn intersects(&self, other: &Self) -> bool {
        self.min_lon <= other.max_lon
            && self.max_lon >= other.min_lon
            && self.min_lat <= other.max_lat
            && self.max_lat >= other.min_lat
    }

    pub fn union(&self, other: &Self) -> Self {
        Self {
            min_lon: self.min_lon.min(other.min_lon),
            min_lat: self.min_lat.min(other.min_lat),
            max_lon: self.max_lon.max(other.max_lon),
            max_lat: self.max_lat.max(other.max_lat),
        }
    }

    pub fn expand(&mut self, p: Coord) {
        self.min_lon = self.min_lon.min(p.lon);
        self.max_lon = self.max_lon.max(p.lon);
        self.min_lat = self.min_lat.min(p.lat);
        self.max_lat = self.max_lat.max(p.lat);
    }

    pub fn center(&self) -> Coord {
        Coord {
            lon: (self.min_lon + self.max_lon) / 2.0,
            lat: (self.min_lat + self.max_lat) / 2.0,
        }
    }

    /// Approximate bbox area in square meters (equirectangular).
    pub fn area_m2(&self) -> f64 {
        let lat_mid = ((self.min_lat + self.max_lat) / 2.0).to_radians();
        let width = (self.max_lon - self.min_lon).to_radians() * EARTH_RADIUS_M * lat_mid.cos();
        let height = (self.max_lat - self.min_lat).to_radians() * EARTH_RADIUS_M;
        width.abs() * height.abs()
    }
}
