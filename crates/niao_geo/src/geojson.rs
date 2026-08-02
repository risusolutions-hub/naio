//! GeoJSON parse, validate, and stringify.

use crate::bbox::Bbox;
use crate::error::{GeoError, GeoResult};
use crate::linestring::{validate_linestring, LineString};
use crate::point::Coord;
use crate::polygon::{validate_ring, Polygon, Ring};
use geojson::{Feature, FeatureCollection, GeoJson, Geometry, Value};
use serde_json::{Map, Value as JsonValue};

#[derive(Debug, Clone, PartialEq)]
pub enum GeoEntity {
    Point(Coord),
    Bbox(Bbox),
    LineString(LineString),
    Polygon(Polygon),
    Feature {
        geometry: Box<GeoEntity>,
        properties: Option<Map<String, JsonValue>>,
    },
    FeatureCollection(Vec<GeoEntity>),
}

impl GeoEntity {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Point(_) => "point",
            Self::Bbox(_) => "bbox",
            Self::LineString(_) => "linestring",
            Self::Polygon(_) => "polygon",
            Self::Feature { .. } => "feature",
            Self::FeatureCollection(_) => "feature_collection",
        }
    }
}

/// Parse GeoJSON text into a geometry entity.
///
/// >>> use niao_geo::parse_geojson;
/// >>> let e = parse_geojson(r#"{"type":"Point","coordinates":[-73.9857,40.7484]}"#).unwrap();
/// >>> e.kind()
/// "point"
pub fn parse_geojson(text: &str) -> GeoResult<GeoEntity> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(GeoError::Parse("empty GeoJSON".into()));
    }
    let gj: GeoJson = trimmed
        .parse::<GeoJson>()
        .map_err(|e: geojson::Error| GeoError::Parse(e.to_string()))?;
    entity_from_geojson(gj)
}

pub fn valid_geojson(text: &str) -> bool {
    parse_geojson(text).is_ok()
}

pub fn stringify_entity(entity: &GeoEntity) -> GeoResult<String> {
    let gj = geojson_from_entity(entity)?;
    serde_json::to_string(&gj).map_err(|e| GeoError::Parse(e.to_string()))
}

pub fn stringify_pretty(entity: &GeoEntity) -> GeoResult<String> {
    let gj = geojson_from_entity(entity)?;
    serde_json::to_string_pretty(&gj).map_err(|e| GeoError::Parse(e.to_string()))
}

fn entity_from_geojson(gj: GeoJson) -> GeoResult<GeoEntity> {
    match gj {
        GeoJson::Geometry(g) => entity_from_geometry(g),
        GeoJson::Feature(f) => entity_from_feature(f),
        GeoJson::FeatureCollection(fc) => {
            let items = fc
                .features
                .into_iter()
                .map(|f| entity_from_feature(f))
                .collect::<GeoResult<Vec<_>>>()?;
            Ok(GeoEntity::FeatureCollection(items))
        }
    }
}

fn entity_from_feature(f: Feature) -> GeoResult<GeoEntity> {
    let geometry = f
        .geometry
        .ok_or_else(|| GeoError::Parse("feature missing geometry".into()))?;
    let geom = entity_from_geometry(geometry)?;
    Ok(GeoEntity::Feature {
        geometry: Box::new(geom),
        properties: f.properties,
    })
}

fn entity_from_geometry(g: Geometry) -> GeoResult<GeoEntity> {
    match g.value {
        Value::Point(coords) => Ok(GeoEntity::Point(coord_from_vec(&coords)?)),
        Value::MultiPoint(coords) => {
            if coords.is_empty() {
                return Err(GeoError::EmptyGeometry);
            }
            Ok(GeoEntity::Point(coord_from_vec(&coords[0])?))
        }
        Value::LineString(coords) => {
            let line = coords
                .iter()
                .map(|c| coord_from_vec(c))
                .collect::<GeoResult<LineString>>()?;
            validate_linestring(&line)?;
            Ok(GeoEntity::LineString(line))
        }
        Value::MultiLineString(lines) => {
            let first = lines.first().ok_or(GeoError::EmptyGeometry)?;
            let line = first
                .iter()
                .map(|c| coord_from_vec(c))
                .collect::<GeoResult<LineString>>()?;
            validate_linestring(&line)?;
            Ok(GeoEntity::LineString(line))
        }
        Value::Polygon(rings) => polygon_from_rings(rings),
        Value::MultiPolygon(polys) => {
            let first = polys.first().ok_or(GeoError::EmptyGeometry)?;
            polygon_from_rings(first.clone())
        }
        Value::GeometryCollection(_) => Err(GeoError::Parse(
            "GeometryCollection not supported in v0.1".into(),
        )),
    }
}

