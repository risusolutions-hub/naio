//! Native ngeo standard library — haversine, GeoJSON, points/polygons,
//! bounding boxes, tile math (~shapely, geopy, geojson subset).
//!
//! Import with `import "ngeo"` (or `import "std/ngeo"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_geo::{
    batch_haversine_m, bearing_deg, destination, haversine_km, haversine_m, lat_lon_to_tile,
    linestring_length_m, midpoint, parse_geojson, point_at_distance, quadkey_to_tile,
    stringify_entity, tile_bounds, tile_center, tile_to_quadkey, valid_geojson,
    validate_linestring, Bbox, Coord, GeoEntity, GeoError, LineString, Polygon, Ring,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const E4605: u32 = codes::E4605_NGEO_ARITY;
const E4606: u32 = codes::E4606_NGEO_ERROR;
const E4607: u32 = codes::E4607_NGEO_TYPE;
const E4608: u32 = codes::E4608_NGEO_INVALID_HANDLE;
const E4609: u32 = codes::E4609_NGEO_PARSE;

thread_local! {
    static STORE: RefCell<HashMap<i64, GeoEntity>> = RefCell::new(HashMap::new());
    static NEXT_ID: RefCell<i64> = const { RefCell::new(1) };
}

fn alloc(entity: GeoEntity) -> i64 {
    let id = NEXT_ID.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    STORE.with(|m| m.borrow_mut().insert(id, entity));
    id
}

fn with_entity<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&GeoEntity) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    STORE.with(|m| match m.borrow().get(&id) {
        Some(e) => Ok(Ok(f(e))),
        None => Ok(Err(error_value(
            E4608,
            "ngeo_error",
            format!("invalid or closed ngeo handle {id}"),
            span,
        ))),
    })
}

fn get_entity(id: i64, span: Span) -> NiaoResult<GeoEntity> {
    match with_entity(id, span, |e| e.clone())? {
        Ok(e) => Ok(e),
        Err(v) => {
            let msg = error_message(&v);
            Err(RuntimeError::at(span, E4606, msg))
        }
    }
}

fn error_message(v: &ValueRef) -> String {
    match &*v.borrow() {
        Value::Object(m) => m
            .get("message")
            .map(|x| match &*x.borrow() {
                Value::String(s) => s.clone(),
                _ => "ngeo error".into(),
            })
            .unwrap_or_else(|| "ngeo error".into()),
        _ => "ngeo error".into(),
    }
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E4607, msg.into())
}

fn ngeo_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E4606, "ngeo_error", msg.into(), span)
}

fn parse_err(span: Span, e: GeoError) -> ValueRef {
    error_value(E4609, "ngeo_error", e.to_string(), span)
}

fn geo_err(span: Span, e: GeoError) -> ValueRef {
    ngeo_err(span, e.to_string())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E4605,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(
    args: &[ValueRef],
    min: usize,
    max: usize,
    name: &str,
    span: Span,
) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E4605,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
        ));
    }
    Ok(())
}

fn string_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn float_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<f64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n as f64),
        Value::Float(f) => Ok(*f),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a number as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn int_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an int as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn handle_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) if *n > 0 => Ok(*n),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an ngeo handle as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn optional_bool(args: &[ValueRef], idx: usize, default: bool) -> bool {
    if args.len() <= idx {
        return default;
    }
    match &*args[idx].borrow() {
        Value::Bool(b) => *b,
        Value::Int(n) => *n != 0,
        _ => default,
    }
}

fn ok_handle(entity: GeoEntity) -> NiaoResult<ValueRef> {
    Ok(Value::Int(alloc(entity)).ref_cell())
}

fn bool_val(b: bool) -> NiaoResult<ValueRef> {
    Ok(Value::Bool(b).ref_cell())
}

fn str_val(s: impl Into<String>) -> NiaoResult<ValueRef> {
    Ok(Value::String(s.into()).ref_cell())
}

fn int_val(n: i64) -> NiaoResult<ValueRef> {
    Ok(Value::Int(n).ref_cell())
}

fn float_val(f: f64) -> NiaoResult<ValueRef> {
    Ok(Value::Float(f).ref_cell())
}

