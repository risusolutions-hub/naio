# ngeo — geospatial: haversine, GeoJSON, polygons, tiles

Haversine distance, GeoJSON parse/stringify, points, polygons, bounding boxes, and Web Mercator tile math. Native Rust implementation (~shapely, geopy, geojson subset).

## Import

```niao
import "ngeo"
```

Paths `import "std/ngeo"` and `import "ngeo"` are equivalent. Flat builtins (`ngeo_point`, `ngeo_haversine`, …) are also available globally after import.

## Quick start

```niao
import "ngeo"

let nyc = ngeo.point(-73.9857, 40.7484)
let london = ngeo.point(-0.1276, 51.5072)
print(ngeo.haversine(nyc, london, true))   // ~5570 km

let gj = ngeo.parse_geojson("{\"type\":\"Point\",\"coordinates\":[-73.9857,40.7484]}")
print(ngeo.stringify(gj))

let poly = ngeo.polygon([[
    [0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.0, 0.0]
]])
print(ngeo.contains(poly, ngeo.point(0.5, 0.5)))  // true

let tile = ngeo.lat_lon_to_tile(40.7484, -73.9857, 12)
print(tile.x, tile.y, tile.z)

ngeo.close(nyc)
ngeo.close(london)
ngeo.close(gj)
ngeo.close(poly)
```

## Constructors

| Method | Description |
|--------|-------------|
| `ngeo.point(lon, lat)` | WGS84 point handle. |
| `ngeo.point_from_array([lon, lat])` | Point from coordinate pair. |
| `ngeo.bbox(min_lon, min_lat, max_lon, max_lat)` | Axis-aligned bounding box. |
| `ngeo.bbox_from_points(handles)` | Bbox enclosing point handles. |
| `ngeo.polygon(rings)` | Polygon from array of rings (`[[lon,lat],…]`). |
| `ngeo.linestring(coords)` | LineString from `[[lon,lat],…]`. |
| `ngeo.parse_geojson(s)` | Parse GeoJSON text → geometry handle. |
| `ngeo.valid_geojson(s)` | `true` when string is valid GeoJSON. |
| `ngeo.close(handle)` | Free handle. |

Constructors return an integer **handle** on success, or a catchable `ngeo_error` object on failure.

## Geodesic

| Method | Description |
|--------|-------------|
| `ngeo.haversine(a, b, km?)` | Great-circle distance (meters; pass `true` for km). |
| `ngeo.bearing(a, b)` | Initial bearing in degrees [0, 360). |
| `ngeo.destination(from, bearing_deg, distance_m)` | Point at bearing and distance. |
| `ngeo.midpoint(a, b)` | Great-circle midpoint. |
| `ngeo.distance_to(p, other, km?)` | Haversine from point handle to another point. |
| `ngeo.distances_from(origin, point_handles)` | Parallel batch distances (meters). |

## Point introspection

| Method | Description |
|--------|-------------|
| `ngeo.kind(h)` | `"point"`, `"bbox"`, `"polygon"`, `"linestring"`, `"feature"`, `"feature_collection"`. |
| `ngeo.lon(h)` / `ngeo.lat(h)` | Coordinates in degrees. |
| `ngeo.to_array(h)` | `[lon, lat]`. |

## Bounding box

| Method | Description |
|--------|-------------|
| `ngeo.min_lon(b)` / `min_lat` / `max_lon` / `max_lat` | Corner values. |
| `ngeo.bbox_contains(b, p)` | Point inside bbox. |
| `ngeo.bbox_intersects(a, b)` | Bboxes overlap. |
| `ngeo.bbox_union(a, b)` | Smallest enclosing bbox handle. |
| `ngeo.bbox_center(b)` | Center point handle. |
| `ngeo.bbox_area(b)` | Approximate area in m². |
| `ngeo.bbox_of(geom)` | Bbox of polygon, linestring, or bbox. |

## Polygon & line

| Method | Description |
|--------|-------------|
| `ngeo.contains(poly, p)` | Point-in-polygon (exterior minus holes). |
| `ngeo.area(geom)` | Area in m² (polygon or bbox). |
| `ngeo.perimeter(poly)` | Geodesic perimeter in meters. |
| `ngeo.centroid(geom)` | Centroid point handle. |
| `ngeo.exterior(poly)` | Exterior ring as `[[lon,lat],…]`. |
| `ngeo.ring_count(poly)` | 1 + number of holes. |
| `ngeo.length(line)` | LineString length in meters. |
| `ngeo.point_at(line, distance_m)` | Interpolate along geodesic segments. |

## GeoJSON & tiles

| Method | Description |
|--------|-------------|
| `ngeo.stringify(h, pretty?)` | GeoJSON text from geometry handle. |
| `ngeo.lat_lon_to_tile(lat, lon, z)` | Slippy map tile `{x, y, z}`. |
| `ngeo.tile_bounds(x, y, z)` | Geographic bbox for tile. |
| `ngeo.tile_center(x, y, z)` | Tile center point. |
| `ngeo.tile_to_quadkey(x, y, z)` | Bing Maps quadkey string. |
| `ngeo.quadkey_to_tile(q)` | `{x, y, z}` from quadkey. |

## Errors

Arity/type mistakes raise `RuntimeError`. Domain failures return catchable `ngeo_error` values (use `ntest.is_error` / `try`).

## See also

- [`nipaddr`](NIPADDR.md) — IP geolocation addressing
- [`ntest`](NTEST.md) — test harness
