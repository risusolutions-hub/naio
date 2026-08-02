//! Inverted-index FTS engine with BM25, phrase, prefix, and facets.

use crate::error::{FtsError, FtsResult};
use crate::query::{parse, Query};
use crate::score::bm25;
use crate::tokenize::{tokenize, tokenize_with_positions};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::Path;

const DEFAULT_K1: f64 = 1.2;
const DEFAULT_B: f64 = 0.75;
const MAGIC: &str = "nfts-v1";

/// A single posting: doc ordinal + term frequency + positions (for phrases).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Posting {
    doc_ord: u32,
    tf: u32,
    positions: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DocStore {
    id: String,
    fields: HashMap<String, String>,
    facets: HashMap<String, String>,
}

/// Search hit returned to callers.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub id: String,
    pub score: f64,
    pub fields: HashMap<String, String>,
    pub facets: HashMap<String, String>,
}

/// Facet bucket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FacetCount {
    pub value: String,
    pub count: u64,
}

/// Schema summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaInfo {
    pub fields: Vec<String>,
    pub facet_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Snapshot {
    magic: String,
    k1: f64,
    b: f64,
    docs: Vec<DocStore>,
    /// field -> term -> postings
    inverted: HashMap<String, BTreeMap<String, Vec<Posting>>>,
    /// field -> doc_ord -> token length
    doc_lens: HashMap<String, HashMap<u32, u32>>,
    /// facet_field -> value -> doc ords
    facet_index: HashMap<String, HashMap<String, Vec<u32>>>,
    /// known field names
    field_names: Vec<String>,
    facet_names: Vec<String>,
}

/// In-memory full-text index.
#[derive(Debug, Clone)]
pub struct Index {
    k1: f64,
    b: f64,
    /// doc_id -> store
    docs: HashMap<String, DocStore>,
    /// ordinal -> doc_id
    ord_to_id: Vec<String>,
    /// doc_id -> ordinal
    id_to_ord: HashMap<String, u32>,
    /// field -> term -> postings (sorted by doc_ord)
    inverted: HashMap<String, BTreeMap<String, Vec<Posting>>>,
    /// field -> doc_ord -> length
    doc_lens: HashMap<String, HashMap<u32, u32>>,
    /// facet_field -> value -> set of doc ords
    facet_index: HashMap<String, HashMap<String, HashSet<u32>>>,
    field_names: HashSet<String>,
    facet_names: HashSet<String>,
    /// Free ordinals from deletes (reuse to keep vectors compact).
    free_ords: Vec<u32>,
}

impl Default for Index {
    fn default() -> Self {
        Self::new()
    }
}

impl Index {
    pub fn new() -> Self {
        Self {
            k1: DEFAULT_K1,
            b: DEFAULT_B,
            docs: HashMap::new(),
            ord_to_id: Vec::new(),
            id_to_ord: HashMap::new(),
            inverted: HashMap::new(),
            doc_lens: HashMap::new(),
            facet_index: HashMap::new(),
            field_names: HashSet::new(),
            facet_names: HashSet::new(),
            free_ords: Vec::new(),
        }
    }

    pub fn open_or_create(path: Option<&str>) -> FtsResult<Self> {
        match path {
            Some(p) if Path::new(p).exists() => Self::load(p),
            _ => Ok(Self::new()),
        }
    }

    pub fn count(&self) -> usize {
        self.docs.len()
    }

    pub fn schema(&self) -> SchemaInfo {
        let mut fields: Vec<_> = self.field_names.iter().cloned().collect();
        fields.sort();
        let mut facet_fields: Vec<_> = self.facet_names.iter().cloned().collect();
        facet_fields.sort();
        SchemaInfo {
            fields,
            facet_fields,
        }
    }

    pub fn clear(&mut self) {
        *self = Self::new();
    }

    pub fn get_fields(
        &self,
        doc_id: &str,
    ) -> Option<(HashMap<String, String>, HashMap<String, String>)> {
        self.docs
            .get(doc_id)
            .map(|d| (d.fields.clone(), d.facets.clone()))
    }