fn polygon_from_rings(rings: Vec<Vec<Vec<f64>>>) -> GeoResult<GeoEntity> {
    if rings.is_empty() {
        return Err(GeoError::EmptyGeometry);
    }
    let exterior = ring_from_coords(&rings[0])?;
    let holes = rings
        .iter()
        .skip(1)
        .map(|r| ring_from_coords(r))
        .collect::<GeoResult<Vec<_>>>()?;
    Ok(GeoEntity::Polygon(Polygon::new(exterior, holes)?))
}

fn ring_from_coords(coords: &[Vec<f64>]) -> GeoResult<Ring> {
    let mut ring: Ring = coords
        .iter()
        .map(|c| coord_from_vec(c))
        .collect::<GeoResult<_>>()?;
    if ring.len() >= 3 {
        let first = ring[0];
        let last = ring[ring.len() - 1];
        if (first.lon - last.lon).abs() > 1e-10 || (first.lat - last.lat).abs() > 1e-10 {
            ring.push(first);
        }
    }
    validate_ring(&ring, true)?;
    Ok(ring)
}

fn coord_from_vec(v: &[f64]) -> GeoResult<Coord> {
    if v.len() < 2 {
        return Err(GeoError::InvalidCoord("expected [lon, lat]".into()));
    }
    Coord::new(v[0], v[1])
}

fn geojson_from_entity(entity: &GeoEntity) -> GeoResult<GeoJson> {
    match entity {
        GeoEntity::Point(c) => Ok(GeoJson::Geometry(Geometry::new(Value::Point(vec![
            c.lon, c.lat,
        ])))),
        GeoEntity::LineString(line) => Ok(GeoJson::Geometry(Geometry::new(Value::LineString(
            line.iter().map(|c| vec![c.lon, c.lat]).collect(),
        )))),
        GeoEntity::Polygon(poly) => {
            let mut rings = vec![poly.exterior.iter().map(|c| vec![c.lon, c.lat]).collect()];
            for h in &poly.holes {
                rings.push(h.iter().map(|c| vec![c.lon, c.lat]).collect());
            }
            Ok(GeoJson::Geometry(Geometry::new(Value::Polygon(rings))))
        }
        GeoEntity::Bbox(b) => Ok(GeoJson::Geometry(Geometry::new(Value::Polygon(vec![
            vec![
                vec![b.min_lon, b.min_lat],
                vec![b.max_lon, b.min_lat],
                vec![b.max_lon, b.max_lat],
                vec![b.min_lon, b.max_lat],
                vec![b.min_lon, b.min_lat],
            ],
        ])))),
        GeoEntity::Feature {
            geometry,
            properties,
        } => {
            let gj = geojson_from_entity(geometry)?;
            let geom = match gj {
                GeoJson::Geometry(g) => Some(g),
                _ => return Err(GeoError::Parse("nested feature".into())),
            };
            Ok(GeoJson::Feature(Feature {
                geometry: geom,
                properties: properties.clone(),
                ..Default::default()
            }))
        }
        GeoEntity::FeatureCollection(items) => {
            let features = items
                .iter()
                .map(|e| match geojson_from_entity(e)? {
                    GeoJson::Feature(f) => Ok(f),
                    GeoJson::Geometry(g) => Ok(Feature {
                        geometry: Some(g),
                        ..Default::default()
                    }),
                    _ => Err(GeoError::Parse("invalid feature collection item".into())),
                })
                .collect::<GeoResult<Vec<_>>>()?;
            Ok(GeoJson::FeatureCollection(FeatureCollection {
                features,
                ..Default::default()
            }))
        }
    }
}

/// Sample GeoJSON conformance cases (RFC 7946 examples).
pub fn conformance_samples() -> Vec<(&'static str, bool)> {
    vec![
        (r#"{"type":"Point","coordinates":[100.0,0.0]}"#, true),
        (
            r#"{"type":"LineString","coordinates":[[100.0,0.0],[101.0,1.0]]}"#,
            true,
        ),
        (
            r#"{"type":"Polygon","coordinates":[[[100.0,0.0],[101.0,0.0],[101.0,1.0],[100.0,1.0],[100.0,0.0]]]}"#,
            true,
        ),
        (r#"{"type":"Point","coordinates":[200.0,0.0]}"#, false),
        ("not json", false),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_point() {
        let e = parse_geojson(r#"{"type":"Point","coordinates":[-73.9857,40.7484]}"#).unwrap();
        match e {
            GeoEntity::Point(c) => {
                assert!((c.lon + 73.9857).abs() < 1e-4);
            }
            _ => panic!("expected point"),
        }
    }

    #[test]
    fn conformance_suite() {
        for (input, ok) in conformance_samples() {
            assert_eq!(valid_geojson(input), ok, "{input}");
        }
    }
}
