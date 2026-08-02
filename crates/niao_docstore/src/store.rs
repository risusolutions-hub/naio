//! Document store core — tables, CRUD, persistence (~TinyDB).

use crate::index::SecondaryIndex;
use crate::query::{self, QueryError};
use crate::value::{merge_patch, strip_id, table_from_json, table_to_json, with_id};
use rayon::prelude::*;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_TABLE: &str = "_default";
pub const META_KEY: &str = "_ndocstore";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    Io(String),
    Json(String),
    Query(String),
    NotFound(String),
    Invalid(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Io(m)
            | StoreError::Json(m)
            | StoreError::Query(m)
            | StoreError::NotFound(m)
            | StoreError::Invalid(m) => write!(f, "{m}"),
        }
    }
}

impl From<QueryError> for StoreError {
    fn from(e: QueryError) -> Self {
        StoreError::Query(e.to_string())
    }
}

#[derive(Debug, Clone)]
struct Table {
    docs: BTreeMap<u64, Value>,
    next_id: u64,
    indexes: SecondaryIndex,
}

impl Table {
    fn new() -> Self {
        Self {
            docs: BTreeMap::new(),
            next_id: 1,
            indexes: SecondaryIndex::new(),
        }
    }

    fn from_docs(docs: BTreeMap<u64, Value>) -> Self {
        let next_id = docs.keys().next_back().copied().unwrap_or(0) + 1;
        Self {
            docs,
            next_id,
            indexes: SecondaryIndex::new(),
        }
    }
}

/// Embedded JSON document store.
#[derive(Debug, Clone)]
pub struct DocumentStore {
    path: Option<PathBuf>,
    tables: HashMap<String, Table>,
    default_table: String,
    dirty: bool,
}