    /// Insert a document. Fails if `doc_id` already exists.
    pub fn add(
        &mut self,
        doc_id: &str,
        fields: HashMap<String, String>,
        facets: HashMap<String, String>,
    ) -> FtsResult<()> {
        if self.docs.contains_key(doc_id) {
            return Err(FtsError::new(format!("document already exists: {doc_id}")));
        }
        self.index_doc(doc_id, fields, facets);
        Ok(())
    }

    /// Insert or replace a document.
    pub fn update(
        &mut self,
        doc_id: &str,
        fields: HashMap<String, String>,
        facets: HashMap<String, String>,
    ) {
        if self.docs.contains_key(doc_id) {
            let _ = self.delete(doc_id);
        }
        self.index_doc(doc_id, fields, facets);
    }

    pub fn delete(&mut self, doc_id: &str) -> bool {
        let Some(ord) = self.id_to_ord.remove(doc_id) else {
            return false;
        };
        let Some(store) = self.docs.remove(doc_id) else {
            return false;
        };
        if (ord as usize) < self.ord_to_id.len() {
            self.ord_to_id[ord as usize].clear();
        }
        self.free_ords.push(ord);

        for field in store.fields.keys() {
            if let Some(terms) = self.inverted.get_mut(field) {
                for postings in terms.values_mut() {
                    postings.retain(|p| p.doc_ord != ord);
                }
                terms.retain(|_, v| !v.is_empty());
            }
            if let Some(lens) = self.doc_lens.get_mut(field) {
                lens.remove(&ord);
            }
        }
        for (ff, val) in &store.facets {
            if let Some(by_val) = self.facet_index.get_mut(ff) {
                if let Some(set) = by_val.get_mut(val) {
                    set.remove(&ord);
                }
            }
        }
        true
    }

    fn alloc_ord(&mut self, doc_id: &str) -> u32 {
        if let Some(ord) = self.free_ords.pop() {
            if (ord as usize) < self.ord_to_id.len() {
                self.ord_to_id[ord as usize] = doc_id.to_string();
            } else {
                self.ord_to_id.resize(ord as usize + 1, String::new());
                self.ord_to_id[ord as usize] = doc_id.to_string();
            }
            self.id_to_ord.insert(doc_id.to_string(), ord);
            ord
        } else {
            let ord = self.ord_to_id.len() as u32;
            self.ord_to_id.push(doc_id.to_string());
            self.id_to_ord.insert(doc_id.to_string(), ord);
            ord
        }
    }

    fn index_doc(
        &mut self,
        doc_id: &str,
        fields: HashMap<String, String>,
        facets: HashMap<String, String>,
    ) {
        let ord = self.alloc_ord(doc_id);
        for (field, text) in &fields {
            self.field_names.insert(field.clone());
            let tokens = tokenize_with_positions(text);
            let len = tokens.len() as u32;
            self.doc_lens
                .entry(field.clone())
                .or_default()
                .insert(ord, len);

            // Group positions by term.
            let mut by_term: HashMap<String, Vec<u32>> = HashMap::new();
            for (term, pos) in tokens {
                by_term.entry(term).or_default().push(pos);
            }
            let inv = self.inverted.entry(field.clone()).or_default();
            for (term, positions) in by_term {
                let tf = positions.len() as u32;
                let postings = inv.entry(term).or_default();
                // Keep postings sorted by doc_ord.
                let posting = Posting {
                    doc_ord: ord,
                    tf,
                    positions,
                };
                match postings.binary_search_by_key(&ord, |p| p.doc_ord) {
                    Ok(i) => postings[i] = posting,
                    Err(i) => postings.insert(i, posting),
                }
            }
        }
        for (ff, val) in &facets {
            self.facet_names.insert(ff.clone());
            self.facet_index
                .entry(ff.clone())
                .or_default()
                .entry(val.clone())
                .or_default()
                .insert(ord);
        }
        self.docs.insert(
            doc_id.to_string(),
            DocStore {
                id: doc_id.to_string(),
                fields,
                facets,
            },
        );
    }

