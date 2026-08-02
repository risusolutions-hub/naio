//! Geospatial: haversine, GeoJSON, points/polygons, bounding boxes, tile math.
//! (~shapely, geopy, geojson subset)

mod batch;
mod bbox;
mod error;
mod geojson;
mod haversine;
mod linestring;
mod point;
mod polygon;
mod tile;

pub use batch::{batch_haversine_m, batch_haversine_m_naive};
pub use bbox::Bbox;
pub use error::{GeoError, GeoResult};
pub use geojson::{
    conformance_samples, parse_geojson, stringify_entity, stringify_pretty, valid_geojson,
    GeoEntity,
};
pub use haversine::{
    bearing_deg, destination, haversine_km, haversine_m, midpoint, validate_distance_m,
    EARTH_RADIUS_M,
};
pub use linestring::{linestring_length_m, point_at_distance, validate_linestring, LineString};
pub use point::{validate_lon_lat, Coord};
pub use polygon::{point_in_ring, validate_ring, Polygon, Ring};
pub use tile::{
    lat_lon_to_tile, quadkey_to_tile, tile_bounds, tile_center, tile_to_quadkey, validate_tile,
    validate_zoom, Tile, MAX_ZOOM,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn haversine_zero() {
        let p = Coord::new(0.0, 0.0).unwrap();
        assert_eq!(haversine_m(p, p), 0.0);
    }

    #[test]
    fn bbox_union() {
        let a = Bbox::new(0.0, 0.0, 1.0, 1.0).unwrap();
        let b = Bbox::new(0.5, 0.5, 2.0, 2.0).unwrap();
        let u = a.union(&b);
        assert_eq!(u.max_lon, 2.0);
    }
}
