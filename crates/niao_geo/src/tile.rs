//! Web Mercator / Slippy Map tile math.

use crate::bbox::Bbox;
use crate::error::{GeoError, GeoResult};
use crate::point::Coord;

pub const MAX_ZOOM: u32 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile {
    pub x: u32,
    pub y: u32,
    pub z: u32,
}

pub fn validate_zoom(z: u32) -> GeoResult<()> {
    if z > MAX_ZOOM {
        return Err(GeoError::InvalidZoom);
    }
    Ok(())
}

pub fn lat_lon_to_tile(lat: f64, lon: f64, z: u32) -> GeoResult<Tile> {
    validate_zoom(z)?;
    crate::point::validate_lon_lat(lon, lat)?;
    let n = 1u32 << z;
    let x = ((lon + 180.0) / 360.0 * n as f64).floor() as u32;
    let lat_rad = lat.to_radians();
    let y = ((1.0 - lat_rad.tan().asinh() / std::f64::consts::PI) / 2.0 * n as f64).floor() as u32;
    validate_tile(x, y, z)?;
    Ok(Tile { x, y, z })
}

pub fn validate_tile(x: u32, y: u32, z: u32) -> GeoResult<()> {
    validate_zoom(z)?;
    let n = 1u32 << z;
    if x >= n || y >= n {
        return Err(GeoError::InvalidTile);
    }
    Ok(())
}

/// Geographic bounds of tile `(x, y, z)`.
///
/// >>> use niao_geo::tile_bounds;
/// >>> let b = tile_bounds(0, 0, 0).unwrap();
/// >>> (b.min_lon, b.max_lat)
/// (-180.0, 85.05112877980659)
pub fn tile_bounds(x: u32, y: u32, z: u32) -> GeoResult<Bbox> {
    validate_tile(x, y, z)?;
    let n = 1u32 << z;
    let nf = n as f64;
    let lon_min = x as f64 / nf * 360.0 - 180.0;
    let lon_max = (x + 1) as f64 / nf * 360.0 - 180.0;
    let lat_max = tile_y_to_lat(y, nf);
    let lat_min = tile_y_to_lat(y + 1, nf);
    Bbox::new(lon_min, lat_min, lon_max, lat_max)
}

fn tile_y_to_lat(y: u32, n: f64) -> f64 {
    let yf = y as f64;
    (std::f64::consts::PI * (1.0 - 2.0 * yf / n))
        .sinh()
        .atan()
        .to_degrees()
}

pub fn tile_center(x: u32, y: u32, z: u32) -> GeoResult<Coord> {
    let b = tile_bounds(x, y, z)?;
    Ok(b.center())
}

/// Bing Maps quadkey for tile.
pub fn tile_to_quadkey(x: u32, y: u32, z: u32) -> GeoResult<String> {
    validate_tile(x, y, z)?;
    let mut key = String::with_capacity(z as usize);
    for i in (1..=z).rev() {
        let mask = 1u32 << (i - 1);
        let mut digit = b'0';
        if (x & mask) != 0 {
            digit += 1;
        }
        if (y & mask) != 0 {
            digit += 2;
        }
        key.push(digit as char);
    }
    Ok(key)
}

pub fn quadkey_to_tile(key: &str) -> GeoResult<Tile> {
    if key.is_empty() || key.len() > MAX_ZOOM as usize {
        return Err(GeoError::InvalidQuadkey);
    }
    if !key.bytes().all(|b| matches!(b, b'0'..=b'3')) {
        return Err(GeoError::InvalidQuadkey);
    }
    let z = key.len() as u32;
    validate_zoom(z)?;
    let mut x = 0u32;
    let mut y = 0u32;
    for (i, ch) in key.bytes().enumerate() {
        let mask = 1u32 << (z - 1 - i as u32);
        match ch {
            b'1' => x |= mask,
            b'2' => y |= mask,
            b'3' => {
                x |= mask;
                y |= mask;
            }
            b'0' => {}
            _ => return Err(GeoError::InvalidQuadkey),
        }
    }
    Ok(Tile { x, y, z })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn z0_tile() {
        let t = lat_lon_to_tile(0.0, 0.0, 0).unwrap();
        assert_eq!(t, Tile { x: 0, y: 0, z: 0 });
    }

    #[test]
    fn quadkey_roundtrip() {
        let t = Tile { x: 3, y: 5, z: 4 };
        let q = tile_to_quadkey(t.x, t.y, t.z).unwrap();
        assert_eq!(quadkey_to_tile(&q).unwrap(), t);
    }
}