    fn avg_dl(&self, field: &str) -> f64 {
        let Some(lens) = self.doc_lens.get(field) else {
            return 1.0;
        };
        if lens.is_empty() {
            return 1.0;
        }
        let sum: u64 = lens.values().map(|&l| l as u64).sum();
        sum as f64 / lens.len() as f64
    }

    fn n_docs(&self) -> u64 {
        self.docs.len() as u64
    }

    /// Search with BM25 ranking.
    pub fn search(&self, query: &str, top_k: usize, default_field: Option<&str>) -> Vec<Hit> {
        let q = parse(query);
        if matches!(&q, Query::And(v) if v.is_empty()) {
            return Vec::new();
        }
        let scores = self.eval_query(&q, default_field);
        self.topk_hits(scores, top_k)
    }

    fn topk_hits(&self, scores: HashMap<u32, f64>, top_k: usize) -> Vec<Hit> {
        if scores.is_empty() || top_k == 0 {
            return Vec::new();
        }
        let mut scored: Vec<(u32, f64)> = scores.into_iter().filter(|(_, s)| *s > 0.0).collect();
        if scored.len() > 64 {
            // Parallel sort for larger candidate sets.
            scored.par_sort_unstable_by(|a, b| {
                b.1.partial_cmp(&a.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.0.cmp(&b.0))
            });
        } else {
            scored.sort_unstable_by(|a, b| {
                b.1.partial_cmp(&a.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.0.cmp(&b.0))
            });
        }
        scored.truncate(top_k);
        let mut hits = Vec::with_capacity(scored.len());
        for (ord, score) in scored {
            if let Some(id) = self.ord_to_id.get(ord as usize) {
                if id.is_empty() {
                    continue;
                }
                if let Some(doc) = self.docs.get(id) {
                    hits.push(Hit {
                        id: id.clone(),
                        score,
                        fields: doc.fields.clone(),
                        facets: doc.facets.clone(),
                    });
                }
            }
        }
        hits
    }

    fn eval_query(&self, q: &Query, default_field: Option<&str>) -> HashMap<u32, f64> {
        match q {
            Query::Term { field, term } => {
                self.score_term(field.as_deref().or(default_field), term)
            }
            Query::Prefix { field, prefix } => {
                self.score_prefix(field.as_deref().or(default_field), prefix)
            }
            Query::Phrase { field, terms } => {
                self.score_phrase(field.as_deref().or(default_field), terms)
            }
            Query::And(parts) => {
                if parts.is_empty() {
                    return HashMap::new();
                }
                let mut iter = parts.iter().map(|p| self.eval_query(p, default_field));
                let mut acc = iter.next().unwrap_or_default();
                for next in iter {
                    acc.retain(|k, s| {
                        if let Some(ns) = next.get(k) {
                            *s += ns;
                            true
                        } else {
                            false
                        }
                    });
                }
                acc
            }
            Query::Or(parts) => {
                let mut acc: HashMap<u32, f64> = HashMap::new();
                for p in parts {
                    for (k, v) in self.eval_query(p, default_field) {
                        *acc.entry(k).or_default() += v;
                    }
                }
                acc
            }
            Query::Not(inner) => {
                let banned: HashSet<u32> =
                    self.eval_query(inner, default_field).into_keys().collect();
                let mut all = HashMap::new();
                for (id, _) in &self.docs {
                    if let Some(&ord) = self.id_to_ord.get(id) {
                        if !banned.contains(&ord) {
                            all.insert(ord, 1.0);
                        }
                    }
                }
                all
            }
        }
    }

    fn fields_for(&self, field: Option<&str>) -> Vec<String> {
        if let Some(f) = field {
            vec![f.to_string()]
        } else {
            let mut v: Vec<_> = self.field_names.iter().cloned().collect();
            v.sort();
            v
        }
    }

