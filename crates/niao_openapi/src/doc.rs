//! In-memory OpenAPI 3 document.

use crate::error::{OpenApiError, OpenApiResult};
use crate::pathutil::{method_key, normalize_path, operation_id, path_params};
use crate::schema::{infer_schema, param as make_param, schema_string};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct OpenApiDoc {
    pub root: Map<String, Value>,
}

impl OpenApiDoc {
    pub fn create(
        info: &Map<String, Value>,
        opts: Option<&Map<String, Value>>,
    ) -> OpenApiResult<Self> {
        let version = opts
            .and_then(|o| o.get("openapi"))
            .and_then(|v| v.as_str())
            .unwrap_or("3.1.0");
        if !version.starts_with("3.") {
            return Err(OpenApiError::new(format!(
                "unsupported OpenAPI version: {version} (expected 3.x)"
            )));
        }
        let title = info
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("API")
            .to_string();
        let ver = info
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("0.1.0")
            .to_string();
        let mut info_obj = Map::new();
        info_obj.insert("title".into(), Value::String(title));
        info_obj.insert("version".into(), Value::String(ver));
        for (k, v) in info {
            if k != "title" && k != "version" {
                info_obj.insert(k.clone(), v.clone());
            }
        }
        let mut root = Map::new();
        root.insert("openapi".into(), Value::String(version.into()));
        root.insert("info".into(), Value::Object(info_obj));
        root.insert("paths".into(), Value::Object(Map::new()));
        if let Some(opts) = opts {
            if let Some(servers) = opts.get("servers") {
                root.insert("servers".into(), servers.clone());
            }
            if let Some(tags) = opts.get("tags") {
                root.insert("tags".into(), tags.clone());
            }
        }
        Ok(Self { root })
    }

    pub fn parse_value(v: Value) -> OpenApiResult<Self> {
        let obj = match v {
            Value::Object(m) => m,
            _ => return Err(OpenApiError::new("OpenAPI document must be a JSON object")),
        };
        if obj.get("openapi").and_then(|x| x.as_str()).is_none() {
            return Err(OpenApiError::new("missing required field: openapi"));
        }
        Ok(Self { root: obj })
    }

    pub fn parse_str(s: &str) -> OpenApiResult<Self> {
        let v: Value =
            serde_json::from_str(s).map_err(|e| OpenApiError::new(format!("invalid JSON: {e}")))?;
        Self::parse_value(v)
    }

    pub fn load(path: &Path) -> OpenApiResult<Self> {
        let s = fs::read_to_string(path)
            .map_err(|e| OpenApiError::new(format!("failed to read {}: {e}", path.display())))?;
        Self::parse_str(&s)
    }

