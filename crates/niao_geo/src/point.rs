//! Geographic points (longitude, latitude in WGS84 degrees).

use crate::error::{GeoError, GeoResult};

/// WGS84 point: `lon` east-positive, `lat` north-positive.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coord {
    pub lon: f64,
    pub lat: f64,
}

impl Coord {
    /// Validate longitude/latitude ranges.
    ///
    /// >>> let c = niao_geo::Coord::new(-73.9857, 40.7484).unwrap();
    /// >>> (c.lon, c.lat)
    /// (-73.9857, 40.7484)
    pub fn new(lon: f64, lat: f64) -> GeoResult<Self> {
        validate_lon_lat(lon, lat)?;
        Ok(Self { lon, lat })
    }

    /// Parse `[lon, lat]` array.
    pub fn from_pair(pair: [f64; 2]) -> GeoResult<Self> {
        Self::new(pair[0], pair[1])
    }
}

pub fn validate_lon_lat(lon: f64, lat: f64) -> GeoResult<()> {
    if !lon.is_finite() || !lat.is_finite() {
        return Err(GeoError::InvalidCoord("non-finite value".into()));
    }
    if !(-180.0..=180.0).contains(&lon) {
        return Err(GeoError::InvalidCoord(format!(
            "longitude {lon} not in [-180, 180]"
        )));
    }
    if !(-90.0..=90.0).contains(&lat) {
        return Err(GeoError::InvalidCoord(format!(
            "latitude {lat} not in [-90, 90]"
        )));
    }
    Ok(())
}