    fn score_term(&self, field: Option<&str>, term: &str) -> HashMap<u32, f64> {
        let mut scores: HashMap<u32, f64> = HashMap::new();
        let n = self.n_docs();
        for f in self.fields_for(field) {
            let avg = self.avg_dl(&f);
            let Some(terms) = self.inverted.get(&f) else {
                continue;
            };
            let Some(postings) = terms.get(term) else {
                continue;
            };
            let df = postings.len() as u64;
            let lens = self.doc_lens.get(&f);
            for p in postings {
                let dl = lens.and_then(|m| m.get(&p.doc_ord)).copied().unwrap_or(1) as f64;
                let s = bm25(p.tf as f64, df, n, dl, avg, self.k1, self.b);
                *scores.entry(p.doc_ord).or_default() += s;
            }
        }
        scores
    }

    fn score_prefix(&self, field: Option<&str>, prefix: &str) -> HashMap<u32, f64> {
        let mut scores: HashMap<u32, f64> = HashMap::new();
        if prefix.is_empty() {
            return scores;
        }
        let n = self.n_docs();
        for f in self.fields_for(field) {
            let avg = self.avg_dl(&f);
            let Some(terms) = self.inverted.get(&f) else {
                continue;
            };
            let lens = self.doc_lens.get(&f);
            // BTreeMap range scan for prefix.
            for (term, postings) in terms.range(prefix.to_string()..) {
                if !term.starts_with(prefix) {
                    break;
                }
                let df = postings.len() as u64;
                for p in postings {
                    let dl = lens.and_then(|m| m.get(&p.doc_ord)).copied().unwrap_or(1) as f64;
                    let s = bm25(p.tf as f64, df, n, dl, avg, self.k1, self.b);
                    *scores.entry(p.doc_ord).or_default() += s;
                }
            }
        }
        scores
    }

    fn score_phrase(&self, field: Option<&str>, terms: &[String]) -> HashMap<u32, f64> {
        let mut scores: HashMap<u32, f64> = HashMap::new();
        if terms.is_empty() {
            return scores;
        }
        if terms.len() == 1 {
            return self.score_term(field, &terms[0]);
        }
        let n = self.n_docs();
        for f in self.fields_for(field) {
            let avg = self.avg_dl(&f);
            let Some(inv) = self.inverted.get(&f) else {
                continue;
            };
            // Start from rarest term's postings.
            let mut posting_lists: Vec<&[Posting]> = Vec::with_capacity(terms.len());
            let mut ok = true;
            for t in terms {
                match inv.get(t) {
                    Some(p) => posting_lists.push(p.as_slice()),
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if !ok {
                continue;
            }
            let first = posting_lists[0];
            for p0 in first {
                if !phrase_matches(&posting_lists, p0.doc_ord) {
                    continue;
                }
                let mut combined_tf = p0.tf;
                for plist in posting_lists.iter().skip(1) {
                    if let Some(pi) = plist.iter().find(|p| p.doc_ord == p0.doc_ord) {
                        combined_tf = combined_tf.min(pi.tf);
                    }
                }
                let dl = self
                    .doc_lens
                    .get(&f)
                    .and_then(|m| m.get(&p0.doc_ord))
                    .copied()
                    .unwrap_or(1) as f64;
                // Phrase rarity: count docs that contain the phrase (approx via first term df).
                let df = first.len() as u64;
                let s = bm25(combined_tf as f64, df.max(1), n, dl, avg, self.k1, self.b);
                *scores.entry(p0.doc_ord).or_default() += s * 1.5;
            }
        }
        scores
    }

    /// Prefix completion over the term dictionary.
    pub fn suggest(&self, prefix: &str, field: Option<&str>, limit: usize) -> Vec<String> {
        let mut out = Vec::new();
        if prefix.is_empty() || limit == 0 {
            return out;
        }
        let lower: String = prefix.chars().flat_map(|c| c.to_lowercase()).collect();
        let mut seen = HashSet::new();
        for f in self.fields_for(field) {
            let Some(terms) = self.inverted.get(&f) else {
                continue;
            };
            for (term, _) in terms.range(lower.clone()..) {
                if !term.starts_with(&lower) {
                    break;
                }
                if seen.insert(term.clone()) {
                    out.push(term.clone());
                    if out.len() >= limit {
                        return out;
                    }
                }
            }
        }
        out.sort();
        out.truncate(limit);
        out
    }

    /// Facet counts, optionally restricted to documents matching `query`.
    pub fn facets(&self, facet_field: &str, query: Option<&str>, limit: usize) -> Vec<FacetCount> {
        let filter: Option<HashSet<u32>> = query.map(|q| {
            let q = parse(q);
            self.eval_query(&q, None).into_keys().collect()
        });
        let Some(by_val) = self.facet_index.get(facet_field) else {
            return Vec::new();
        };
        let mut counts: Vec<FacetCount> = by_val
            .iter()
            .map(|(value, ords)| {
                let count = match &filter {
                    Some(f) => ords.iter().filter(|o| f.contains(o)).count() as u64,
                    None => ords.len() as u64,
                };
                FacetCount {
                    value: value.clone(),
                    count,
                }
            })
            .filter(|c| c.count > 0)
            .collect();
        counts.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.value.cmp(&b.value)));
        if limit > 0 {
            counts.truncate(limit);
        }
        counts
    }