impl DocumentStore {
    pub fn memory() -> Self {
        let mut tables = HashMap::new();
        tables.insert(DEFAULT_TABLE.to_string(), Table::new());
        Self {
            path: None,
            tables,
            default_table: DEFAULT_TABLE.to_string(),
            dirty: false,
        }
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();
        if path.exists() {
            let text = fs::read_to_string(&path).map_err(|e| StoreError::Io(e.to_string()))?;
            let mut store = Self::from_json(&text)?;
            store.path = Some(path);
            store.dirty = false;
            Ok(store)
        } else {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent).map_err(|e| StoreError::Io(e.to_string()))?;
                }
            }
            let mut store = Self::memory();
            store.path = Some(path);
            store.dirty = true;
            store.flush()?;
            Ok(store)
        }
    }

    pub fn from_json(text: &str) -> Result<Self, StoreError> {
        let root: Value =
            serde_json::from_str(text).map_err(|e| StoreError::Json(e.to_string()))?;
        let obj = root
            .as_object()
            .ok_or_else(|| StoreError::Json("store root must be a JSON object".into()))?;

        let mut tables = HashMap::new();
        let mut index_meta: HashMap<String, Vec<String>> = HashMap::new();
        let mut default_table = DEFAULT_TABLE.to_string();

        if let Some(meta) = obj.get(META_KEY).and_then(|v| v.as_object()) {
            if let Some(dt) = meta.get("default_table").and_then(|v| v.as_str()) {
                default_table = dt.to_string();
            }
            if let Some(idx) = meta.get("indexes").and_then(|v| v.as_object()) {
                for (table, fields) in idx {
                    if let Some(arr) = fields.as_array() {
                        let list: Vec<String> = arr
                            .iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect();
                        index_meta.insert(table.clone(), list);
                    }
                }
            }
        }

        for (name, table_val) in obj {
            if name == META_KEY {
                continue;
            }
            let docs = table_from_json(table_val).map_err(StoreError::Json)?;
            let mut table = Table::from_docs(docs);
            if let Some(fields) = index_meta.get(name) {
                let snapshot: Vec<(u64, &Value)> =
                    table.docs.iter().map(|(id, d)| (*id, d)).collect();
                for field in fields {
                    table.indexes.create(field, &snapshot);
                }
            }
            tables.insert(name.clone(), table);
        }

        if !tables.contains_key(&default_table) {
            tables.insert(default_table.clone(), Table::new());
        }

        Ok(Self {
            path: None,
            tables,
            default_table,
            dirty: false,
        })
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn set_path(&mut self, path: Option<PathBuf>) {
        self.path = path;
        self.dirty = true;
    }

    pub fn default_table(&self) -> &str {
        &self.default_table
    }

    pub fn set_default_table(&mut self, name: &str) {
        if !self.tables.contains_key(name) {
            self.tables.insert(name.to_string(), Table::new());
        }
        self.default_table = name.to_string();
        self.dirty = true;
    }

    pub fn tables(&self) -> Vec<String> {
        let mut names: Vec<_> = self.tables.keys().cloned().collect();
        names.sort();
        names
    }

    pub fn ensure_table(&mut self, name: &str) {
        self.tables
            .entry(name.to_string())
            .or_insert_with(Table::new);
    }

    pub fn drop_table(&mut self, name: &str) -> Result<bool, StoreError> {
        if name == DEFAULT_TABLE {
            return Err(StoreError::Invalid("cannot drop the _default table".into()));
        }
        let removed = self.tables.remove(name).is_some();
        if removed {
            if self.default_table == name {
                self.default_table = DEFAULT_TABLE.to_string();
                if !self.tables.contains_key(DEFAULT_TABLE) {
                    self.tables.insert(DEFAULT_TABLE.to_string(), Table::new());
                }
            }
            self.dirty = true;
        }
        Ok(removed)
    }

    fn table_mut(&mut self, name: Option<&str>) -> &mut Table {
        let key = name.unwrap_or(&self.default_table).to_string();
        self.tables.entry(key).or_insert_with(Table::new)
    }

    fn table_ref(&self, name: Option<&str>) -> Result<&Table, StoreError> {
        let key = name.unwrap_or(&self.default_table);
        self.tables
            .get(key)
            .ok_or_else(|| StoreError::NotFound(format!("table '{key}' not found")))
    }

    pub fn len(&self, table: Option<&str>) -> Result<usize, StoreError> {
        Ok(self.table_ref(table)?.docs.len())
    }

    pub fn is_empty(&self, table: Option<&str>) -> Result<bool, StoreError> {
        Ok(self.len(table)? == 0)
    }

    pub fn insert(&mut self, table: Option<&str>, doc: Value) -> Result<u64, StoreError> {
        if !doc.is_object() {
            return Err(StoreError::Invalid("document must be a JSON object".into()));
        }
        let doc = strip_id(doc);
        let t = self.table_mut(table);
        let id = t.next_id;
        t.next_id += 1;
        t.indexes.insert_doc(id, &doc);
        t.docs.insert(id, doc);
        self.dirty = true;
        Ok(id)
    }

    pub fn insert_many(
        &mut self,
        table: Option<&str>,
        docs: Vec<Value>,
    ) -> Result<Vec<u64>, StoreError> {
        let mut ids = Vec::with_capacity(docs.len());
        for doc in docs {
            ids.push(self.insert(table, doc)?);
        }
        Ok(ids)
    }

    pub fn get(&self, table: Option<&str>, id: u64) -> Result<Option<Value>, StoreError> {
        Ok(self
            .table_ref(table)?
            .docs
            .get(&id)
            .map(|d| with_id(d.clone(), id)))
    }

    pub fn exists(&self, table: Option<&str>, id: u64) -> Result<bool, StoreError> {
        Ok(self.table_ref(table)?.docs.contains_key(&id))
    }

    pub fn all(&self, table: Option<&str>) -> Result<Vec<Value>, StoreError> {
        Ok(self
            .table_ref(table)?
            .docs
            .iter()
            .map(|(id, d)| with_id(d.clone(), *id))
            .collect())
    }

    pub fn search(&self, table: Option<&str>, query: &Value) -> Result<Vec<Value>, StoreError> {
        let t = self.table_ref(table)?;
        let candidate_ids = self.candidate_ids(t, query)?;
        let mut out = Vec::new();
        match candidate_ids {
            Some(ids) => {
                for id in ids {
                    if let Some(doc) = t.docs.get(&id) {
                        if query::matches(doc, query)? {
                            out.push(with_id(doc.clone(), id));
                        }
                    }
                }
            }
            None => {
                // Full scan — parallel when large.
                if t.docs.len() >= 512 {
                    let matched: Result<Vec<_>, QueryError> = t
                        .docs
                        .par_iter()
                        .filter_map(|(id, doc)| match query::matches(doc, query) {
                            Ok(true) => Some(Ok(with_id(doc.clone(), *id))),
                            Ok(false) => None,
                            Err(e) => Some(Err(e)),
                        })
                        .collect();
                    out = matched?;
                    out.sort_by_key(|d| d.get("_id").and_then(|v| v.as_u64()).unwrap_or(0));
                } else {
                    for (id, doc) in &t.docs {
                        if query::matches(doc, query)? {
                            out.push(with_id(doc.clone(), *id));
                        }
                    }
                }
            }
        }
        Ok(out)
    }

    fn candidate_ids(&self, t: &Table, query: &Value) -> Result<Option<Vec<u64>>, StoreError> {
        let eqs = query::extract_eq_fields(query);
        let mut best: Option<Vec<u64>> = None;
        for (field, value) in eqs {
            if let Some(ids) = t.indexes.lookup_eq(&field, &value) {
                best = Some(match best {
                    None => ids,
                    Some(prev) => intersect_sorted(prev, ids),
                });
            }
        }
        Ok(best)
    }

    pub fn contains(&self, table: Option<&str>, query: &Value) -> Result<bool, StoreError> {
        Ok(!self.search(table, query)?.is_empty())
    }

    pub fn count(&self, table: Option<&str>, query: Option<&Value>) -> Result<usize, StoreError> {
        match query {
            None => self.len(table),
            Some(q) => Ok(self.search(table, q)?.len()),
        }
    }

    pub fn update(
        &mut self,
        table: Option<&str>,
        fields: &Value,
        cond: UpdateCond<'_>,
    ) -> Result<usize, StoreError> {
        if !fields.is_object() {
            return Err(StoreError::Invalid(
                "update fields must be an object".into(),
            ));
        }
        let ids = self.resolve_cond(table, cond)?;
        let t = self.table_mut(table);
        let mut n = 0;
        for id in ids {
            if let Some(old) = t.docs.get(&id).cloned() {
                let mut new_doc = old.clone();
                merge_patch(&mut new_doc, fields);
                if let Value::Object(ref mut m) = new_doc {
                    m.remove("_id");
                }
                t.indexes.update_doc(id, &old, &new_doc);
                t.docs.insert(id, new_doc);
                n += 1;
            }
        }
        if n > 0 {
            self.dirty = true;
        }
        Ok(n)
    }

    pub fn upsert(
        &mut self,
        table: Option<&str>,
        fields: Value,
        query: &Value,
    ) -> Result<u64, StoreError> {
        let found = self.search(table, query)?;
        if found.is_empty() {
            // Merge query equality fields into the new doc when possible.
            let mut doc = fields;
            for (f, v) in query::extract_eq_fields(query) {
                if let Value::Object(ref mut m) = doc {
                    if !m.contains_key(&f) {
                        m.insert(f, v);
                    }
                }
            }
            self.insert(table, doc)
        } else {
            let id = found[0]
                .get("_id")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| StoreError::Invalid("missing _id on matched document".into()))?;
            self.update(table, &fields, UpdateCond::Ids(&[id]))?;
            Ok(id)
        }
    }

    pub fn remove(
        &mut self,
        table: Option<&str>,
        cond: UpdateCond<'_>,
    ) -> Result<usize, StoreError> {
        let ids = self.resolve_cond(table, cond)?;
        let t = self.table_mut(table);
        let mut n = 0;
        for id in ids {
            if let Some(old) = t.docs.remove(&id) {
                t.indexes.remove_doc(id, &old);
                n += 1;
            }
        }
        if n > 0 {
            self.dirty = true;
        }
        Ok(n)
    }

    pub fn truncate(&mut self, table: Option<&str>) -> Result<(), StoreError> {
        let t = self.table_mut(table);
        let fields = t.indexes.fields();
        t.docs.clear();
        t.next_id = 1;
        for f in &fields {
            t.indexes.create(f, &[]);
        }
        self.dirty = true;
        Ok(())
    }

    fn resolve_cond(
        &self,
        table: Option<&str>,
        cond: UpdateCond<'_>,
    ) -> Result<Vec<u64>, StoreError> {
        match cond {
            UpdateCond::Ids(ids) => Ok(ids.to_vec()),
            UpdateCond::Query(q) => {
                let rows = self.search(table, q)?;
                Ok(rows
                    .iter()
                    .filter_map(|d| d.get("_id").and_then(|v| v.as_u64()))
                    .collect())
            }
        }
    }

    pub fn create_index(&mut self, table: Option<&str>, field: &str) -> Result<(), StoreError> {
        if field.is_empty() || field == "_id" {
            return Err(StoreError::Invalid(
                "index field must be a non-empty path other than _id".into(),
            ));
        }
        let t = self.table_mut(table);
        if t.indexes.has(field) {
            return Ok(());
        }
        let snapshot: Vec<(u64, &Value)> = t.docs.iter().map(|(id, d)| (*id, d)).collect();
        t.indexes.create(field, &snapshot);
        self.dirty = true;
        Ok(())
    }

    pub fn drop_index(&mut self, table: Option<&str>, field: &str) -> Result<bool, StoreError> {
        let t = self.table_mut(table);
        let removed = t.indexes.drop(field);
        if removed {
            self.dirty = true;
        }
        Ok(removed)
    }

    pub fn indexes(&self, table: Option<&str>) -> Result<Vec<String>, StoreError> {
        Ok(self.table_ref(table)?.indexes.fields())
    }

    pub fn to_json_value(&self) -> Value {
        let mut root = Map::new();
        for (name, table) in &self.tables {
            root.insert(name.clone(), table_to_json(&table.docs));
        }
        let mut meta = Map::new();
        meta.insert(
            "default_table".into(),
            Value::String(self.default_table.clone()),
        );
        let mut idx_meta = Map::new();
        for (name, table) in &self.tables {
            let fields = table.indexes.fields();
            if !fields.is_empty() {
                idx_meta.insert(
                    name.clone(),
                    Value::Array(fields.into_iter().map(Value::String).collect()),
                );
            }
        }
        if !idx_meta.is_empty() {
            meta.insert("indexes".into(), Value::Object(idx_meta));
        }
        root.insert(META_KEY.into(), Value::Object(meta));
        Value::Object(root)
    }

    pub fn to_json_string(&self, pretty: bool) -> Result<String, StoreError> {
        let v = self.to_json_value();
        if pretty {
            serde_json::to_string_pretty(&v).map_err(|e| StoreError::Json(e.to_string()))
        } else {
            serde_json::to_string(&v).map_err(|e| StoreError::Json(e.to_string()))
        }
    }

    pub fn flush(&mut self) -> Result<(), StoreError> {
        let Some(path) = self.path.clone() else {
            return Ok(());
        };
        let text = self.to_json_string(true)?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, text.as_bytes()).map_err(|e| StoreError::Io(e.to_string()))?;
        fs::rename(&tmp, &path).map_err(|e| StoreError::Io(e.to_string()))?;
        self.dirty = false;
        Ok(())
    }

    pub fn dirty(&self) -> bool {
        self.dirty
    }

    /// Estimate whether an equality on `field` would hit the index (for benches).
    pub fn index_lookup_count(
        &self,
        table: Option<&str>,
        field: &str,
        value: &Value,
    ) -> Result<usize, StoreError> {
        let t = self.table_ref(table)?;
        Ok(t.indexes
            .lookup_eq(field, value)
            .map(|v| v.len())
            .unwrap_or(0))
    }
}