fn coord_pair_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Coord> {
    match &*args[idx].borrow() {
        Value::Array(items) if items.len() >= 2 => {
            let lon = match &*items[0].borrow() {
                Value::Int(n) => *n as f64,
                Value::Float(f) => *f,
                other => {
                    return Err(type_err(
                        span,
                        format!(
                            "{name}() [lon, lat] item 1 must be number, got {}",
                            other.type_name()
                        ),
                    ))
                }
            };
            let lat = match &*items[1].borrow() {
                Value::Int(n) => *n as f64,
                Value::Float(f) => *f,
                other => {
                    return Err(type_err(
                        span,
                        format!(
                            "{name}() [lon, lat] item 2 must be number, got {}",
                            other.type_name()
                        ),
                    ))
                }
            };
            Coord::new(lon, lat).map_err(|e| type_err(span, e.to_string()))
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() expects [lon, lat] array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn coord_from_value(v: &ValueRef, span: Span, ctx: &str) -> NiaoResult<Coord> {
    match &*v.borrow() {
        Value::Int(id) if *id > 0 => {
            let e = get_entity(*id, span)?;
            match e {
                GeoEntity::Point(c) => Ok(c),
                _ => Err(type_err(span, format!("{ctx}: expected point handle"))),
            }
        }
        Value::Array(items) if items.len() >= 2 => {
            coord_pair_arg(std::slice::from_ref(v), 0, ctx, span)
        }
        other => Err(type_err(
            span,
            format!(
                "{ctx}: expected point handle or [lon, lat], got {}",
                other.type_name()
            ),
        )),
    }
}

fn ring_from_array(items: &[ValueRef], span: Span, ctx: &str) -> NiaoResult<Ring> {
    let mut ring = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        match coord_from_value(item, span, ctx) {
            Ok(c) => ring.push(c),
            Err(e) => {
                return Err(type_err(
                    span,
                    format!("{ctx}: ring point {} invalid: {}", i + 1, e.message()),
                ))
            }
        }
    }
    Ok(ring)
}

fn polygon_from_value(v: &ValueRef, span: Span, ctx: &str) -> NiaoResult<Polygon> {
    match &*v.borrow() {
        Value::Int(id) if *id > 0 => {
            let e = get_entity(*id, span)?;
            match e {
                GeoEntity::Polygon(p) => Ok(p),
                _ => Err(type_err(span, format!("{ctx}: expected polygon handle"))),
            }
        }
        Value::Array(rings) => {
            if rings.is_empty() {
                return Err(type_err(
                    span,
                    format!("{ctx}: polygon needs at least one ring"),
                ));
            }
            let exterior =
                ring_from_array(&rings[0].borrow().as_array_items(span, ctx)?, span, ctx)?;
            let mut holes = Vec::new();
            for (i, r) in rings.iter().skip(1).enumerate() {
                let ring = ring_from_array(
                    &r.borrow()
                        .as_array_items(span, &format!("{ctx} hole {}", i + 1))?,
                    span,
                    ctx,
                )?;
                holes.push(ring);
            }
            Polygon::new(exterior, holes).map_err(|e| type_err(span, e.to_string()))
        }
        other => Err(type_err(
            span,
            format!(
                "{ctx}: expected polygon handle or rings array, got {}",
                other.type_name()
            ),
        )),
    }
}

trait ValueArrayExt {
    fn as_array_items(&self, span: Span, ctx: &str) -> NiaoResult<Vec<ValueRef>>;
}

impl ValueArrayExt for Value {
    fn as_array_items(&self, span: Span, ctx: &str) -> NiaoResult<Vec<ValueRef>> {
        match self {
            Value::Array(items) => Ok(items.clone()),
            other => Err(type_err(
                span,
                format!("{ctx}: expected array, got {}", other.type_name()),
            )),
        }
    }
}

fn coord_array(c: Coord) -> ValueRef {
    Value::Array(vec![
        Value::Float(c.lon).ref_cell(),
        Value::Float(c.lat).ref_cell(),
    ])
    .ref_cell()
}

fn coord_list_to_array(coords: &[Coord]) -> NiaoResult<ValueRef> {
    Ok(Value::Array(coords.iter().map(|c| coord_array(*c)).collect()).ref_cell())
}

fn get_bbox(id: i64, span: Span) -> NiaoResult<Bbox> {
    let e = get_entity(id, span)?;
    match e {
        GeoEntity::Bbox(b) => Ok(b),
        GeoEntity::Polygon(p) => p
            .bbox()
            .map_err(|e| RuntimeError::at(span, E4606, e.to_string())),
        _ => Err(type_err(span, "expected bbox or polygon handle")),
    }
}

fn get_point(id: i64, span: Span) -> NiaoResult<Coord> {
    match get_entity(id, span)? {
        GeoEntity::Point(c) => Ok(c),
        _ => Err(type_err(span, "expected point handle")),
    }
}

fn get_polygon(id: i64, span: Span) -> NiaoResult<Polygon> {
    match get_entity(id, span)? {
        GeoEntity::Polygon(p) => Ok(p),
        _ => Err(type_err(span, "expected polygon handle")),
    }
}

fn get_linestring(id: i64, span: Span) -> NiaoResult<LineString> {
    match get_entity(id, span)? {
        GeoEntity::LineString(l) => Ok(l),
        _ => Err(type_err(span, "expected linestring handle")),
    }
}

// ---------------------------------------------------------------------------
// Constructors
// ---------------------------------------------------------------------------

// >>> ngeo.point(-73.9857, 40.7484)
fn ngeo_point(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ngeo_point", span)?;
    match Coord::new(
        float_arg(args, 0, "ngeo_point", span)?,
        float_arg(args, 1, "ngeo_point", span)?,
    ) {
        Ok(c) => ok_handle(GeoEntity::Point(c)),
        Err(e) => Ok(geo_err(span, e)),
    }
}

fn ngeo_point_from_array(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngeo_point_from_array", span)?;
    match coord_pair_arg(args, 0, "ngeo_point_from_array", span) {
        Ok(c) => ok_handle(GeoEntity::Point(c)),
        Err(e) => Err(e),
    }
}

fn ngeo_bbox(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 4, "ngeo_bbox", span)?;
    match Bbox::new(
        float_arg(args, 0, "ngeo_bbox", span)?,
        float_arg(args, 1, "ngeo_bbox", span)?,
        float_arg(args, 2, "ngeo_bbox", span)?,
        float_arg(args, 3, "ngeo_bbox", span)?,
    ) {
        Ok(b) => ok_handle(GeoEntity::Bbox(b)),
        Err(e) => Ok(geo_err(span, e)),
    }
}

fn ngeo_bbox_from_points(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngeo_bbox_from_points", span)?;
    let ids = handles_from_array(args, 0, "ngeo_bbox_from_points", span)?;
    let coords: Vec<Coord> = ids
        .iter()
        .map(|id| get_point(*id, span))
        .collect::<NiaoResult<_>>()?;
    match Bbox::from_points(&coords) {
        Ok(b) => ok_handle(GeoEntity::Bbox(b)),
        Err(e) => Ok(geo_err(span, e)),
    }
}

fn ngeo_polygon(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngeo_polygon", span)?;
    match polygon_from_value(&args[0], span, "ngeo_polygon") {
        Ok(p) => ok_handle(GeoEntity::Polygon(p)),
        Err(e) => Err(e),
    }
}

fn ngeo_linestring(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngeo_linestring", span)?;
    match &*args[0].borrow() {
        Value::Array(items) => {
            let line = ring_from_array(items, span, "ngeo_linestring")?;
            match validate_linestring(&line) {
                Ok(()) => ok_handle(GeoEntity::LineString(line)),
                Err(e) => Ok(geo_err(span, e)),
            }
        }
        other => Err(type_err(
            span,
            format!(
                "ngeo.linestring() expects coordinate array, got {}",
                other.type_name()
            ),
        )),
    }
}

fn ngeo_parse_geojson(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngeo_parse_geojson", span)?;
    match parse_geojson(&string_arg(args, 0, "ngeo_parse_geojson", span)?) {
        Ok(e) => ok_handle(e),
        Err(e) => Ok(parse_err(span, e)),
    }
}

fn ngeo_valid_geojson(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngeo_valid_geojson", span)?;
    bool_val(valid_geojson(&string_arg(
        args,
        0,
        "ngeo_valid_geojson",
        span,
    )?))
}

fn ngeo_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngeo_close", span)?;
    let id = handle_arg(args, 0, "ngeo_close", span)?;
    let removed = STORE.with(|m| m.borrow_mut().remove(&id).is_some());
    bool_val(removed)
}