    pub fn save(&self, path: &Path, pretty: bool) -> OpenApiResult<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|e| {
                    OpenApiError::new(format!("failed to create {}: {e}", parent.display()))
                })?;
            }
        }
        let s = self.to_json(pretty)?;
        fs::write(path, s)
            .map_err(|e| OpenApiError::new(format!("failed to write {}: {e}", path.display())))?;
        Ok(())
    }

    pub fn to_json(&self, pretty: bool) -> OpenApiResult<String> {
        if pretty {
            serde_json::to_string_pretty(&Value::Object(self.root.clone()))
                .map_err(|e| OpenApiError::new(format!("serialize failed: {e}")))
        } else {
            serde_json::to_string(&Value::Object(self.root.clone()))
                .map_err(|e| OpenApiError::new(format!("serialize failed: {e}")))
        }
    }

    pub fn to_value(&self) -> Value {
        Value::Object(self.root.clone())
    }

    pub fn version(&self) -> &str {
        self.root
            .get("openapi")
            .and_then(|v| v.as_str())
            .unwrap_or("")
    }

    pub fn set_info(&mut self, info: &Map<String, Value>) {
        let mut cur = self
            .root
            .get("info")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        for (k, v) in info {
            cur.insert(k.clone(), v.clone());
        }
        self.root.insert("info".into(), Value::Object(cur));
    }

    pub fn add_server(&mut self, url: &str, description: Option<&str>) {
        let mut arr = self
            .root
            .get("servers")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let mut s = Map::new();
        s.insert("url".into(), Value::String(url.into()));
        if let Some(d) = description {
            s.insert("description".into(), Value::String(d.into()));
        }
        arr.push(Value::Object(s));
        self.root.insert("servers".into(), Value::Array(arr));
    }

    pub fn add_tag(&mut self, name: &str, description: Option<&str>) {
        let mut arr = self
            .root
            .get("tags")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        // replace existing tag with same name
        arr.retain(|t| t.get("name").and_then(|n| n.as_str()) != Some(name));
        let mut t = Map::new();
        t.insert("name".into(), Value::String(name.into()));
        if let Some(d) = description {
            t.insert("description".into(), Value::String(d.into()));
        }
        arr.push(Value::Object(t));
        self.root.insert("tags".into(), Value::Array(arr));
    }

    fn ensure_components(&mut self) -> &mut Map<String, Value> {
        if !self.root.contains_key("components") {
            self.root
                .insert("components".into(), Value::Object(Map::new()));
        }
        self.root
            .get_mut("components")
            .and_then(|v| v.as_object_mut())
            .expect("components object")
    }

    pub fn add_component(&mut self, kind: &str, name: &str, value: Value) -> OpenApiResult<()> {
        let kind = kind.trim();
        if kind.is_empty() || name.is_empty() {
            return Err(OpenApiError::new(
                "component kind and name must be non-empty",
            ));
        }
        let comps = self.ensure_components();
        if !comps.contains_key(kind) {
            comps.insert(kind.to_string(), Value::Object(Map::new()));
        }
        let bucket = comps
            .get_mut(kind)
            .and_then(|v| v.as_object_mut())
            .ok_or_else(|| OpenApiError::new(format!("components.{kind} is not an object")))?;
        bucket.insert(name.to_string(), value);
        Ok(())
    }

    pub fn add_schema(&mut self, name: &str, schema: Value) -> OpenApiResult<()> {
        self.add_component("schemas", name, schema)
    }

    pub fn add_security_scheme(&mut self, name: &str, scheme: Value) -> OpenApiResult<()> {
        self.add_component("securitySchemes", name, scheme)
    }

    fn paths_mut(&mut self) -> &mut Map<String, Value> {
        if !self.root.contains_key("paths") {
            self.root.insert("paths".into(), Value::Object(Map::new()));
        }
        self.root
            .get_mut("paths")
            .and_then(|v| v.as_object_mut())
            .expect("paths object")
    }

    pub fn add_path(&mut self, path: &str, method: &str, operation: Value) -> OpenApiResult<()> {
        let path = normalize_path(path);
        let method = method_key(method)?;
        let paths = self.paths_mut();
        if !paths.contains_key(&path) {
            paths.insert(path.clone(), Value::Object(Map::new()));
        }
        let item = paths
            .get_mut(&path)
            .and_then(|v| v.as_object_mut())
            .ok_or_else(|| OpenApiError::new(format!("paths.{path} is not an object")))?;
        item.insert(method, operation);
        Ok(())
    }

    /// Add a rich route descriptor object.
    ///
    /// Expected keys: `method`, `path`, plus optional operation fields
    /// (`summary`, `description`, `tags`, `operationId`, `parameters`,
    /// `requestBody`, `responses`, `security`, `deprecated`, …).
    /// Extra keys listed in `body` / `request` / `response_schema` get
    /// folded into requestBody / default 200 response when present.
    pub fn add_route(&mut self, route: &Map<String, Value>) -> OpenApiResult<()> {
        let method = route
            .get("method")
            .and_then(|v| v.as_str())
            .ok_or_else(|| OpenApiError::new("route.method is required"))?;
        let path = route
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| OpenApiError::new("route.path is required"))?;

        // Skip websockets unless explicitly requested
        if route.get("websocket").and_then(|v| v.as_bool()) == Some(true)
            && route.get("include_websocket").and_then(|v| v.as_bool()) != Some(true)
        {
            return Ok(());
        }

        let mut op = Map::new();
        for (k, v) in route {
            match k.as_str() {
                "method" | "path" | "websocket" | "include_websocket" | "permission" | "body"
                | "request" | "response_schema" | "response" | "status" => {}
                _ => {
                    op.insert(k.clone(), v.clone());
                }
            }
        }

        if !op.contains_key("operationId") {
            op.insert(
                "operationId".into(),
                Value::String(operation_id(method, path)?),
            );
        }

        // Auto path parameters for `{id}` / `:id`
        let params_list = path_params(path);
        if !params_list.is_empty() && !op.contains_key("parameters") {
            let mut params = Vec::new();
            for name in &params_list {
                params.push(make_param(name, "path", Some(schema_string(None)), None));
            }
            op.insert("parameters".into(), Value::Array(params));
        }

        // request body from `body` / `request` example or schema
        if !op.contains_key("requestBody") {
            if let Some(body) = route.get("requestBody") {
                op.insert("requestBody".into(), body.clone());
            } else if let Some(schema) = route.get("request") {
                let schema = if schema.get("type").is_some() || schema.get("$ref").is_some() {
                    schema.clone()
                } else {
                    infer_schema(schema)
                };
                op.insert(
                    "requestBody".into(),
                    json!({
                        "required": true,
                        "content": { "application/json": { "schema": schema } }
                    }),
                );
            } else if let Some(body) = route.get("body") {
                let schema = infer_schema(body);
                op.insert(
                    "requestBody".into(),
                    json!({
                        "required": true,
                        "content": { "application/json": { "schema": schema } }
                    }),
                );
            }
        }

        if !op.contains_key("responses") {
            let status = route
                .get("status")
                .and_then(|v| v.as_str())
                .or_else(|| route.get("status").and_then(|v| v.as_i64()).map(|_| ""))
                .filter(|s| !s.is_empty());
            let status_owned;
            let status = if let Some(s) = status {
                s
            } else if let Some(n) = route.get("status").and_then(|v| v.as_i64()) {
                status_owned = n.to_string();
                status_owned.as_str()
            } else {
                "200"
            };
            let mut resp = Map::new();
            resp.insert("description".into(), Value::String("OK".into()));
            if let Some(schema) = route
                .get("response_schema")
                .or_else(|| route.get("response"))
            {
                let schema = if schema.get("type").is_some() || schema.get("$ref").is_some() {
                    schema.clone()
                } else {
                    infer_schema(schema)
                };
                resp.insert(
                    "content".into(),
                    json!({ "application/json": { "schema": schema } }),
                );
            }
            let mut responses = Map::new();
            responses.insert(status.to_string(), Value::Object(resp));
            op.insert("responses".into(), Value::Object(responses));
        }

        // permission → security hint (bearer)
        if let Some(perm) = route.get("permission").and_then(|v| v.as_str()) {
            if !op.contains_key("security") {
                op.insert("security".into(), json!([{ "bearerAuth": [] }]));
            }
            if !op.contains_key("description") {
                op.insert(
                    "description".into(),
                    Value::String(format!("Requires permission: {perm}")),
                );
            }
            // ensure security scheme exists
            let _ = self.add_security_scheme(
                "bearerAuth",
                json!({
                    "type": "http",
                    "scheme": "bearer",
                    "bearerFormat": "JWT"
                }),
            );
        }

        self.add_path(path, method, Value::Object(op))
    }

    pub fn add_routes(&mut self, routes: &[Value]) -> OpenApiResult<()> {
        for (i, r) in routes.iter().enumerate() {
            let obj = r
                .as_object()
                .ok_or_else(|| OpenApiError::new(format!("routes[{i}] must be an object")))?;
            self.add_route(obj)?;
        }
        Ok(())
    }

    pub fn paths(&self) -> Vec<String> {
        self.root
            .get("paths")
            .and_then(|v| v.as_object())
            .map(|m| {
                let mut keys: Vec<String> = m.keys().cloned().collect();
                keys.sort();
                keys
            })
            .unwrap_or_default()
    }

    pub fn schemas(&self) -> Vec<String> {
        self.root
            .get("components")
            .and_then(|v| v.get("schemas"))
            .and_then(|v| v.as_object())
            .map(|m| {
                let mut keys: Vec<String> = m.keys().cloned().collect();
                keys.sort();
                keys
            })
            .unwrap_or_default()
    }

    pub fn operations(&self) -> Vec<Value> {
        let mut out = Vec::new();
        let Some(paths) = self.root.get("paths").and_then(|v| v.as_object()) else {
            return out;
        };
        let methods = [
            "get", "post", "put", "delete", "patch", "options", "head", "trace",
        ];
        let mut path_keys: Vec<&String> = paths.keys().collect();
        path_keys.sort();
        for path in path_keys {
            let Some(item) = paths.get(path).and_then(|v| v.as_object()) else {
                continue;
            };
            for m in methods {
                if let Some(op) = item.get(m) {
                    let mut row = Map::new();
                    row.insert("method".into(), Value::String(m.to_ascii_uppercase()));
                    row.insert("path".into(), Value::String(path.clone()));
                    if let Some(id) = op.get("operationId") {
                        row.insert("operationId".into(), id.clone());
                    }
                    if let Some(s) = op.get("summary") {
                        row.insert("summary".into(), s.clone());
                    }
                    out.push(Value::Object(row));
                }
            }
        }
        out
    }

    pub fn get_operation(&self, path: &str, method: &str) -> OpenApiResult<Option<Value>> {
        let path = normalize_path(path);
        let method = method_key(method)?;
        Ok(self
            .root
            .get("paths")
            .and_then(|v| v.as_object())
            .and_then(|p| p.get(&path))
            .and_then(|v| v.as_object())
            .and_then(|item| item.get(&method))
            .cloned())
    }
}