    pub fn save(&self, path: &str) -> FtsResult<()> {
        let mut facet_index: HashMap<String, HashMap<String, Vec<u32>>> = HashMap::new();
        for (ff, by_val) in &self.facet_index {
            let mut m = HashMap::new();
            for (v, set) in by_val {
                let mut ords: Vec<u32> = set.iter().copied().collect();
                ords.sort_unstable();
                m.insert(v.clone(), ords);
            }
            facet_index.insert(ff.clone(), m);
        }
        let snap = Snapshot {
            magic: MAGIC.to_string(),
            k1: self.k1,
            b: self.b,
            docs: self.docs.values().cloned().collect(),
            inverted: self.inverted.clone(),
            doc_lens: self.doc_lens.clone(),
            facet_index,
            field_names: self.field_names.iter().cloned().collect(),
            facet_names: self.facet_names.iter().cloned().collect(),
        };
        let json = serde_json::to_vec(&snap)?;
        if let Some(parent) = Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        fs::write(path, json)?;
        Ok(())
    }

    pub fn load(path: &str) -> FtsResult<Self> {
        let data = fs::read(path)?;
        let snap: Snapshot = serde_json::from_slice(&data)?;
        if snap.magic != MAGIC {
            return Err(FtsError::new(format!(
                "unsupported index format: {}",
                snap.magic
            )));
        }
        let mut idx = Self::new();
        idx.k1 = snap.k1;
        idx.b = snap.b;
        for doc in snap.docs {
            idx.index_doc(&doc.id, doc.fields, doc.facets);
        }
        Ok(idx)
    }
}

fn phrase_matches(lists: &[&[Posting]], doc_ord: u32) -> bool {
    let mut pos_sets: Vec<HashSet<u32>> = Vec::with_capacity(lists.len());
    for list in lists {
        let Some(p) = list.iter().find(|p| p.doc_ord == doc_ord) else {
            return false;
        };
        pos_sets.push(p.positions.iter().copied().collect());
    }
    for &start in &pos_sets[0] {
        let mut ok = true;
        for (offset, set) in pos_sets.iter().enumerate().skip(1) {
            if !set.contains(&(start + offset as u32)) {
                ok = false;
                break;
            }
        }
        if ok {
            return true;
        }
    }
    false
}