// ---------------------------------------------------------------------------
// Geodesic (no handle)
// ---------------------------------------------------------------------------

fn ngeo_haversine(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ngeo_haversine", span)?;
    let a = coord_from_value(&args[0], span, "ngeo.haversine")?;
    let b = coord_from_value(&args[1], span, "ngeo.haversine")?;
    let km = optional_bool(args, 2, false);
    if km {
        float_val(haversine_km(a, b))
    } else {
        float_val(haversine_m(a, b))
    }
}

fn ngeo_bearing(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ngeo_bearing", span)?;
    let a = coord_from_value(&args[0], span, "ngeo.bearing")?;
    let b = coord_from_value(&args[1], span, "ngeo.bearing")?;
    float_val(bearing_deg(a, b))
}

fn ngeo_destination(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "ngeo_destination", span)?;
    let from = coord_from_value(&args[0], span, "ngeo.destination")?;
    let bearing = float_arg(args, 1, "ngeo_destination", span)?;
    let dist = float_arg(args, 2, "ngeo_destination", span)?;
    if let Err(e) = niao_geo::validate_distance_m(dist) {
        return Ok(geo_err(span, e));
    }
    ok_handle(GeoEntity::Point(destination(from, bearing, dist)))
}

fn ngeo_midpoint(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ngeo_midpoint", span)?;
    let a = coord_from_value(&args[0], span, "ngeo.midpoint")?;
    let b = coord_from_value(&args[1], span, "ngeo.midpoint")?;
    ok_handle(GeoEntity::Point(midpoint(a, b)))
}