/// Build a document from a list of route objects + info.
pub fn from_routes(
    routes: &[Value],
    info: Option<&Map<String, Value>>,
    opts: Option<&Map<String, Value>>,
) -> OpenApiResult<OpenApiDoc> {
    let info = info.cloned().unwrap_or_else(|| {
        let mut m = Map::new();
        m.insert("title".into(), Value::String("API".into()));
        m.insert("version".into(), Value::String("0.1.0".into()));
        m
    });
    let mut doc = OpenApiDoc::create(&info, opts)?;
    doc.add_routes(routes)?;
    Ok(doc)
}

/// Build from ahiru `app.routes()` / enriched route tables.
///
/// Accepts `{method, path, permission?, websocket?}` and optional enrichments
/// keyed by `"METHOD path"` or `"path"` in `opts.enrich`.
pub fn from_ahiru(
    routes: &[Value],
    info: Option<&Map<String, Value>>,
    opts: Option<&Map<String, Value>>,
) -> OpenApiResult<OpenApiDoc> {
    let enrich = opts
        .and_then(|o| o.get("enrich"))
        .and_then(|v| v.as_object());
    let mut expanded = Vec::with_capacity(routes.len());
    for r in routes {
        let Some(obj) = r.as_object() else {
            return Err(OpenApiError::new("ahiru route must be an object"));
        };
        let mut merged = obj.clone();
        if let Some(enrich) = enrich {
            let method = obj.get("method").and_then(|v| v.as_str()).unwrap_or("");
            let path = obj.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let key1 = format!("{method} {path}");
            let key2 = path.to_string();
            if let Some(Value::Object(extra)) = enrich.get(&key1).or_else(|| enrich.get(&key2)) {
                for (k, v) in extra {
                    merged.insert(k.clone(), v.clone());
                }
            }
        }
        expanded.push(Value::Object(merged));
    }
    from_routes(&expanded, info, opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_add_route() {
        let mut info = Map::new();
        info.insert("title".into(), json!("Demo"));
        info.insert("version".into(), json!("1.0.0"));
        let mut doc = OpenApiDoc::create(&info, None).unwrap();
        let mut route = Map::new();
        route.insert("method".into(), json!("GET"));
        route.insert("path".into(), json!("/health"));
        route.insert("summary".into(), json!("Health"));
        doc.add_route(&route).unwrap();
        assert_eq!(doc.paths(), vec!["/health"]);
        let op = doc.get_operation("/health", "get").unwrap().unwrap();
        assert_eq!(op["operationId"], "get_health");
    }

    #[test]
    fn from_ahiru_colon_path() {
        let routes =
            vec![json!({"method": "GET", "path": "/users/:id", "permission": "users.read"})];
        let doc = from_ahiru(&routes, None, None).unwrap();
        assert_eq!(doc.paths(), vec!["/users/{id}"]);
        let op = doc.get_operation("/users/{id}", "GET").unwrap().unwrap();
        assert!(op.get("parameters").is_some());
        assert!(op.get("security").is_some());
    }

    #[test]
    fn roundtrip_json() {
        let info = json!({"title": "T", "version": "0.1.0"});
        let doc = OpenApiDoc::create(info.as_object().unwrap(), None).unwrap();
        let s = doc.to_json(false).unwrap();
        let doc2 = OpenApiDoc::parse_str(&s).unwrap();
        assert_eq!(doc2.version(), "3.1.0");
    }
}