/// Condition for update/remove: either explicit ids or a query.
#[derive(Debug, Clone, Copy)]
pub enum UpdateCond<'a> {
    Ids(&'a [u64]),
    Query(&'a Value),
}

fn intersect_sorted(a: Vec<u64>, b: Vec<u64>) -> Vec<u64> {
    let mut out = Vec::new();
    let mut i = 0;
    let mut j = 0;
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                out.push(a[i]);
                i += 1;
                j += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn crud_and_search() {
        let mut db = DocumentStore::memory();
        let id = db.insert(None, json!({"name": "Ada", "age": 36})).unwrap();
        assert_eq!(id, 1);
        db.insert(None, json!({"name": "Bob", "age": 25})).unwrap();
        let rows = db.search(None, &json!({"gt": {"age": 30}})).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["name"], "Ada");
        let n = db
            .update(
                None,
                &json!({"age": 37}),
                UpdateCond::Query(&json!({"name": "Ada"})),
            )
            .unwrap();
        assert_eq!(n, 1);
        assert_eq!(db.get(None, 1).unwrap().unwrap()["age"], 37);
    }

    #[test]
    fn secondary_index() {
        let mut db = DocumentStore::memory();
        for i in 0..100 {
            db.insert(None, json!({"k": i % 10, "i": i})).unwrap();
        }
        db.create_index(None, "k").unwrap();
        let rows = db.search(None, &json!({"k": 3})).unwrap();
        assert_eq!(rows.len(), 10);
    }

    #[test]
    fn persist_roundtrip() {
        let dir = std::env::temp_dir().join(format!("ndocstore_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("db.json");
        {
            let mut db = DocumentStore::open(&path).unwrap();
            db.insert(None, json!({"x": 1})).unwrap();
            db.create_index(None, "x").unwrap();
            db.flush().unwrap();
        }
        let db = DocumentStore::open(&path).unwrap();
        assert_eq!(db.len(None).unwrap(), 1);
        assert_eq!(db.indexes(None).unwrap(), vec!["x".to_string()]);
        let _ = fs::remove_dir_all(&dir);
    }
}