fn ngeo_stringify(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ngeo_stringify", span)?;
    let e = get_entity(handle_arg(args, 0, "ngeo_stringify", span)?, span)?;
    let pretty = optional_bool(args, 1, false);
    match if pretty {
        niao_geo::stringify_pretty(&e)
    } else {
        stringify_entity(&e)
    } {
        Ok(s) => str_val(s),
        Err(err) => Ok(geo_err(span, err)),
    }
}

fn handles_from_array(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> NiaoResult<Vec<i64>> {
    match &*args[idx].borrow() {
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(i, v)| match &*v.borrow() {
                Value::Int(n) if *n > 0 => Ok(*n),
                other => Err(type_err(
                    span,
                    format!(
                        "{name}() array item {} must be ngeo handle, got {}",
                        i + 1,
                        other.type_name()
                    ),
                )),
            })
            .collect(),
        Value::Nil => Ok(Vec::new()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn ngeo_distances_from(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ngeo_distances_from", span)?;
    let origin = coord_from_value(&args[0], span, "ngeo.distances_from")?;
    let ids = handles_from_array(args, 1, "ngeo_distances_from", span)?;
    let targets: Vec<Coord> = ids
        .iter()
        .map(|id| get_point(*id, span))
        .collect::<NiaoResult<_>>()?;
    let dists = batch_haversine_m(origin, &targets);
    Ok(Value::Array(
        dists
            .into_iter()
            .map(|d| Value::Float(d).ref_cell())
            .collect(),
    )
    .ref_cell())
}

// ---------------------------------------------------------------------------
// Introspection
// ---------------------------------------------------------------------------

fn ngeo_kind(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngeo_kind", span)?;
    let e = get_entity(handle_arg(args, 0, "ngeo_kind", span)?, span)?;
    str_val(e.kind())
}

fn ngeo_lon(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngeo_lon", span)?;
    float_val(get_point(handle_arg(args, 0, "ngeo_lon", span)?, span)?.lon)
}

fn ngeo_lat(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngeo_lat", span)?;
    float_val(get_point(handle_arg(args, 0, "ngeo_lat", span)?, span)?.lat)
}

fn ngeo_to_array(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngeo_to_array", span)?;
    let c = get_point(handle_arg(args, 0, "ngeo_to_array", span)?, span)?;
    Ok(coord_array(c))
}

fn ngeo_distance_to(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ngeo_distance_to", span)?;
    let a = get_point(handle_arg(args, 0, "ngeo_distance_to", span)?, span)?;
    let b = coord_from_value(&args[1], span, "ngeo.distance_to")?;
    let km = optional_bool(args, 2, false);
    if km {
        float_val(haversine_km(a, b))
    } else {
        float_val(haversine_m(a, b))
    }
}

// ---------------------------------------------------------------------------
// Bbox
// ---------------------------------------------------------------------------

fn ngeo_min_lon(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngeo_min_lon", span)?;
    float_val(get_bbox(handle_arg(args, 0, "ngeo_min_lon", span)?, span)?.min_lon)
}

fn ngeo_min_lat(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngeo_min_lat", span)?;
    float_val(get_bbox(handle_arg(args, 0, "ngeo_min_lat", span)?, span)?.min_lat)
}

fn ngeo_max_lon(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngeo_max_lon", span)?;
    float_val(get_bbox(handle_arg(args, 0, "ngeo_max_lon", span)?, span)?.max_lon)
}

fn ngeo_max_lat(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngeo_max_lat", span)?;
    float_val(get_bbox(handle_arg(args, 0, "ngeo_max_lat", span)?, span)?.max_lat)
}

fn ngeo_bbox_contains(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ngeo_bbox_contains", span)?;
    let b = get_bbox(handle_arg(args, 0, "ngeo_bbox_contains", span)?, span)?;
    let p = coord_from_value(&args[1], span, "ngeo.bbox_contains")?;
    bool_val(b.contains_point(p))
}

fn ngeo_bbox_intersects(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ngeo_bbox_intersects", span)?;
    let a = get_bbox(handle_arg(args, 0, "ngeo_bbox_intersects", span)?, span)?;
    let b = get_bbox(handle_arg(args, 1, "ngeo_bbox_intersects", span)?, span)?;
    bool_val(a.intersects(&b))
}

fn ngeo_bbox_union(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ngeo_bbox_union", span)?;
    let a = get_bbox(handle_arg(args, 0, "ngeo_bbox_union", span)?, span)?;
    let b = get_bbox(handle_arg(args, 1, "ngeo_bbox_union", span)?, span)?;
    ok_handle(GeoEntity::Bbox(a.union(&b)))
}

fn ngeo_bbox_center(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngeo_bbox_center", span)?;
    let c = get_bbox(handle_arg(args, 0, "ngeo_bbox_center", span)?, span)?.center();
    ok_handle(GeoEntity::Point(c))
}

fn ngeo_bbox_area(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngeo_bbox_area", span)?;
    float_val(get_bbox(handle_arg(args, 0, "ngeo_bbox_area", span)?, span)?.area_m2())
}

fn ngeo_bbox_of(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngeo_bbox_of", span)?;
    let id = handle_arg(args, 0, "ngeo_bbox_of", span)?;
    let e = get_entity(id, span)?;
    match e {
        GeoEntity::Polygon(p) => match p.bbox() {
            Ok(b) => ok_handle(GeoEntity::Bbox(b)),
            Err(err) => Ok(geo_err(span, err)),
        },
        GeoEntity::LineString(line) => {
            let coords: Vec<Coord> = line;
            match Bbox::from_points(&coords) {
                Ok(b) => ok_handle(GeoEntity::Bbox(b)),
                Err(err) => Ok(geo_err(span, err)),
            }
        }
        GeoEntity::Bbox(b) => ok_handle(GeoEntity::Bbox(b)),
        _ => Err(type_err(
            span,
            "bbox_of() requires polygon, linestring, or bbox",
        )),
    }
}

// ---------------------------------------------------------------------------
// Polygon
// ---------------------------------------------------------------------------

fn ngeo_contains(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ngeo_contains", span)?;
    let poly = get_polygon(handle_arg(args, 0, "ngeo_contains", span)?, span)?;
    let p = coord_from_value(&args[1], span, "ngeo.contains")?;
    bool_val(poly.contains(p))
}

fn ngeo_area(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngeo_area", span)?;
    let id = handle_arg(args, 0, "ngeo_area", span)?;
    let e = get_entity(id, span)?;
    match e {
        GeoEntity::Polygon(p) => float_val(p.area_m2()),
        GeoEntity::Bbox(b) => float_val(b.area_m2()),
        _ => Err(type_err(span, "area() requires polygon or bbox")),
    }
}

fn ngeo_perimeter(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngeo_perimeter", span)?;
    float_val(get_polygon(handle_arg(args, 0, "ngeo_perimeter", span)?, span)?.perimeter_m())
}

fn ngeo_centroid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngeo_centroid", span)?;
    let id = handle_arg(args, 0, "ngeo_centroid", span)?;
    let e = get_entity(id, span)?;
    match e {
        GeoEntity::Polygon(p) => match p.centroid() {
            Ok(c) => ok_handle(GeoEntity::Point(c)),
            Err(err) => Ok(geo_err(span, err)),
        },
        GeoEntity::Bbox(b) => ok_handle(GeoEntity::Point(b.center())),
        _ => Err(type_err(span, "centroid() requires polygon or bbox")),
    }
}

fn ngeo_exterior(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngeo_exterior", span)?;
    let poly = get_polygon(handle_arg(args, 0, "ngeo_exterior", span)?, span)?;
    coord_list_to_array(&poly.exterior)
}

fn ngeo_ring_count(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngeo_ring_count", span)?;
    int_val(get_polygon(handle_arg(args, 0, "ngeo_ring_count", span)?, span)?.ring_count() as i64)
}

// ---------------------------------------------------------------------------
// LineString
// ---------------------------------------------------------------------------

fn ngeo_length(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngeo_length", span)?;
    float_val(linestring_length_m(&get_linestring(
        handle_arg(args, 0, "ngeo_length", span)?,
        span,
    )?))
}

fn ngeo_point_at(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ngeo_point_at", span)?;
    let line = get_linestring(handle_arg(args, 0, "ngeo_point_at", span)?, span)?;
    let d = float_arg(args, 1, "ngeo_point_at", span)?;
    match point_at_distance(&line, d) {
        Ok(c) => ok_handle(GeoEntity::Point(c)),
        Err(e) => Ok(geo_err(span, e)),
    }
}

// ---------------------------------------------------------------------------
// Tiles
// ---------------------------------------------------------------------------

fn ngeo_lat_lon_to_tile(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "ngeo_lat_lon_to_tile", span)?;
    let lat = float_arg(args, 0, "ngeo_lat_lon_to_tile", span)?;
    let lon = float_arg(args, 1, "ngeo_lat_lon_to_tile", span)?;
    let z = int_arg(args, 2, "ngeo_lat_lon_to_tile", span)? as u32;
    match lat_lon_to_tile(lat, lon, z) {
        Ok(t) => {
            let mut m = HashMap::new();
            m.insert("x".into(), Value::Int(t.x as i64).ref_cell());
            m.insert("y".into(), Value::Int(t.y as i64).ref_cell());
            m.insert("z".into(), Value::Int(t.z as i64).ref_cell());
            Ok(Value::Object(m).ref_cell())
        }
        Err(e) => Ok(geo_err(span, e)),
    }
}

fn ngeo_tile_bounds(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "ngeo_tile_bounds", span)?;
    let x = int_arg(args, 0, "ngeo_tile_bounds", span)? as u32;
    let y = int_arg(args, 1, "ngeo_tile_bounds", span)? as u32;
    let z = int_arg(args, 2, "ngeo_tile_bounds", span)? as u32;
    match tile_bounds(x, y, z) {
        Ok(b) => ok_handle(GeoEntity::Bbox(b)),
        Err(e) => Ok(geo_err(span, e)),
    }
}