/// Public re-export helpers used by analyze().
pub fn analyze(text: &str) -> Vec<String> {
    tokenize(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn add_search_bm25() {
        let mut idx = Index::new();
        idx.add(
            "1",
            fields(&[("body", "the quick brown fox")]),
            HashMap::new(),
        )
        .unwrap();
        idx.add("2", fields(&[("body", "lazy brown dog")]), HashMap::new())
            .unwrap();
        let hits = idx.search("brown fox", 10, Some("body"));
        assert!(!hits.is_empty());
        assert_eq!(hits[0].id, "1");
    }

    #[test]
    fn phrase_query() {
        let mut idx = Index::new();
        idx.add("a", fields(&[("body", "new york city")]), HashMap::new())
            .unwrap();
        idx.add("b", fields(&[("body", "york new city")]), HashMap::new())
            .unwrap();
        let hits = idx.search(r#""new york""#, 10, Some("body"));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "a");
    }

    #[test]
    fn prefix_and_suggest() {
        let mut idx = Index::new();
        idx.add(
            "1",
            fields(&[("body", "catalog category cat")]),
            HashMap::new(),
        )
        .unwrap();
        let hits = idx.search("cat*", 10, Some("body"));
        assert_eq!(hits.len(), 1);
        let s = idx.suggest("cat", Some("body"), 10);
        assert!(s
            .iter()
            .any(|t| t == "catalog" || t == "category" || t == "cat"));
    }

    #[test]
    fn facets_filtered() {
        let mut idx = Index::new();
        idx.add(
            "1",
            fields(&[("body", "red apple")]),
            fields(&[("color", "red")]),
        )
        .unwrap();
        idx.add(
            "2",
            fields(&[("body", "green apple")]),
            fields(&[("color", "green")]),
        )
        .unwrap();
        idx.add(
            "3",
            fields(&[("body", "red car")]),
            fields(&[("color", "red")]),
        )
        .unwrap();
        let all = idx.facets("color", None, 10);
        assert_eq!(all.iter().find(|c| c.value == "red").unwrap().count, 2);
        let filtered = idx.facets("color", Some("apple"), 10);
        assert_eq!(filtered.iter().find(|c| c.value == "red").unwrap().count, 1);
        assert_eq!(
            filtered.iter().find(|c| c.value == "green").unwrap().count,
            1
        );
    }

    #[test]
    fn delete_and_duplicate_add() {
        let mut idx = Index::new();
        idx.add("x", fields(&[("body", "hello")]), HashMap::new())
            .unwrap();
        assert!(idx
            .add("x", fields(&[("body", "world")]), HashMap::new())
            .is_err());
        assert!(idx.delete("x"));
        assert_eq!(idx.count(), 0);
        idx.add("x", fields(&[("body", "world")]), HashMap::new())
            .unwrap();
        assert_eq!(idx.search("world", 5, None).len(), 1);
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("nfts_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("idx.nfts");
        let mut idx = Index::new();
        idx.update(
            "d1",
            fields(&[("title", "Rust book"), ("body", "systems programming")]),
            fields(&[("lang", "en")]),
        );
        idx.save(path.to_str().unwrap()).unwrap();
        let loaded = Index::load(path.to_str().unwrap()).unwrap();
        assert_eq!(loaded.count(), 1);
        let hits = loaded.search("systems", 5, None);
        assert_eq!(hits[0].id, "d1");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_query() {
        let idx = Index::new();
        assert!(idx.search("", 10, None).is_empty());
    }

    #[test]
    fn boolean_or_not() {
        let mut idx = Index::new();
        idx.add("1", fields(&[("body", "cats meow")]), HashMap::new())
            .unwrap();
        idx.add("2", fields(&[("body", "dogs bark")]), HashMap::new())
            .unwrap();
        idx.add("3", fields(&[("body", "cats and dogs")]), HashMap::new())
            .unwrap();
        let or_hits = idx.search("cats OR dogs", 10, Some("body"));
        assert_eq!(or_hits.len(), 3);
        let not_hits = idx.search("cats NOT dogs", 10, Some("body"));
        assert_eq!(not_hits.len(), 1);
        assert_eq!(not_hits[0].id, "1");
    }
}