fn ngeo_tile_center(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "ngeo_tile_center", span)?;
    let x = int_arg(args, 0, "ngeo_tile_center", span)? as u32;
    let y = int_arg(args, 1, "ngeo_tile_center", span)? as u32;
    let z = int_arg(args, 2, "ngeo_tile_center", span)? as u32;
    match tile_center(x, y, z) {
        Ok(c) => ok_handle(GeoEntity::Point(c)),
        Err(e) => Ok(geo_err(span, e)),
    }
}

fn ngeo_tile_to_quadkey(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "ngeo_tile_to_quadkey", span)?;
    let x = int_arg(args, 0, "ngeo_tile_to_quadkey", span)? as u32;
    let y = int_arg(args, 1, "ngeo_tile_to_quadkey", span)? as u32;
    let z = int_arg(args, 2, "ngeo_tile_to_quadkey", span)? as u32;
    match tile_to_quadkey(x, y, z) {
        Ok(q) => str_val(q),
        Err(e) => Ok(geo_err(span, e)),
    }
}

fn ngeo_quadkey_to_tile(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngeo_quadkey_to_tile", span)?;
    match quadkey_to_tile(&string_arg(args, 0, "ngeo_quadkey_to_tile", span)?) {
        Ok(t) => {
            let mut m = HashMap::new();
            m.insert("x".into(), Value::Int(t.x as i64).ref_cell());
            m.insert("y".into(), Value::Int(t.y as i64).ref_cell());
            m.insert("z".into(), Value::Int(t.z as i64).ref_cell());
            Ok(Value::Object(m).ref_cell())
        }
        Err(e) => Ok(geo_err(span, e)),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ngeo_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ngeo_fns![
    ("ngeo_point", "point", ngeo_point),
    (
        "ngeo_point_from_array",
        "point_from_array",
        ngeo_point_from_array
    ),
    ("ngeo_bbox", "bbox", ngeo_bbox),
    (
        "ngeo_bbox_from_points",
        "bbox_from_points",
        ngeo_bbox_from_points
    ),
    ("ngeo_polygon", "polygon", ngeo_polygon),
    ("ngeo_linestring", "linestring", ngeo_linestring),
    ("ngeo_parse_geojson", "parse_geojson", ngeo_parse_geojson),
    ("ngeo_valid_geojson", "valid_geojson", ngeo_valid_geojson),
    ("ngeo_close", "close", ngeo_close),
    ("ngeo_haversine", "haversine", ngeo_haversine),
    ("ngeo_bearing", "bearing", ngeo_bearing),
    ("ngeo_destination", "destination", ngeo_destination),
    ("ngeo_midpoint", "midpoint", ngeo_midpoint),
    ("ngeo_stringify", "stringify", ngeo_stringify),
    ("ngeo_distances_from", "distances_from", ngeo_distances_from),
    ("ngeo_kind", "kind", ngeo_kind),
    ("ngeo_lon", "lon", ngeo_lon),
    ("ngeo_lat", "lat", ngeo_lat),
    ("ngeo_to_array", "to_array", ngeo_to_array),
    ("ngeo_distance_to", "distance_to", ngeo_distance_to),
    ("ngeo_min_lon", "min_lon", ngeo_min_lon),
    ("ngeo_min_lat", "min_lat", ngeo_min_lat),
    ("ngeo_max_lon", "max_lon", ngeo_max_lon),
    ("ngeo_max_lat", "max_lat", ngeo_max_lat),
    ("ngeo_bbox_contains", "bbox_contains", ngeo_bbox_contains),
    (
        "ngeo_bbox_intersects",
        "bbox_intersects",
        ngeo_bbox_intersects
    ),
    ("ngeo_bbox_union", "bbox_union", ngeo_bbox_union),
    ("ngeo_bbox_center", "bbox_center", ngeo_bbox_center),
    ("ngeo_bbox_area", "bbox_area", ngeo_bbox_area),
    ("ngeo_bbox_of", "bbox_of", ngeo_bbox_of),
    ("ngeo_contains", "contains", ngeo_contains),
    ("ngeo_area", "area", ngeo_area),
    ("ngeo_perimeter", "perimeter", ngeo_perimeter),
    ("ngeo_centroid", "centroid", ngeo_centroid),
    ("ngeo_exterior", "exterior", ngeo_exterior),
    ("ngeo_ring_count", "ring_count", ngeo_ring_count),
    ("ngeo_length", "length", ngeo_length),
    ("ngeo_point_at", "point_at", ngeo_point_at),
    (
        "ngeo_lat_lon_to_tile",
        "lat_lon_to_tile",
        ngeo_lat_lon_to_tile
    ),
    ("ngeo_tile_bounds", "tile_bounds", ngeo_tile_bounds),
    ("ngeo_tile_center", "tile_center", ngeo_tile_center),
    (
        "ngeo_tile_to_quadkey",
        "tile_to_quadkey",
        ngeo_tile_to_quadkey
    ),
    (
        "ngeo_quadkey_to_tile",
        "quadkey_to_tile",
        ngeo_quadkey_to_tile
    ),
];

fn all_builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs()
        .into_iter()
        .map(|(flat, _, f)| (flat, f))
        .collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "ngeo";
pub const MODULE_PATHS: &[&str] = &["ngeo", "std/ngeo"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    #[test]
    fn point_doctest() {
        let h = ngeo_point(
            &[
                Value::Float(-73.9857).ref_cell(),
                Value::Float(40.7484).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        let lon = ngeo_lon(&[h.clone()], span()).unwrap();
        assert!(
            (match &*lon.borrow() {
                Value::Float(f) => *f,
                _ => 0.0,
            } + 73.9857)
                .abs()
                < 1e-4
        );
    }

    #[test]
    fn haversine_doctest() {
        let a = ngeo_point(
            &[
                Value::Float(-73.9857).ref_cell(),
                Value::Float(40.7484).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        let b = ngeo_point(
            &[
                Value::Float(-0.1276).ref_cell(),
                Value::Float(51.5072).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        let d = ngeo_haversine(&[a, b, Value::Bool(true).ref_cell()], span()).unwrap();
        let km = match &*d.borrow() {
            Value::Float(f) => *f,
            _ => 0.0,
        };
        assert!((5500.0..5600.0).contains(&km));
    }
}
