//! In-memory vector index with brute-force cosine similarity and an optional
//! Navigable Small World (NSW / HNSW-lite) graph for faster approximate search.
//!
//! Strategy:
//! - N ≤ `HNSW_THRESHOLD`: flat brute-force cosine (exact, O(N·D)).
//! - N  > `HNSW_THRESHOLD`: NSW graph search (approximate, sub-linear).
//!
//! All operations are std-only and allocation-minimal on the hot path.

use std::collections::{BinaryHeap, HashMap, HashSet};
use std::cmp::Ordering;
use std::io::Write;

// ---------------------------------------------------------------------------
// Thresholds
// ---------------------------------------------------------------------------

/// Below this count, brute-force is used; above, the NSW graph is used.
const HNSW_THRESHOLD: usize = 256;
/// Max bidirectional neighbors per NSW node.
const NSW_M: usize = 16;
/// Candidate list size during NSW graph construction.
const NSW_EF_CONSTRUCTION: usize = 64;
/// Candidate list size during NSW graph search.
const NSW_EF_SEARCH: usize = 64;

// ---------------------------------------------------------------------------
// Serialization magic
// ---------------------------------------------------------------------------

const MAGIC: &[u8; 4] = b"NVEC";
const FORMAT_VERSION: u8 = 1;

// ---------------------------------------------------------------------------
// Metadata value (fully serializable without serde)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum MetaVal {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

impl MetaVal {
    fn type_byte(&self) -> u8 {
        match self {
            MetaVal::Nil => 0,
            MetaVal::Bool(_) => 1,
            MetaVal::Int(_) => 2,
            MetaVal::Float(_) => 3,
            MetaVal::Str(_) => 4,
        }
    }
}

// ---------------------------------------------------------------------------
// VecEntry
// ---------------------------------------------------------------------------

pub struct VecEntry {
    pub id: String,
    pub vector: Vec<f32>,
    /// `1.0 / ‖vector‖`, precomputed at insert for fast cosine scoring.
    inv_norm: f32,
    pub metadata: HashMap<String, MetaVal>,
}

// ---------------------------------------------------------------------------
// SearchHit
// ---------------------------------------------------------------------------

pub struct SearchHit {
    pub id: String,
    pub score: f32,
    pub metadata: HashMap<String, MetaVal>,
}

// ---------------------------------------------------------------------------
// Dot / cosine helpers (SIMD-friendly hot path)
// ---------------------------------------------------------------------------

const DOT_UNROLL: usize = 8;

/// 8-wide chunks with four independent accumulators to break reduction chains.
#[inline]
pub fn dot_f32(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    let len = a.len();
    let mut acc0 = 0.0f32;
    let mut acc1 = 0.0f32;
    let mut acc2 = 0.0f32;
    let mut acc3 = 0.0f32;
    let mut i = 0;
    while i + DOT_UNROLL * 4 <= len {
        let b0 = i;
        let b1 = i + DOT_UNROLL;
        let b2 = i + DOT_UNROLL * 2;
        let b3 = i + DOT_UNROLL * 3;
        let [a0, a1, a2, a3, a4, a5, a6, a7] = [
            a[b0], a[b0 + 1], a[b0 + 2], a[b0 + 3], a[b0 + 4], a[b0 + 5], a[b0 + 6], a[b0 + 7],
        ];
        let [c0, c1, c2, c3, c4, c5, c6, c7] = [
            b[b0], b[b0 + 1], b[b0 + 2], b[b0 + 3], b[b0 + 4], b[b0 + 5], b[b0 + 6], b[b0 + 7],
        ];
        acc0 += a0 * c0 + a1 * c1 + a2 * c2 + a3 * c3 + a4 * c4 + a5 * c5 + a6 * c6 + a7 * c7;

        let [a0, a1, a2, a3, a4, a5, a6, a7] = [
            a[b1], a[b1 + 1], a[b1 + 2], a[b1 + 3], a[b1 + 4], a[b1 + 5], a[b1 + 6], a[b1 + 7],
        ];
        let [c0, c1, c2, c3, c4, c5, c6, c7] = [
            b[b1], b[b1 + 1], b[b1 + 2], b[b1 + 3], b[b1 + 4], b[b1 + 5], b[b1 + 6], b[b1 + 7],
        ];
        acc1 += a0 * c0 + a1 * c1 + a2 * c2 + a3 * c3 + a4 * c4 + a5 * c5 + a6 * c6 + a7 * c7;

        let [a0, a1, a2, a3, a4, a5, a6, a7] = [
            a[b2], a[b2 + 1], a[b2 + 2], a[b2 + 3], a[b2 + 4], a[b2 + 5], a[b2 + 6], a[b2 + 7],
        ];
        let [c0, c1, c2, c3, c4, c5, c6, c7] = [
            b[b2], b[b2 + 1], b[b2 + 2], b[b2 + 3], b[b2 + 4], b[b2 + 5], b[b2 + 6], b[b2 + 7],
        ];
        acc2 += a0 * c0 + a1 * c1 + a2 * c2 + a3 * c3 + a4 * c4 + a5 * c5 + a6 * c6 + a7 * c7;

        let [a0, a1, a2, a3, a4, a5, a6, a7] = [
            a[b3], a[b3 + 1], a[b3 + 2], a[b3 + 3], a[b3 + 4], a[b3 + 5], a[b3 + 6], a[b3 + 7],
        ];
        let [c0, c1, c2, c3, c4, c5, c6, c7] = [
            b[b3], b[b3 + 1], b[b3 + 2], b[b3 + 3], b[b3 + 4], b[b3 + 5], b[b3 + 6], b[b3 + 7],
        ];
        acc3 += a0 * c0 + a1 * c1 + a2 * c2 + a3 * c3 + a4 * c4 + a5 * c5 + a6 * c6 + a7 * c7;

        i += DOT_UNROLL * 4;
    }
    while i + DOT_UNROLL <= len {
        let [a0, a1, a2, a3, a4, a5, a6, a7] = [
            a[i], a[i + 1], a[i + 2], a[i + 3], a[i + 4], a[i + 5], a[i + 6], a[i + 7],
        ];
        let [b0, b1, b2, b3, b4, b5, b6, b7] = [
            b[i], b[i + 1], b[i + 2], b[i + 3], b[i + 4], b[i + 5], b[i + 6], b[i + 7],
        ];
        acc0 += a0 * b0 + a1 * b1 + a2 * b2 + a3 * b3 + a4 * b4 + a5 * b5 + a6 * b6 + a7 * b7;
        i += DOT_UNROLL;
    }
    while i < len {
        acc0 += a[i] * b[i];
        i += 1;
    }
    acc0 + acc1 + acc2 + acc3
}

#[inline]
fn compute_inv_norm(vector: &[f32]) -> f32 {
    let sum_sq = dot_f32(vector, vector);
    let norm = sum_sq.sqrt();
    if norm < 1e-10 { 0.0 } else { 1.0 / norm }
}

/// Per-search query context: query inverse-norm computed once.
struct QueryCtx<'a> {
    query: &'a [f32],
    q_inv_norm: f32,
}

impl<'a> QueryCtx<'a> {
    #[inline]
    fn new(query: &'a [f32]) -> Self {
        QueryCtx {
            query,
            q_inv_norm: compute_inv_norm(query),
        }
    }

    #[inline]
    fn score(&self, entry: &VecEntry) -> f32 {
        dot_f32(self.query, &entry.vector) * self.q_inv_norm * entry.inv_norm
    }
}

/// Cosine similarity for arbitrary slices (no precomputed norms).
#[inline]
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let a_inv = compute_inv_norm(a);
    let b_inv = compute_inv_norm(b);
    if a_inv == 0.0 || b_inv == 0.0 {
        0.0
    } else {
        dot_f32(a, b) * a_inv * b_inv
    }
}

// ---------------------------------------------------------------------------
// Ordered-float wrapper (std-only BinaryHeap support)
// ---------------------------------------------------------------------------

#[derive(PartialEq)]
struct OrdF32(f32);

impl Eq for OrdF32 {}

impl PartialOrd for OrdF32 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrdF32 {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.partial_cmp(&other.0).unwrap_or(Ordering::Equal)
    }
}

// Heap entry: (score, node_index). Max-heap by score.
#[derive(PartialEq, Eq)]
struct Candidate {
    score: OrdF32,
    idx: usize,
}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        self.score.cmp(&other.score)
    }
}

// ---------------------------------------------------------------------------
// NSW graph (HNSW-lite, single-layer)
// ---------------------------------------------------------------------------

struct NswGraph {
    /// `neighbors[i]` = indices into `VecIndex::entries` of node i's neighbors.
    neighbors: Vec<Vec<usize>>,
    entry: Option<usize>,
}

impl NswGraph {
    fn new() -> Self {
        NswGraph { neighbors: Vec::new(), entry: None }
    }

    /// Called after a new entry is appended to `entries` at `node_idx`.
    fn add_node(&mut self, node_idx: usize, entries: &[VecEntry], query: &[f32]) {
        while self.neighbors.len() <= node_idx {
            self.neighbors.push(Vec::new());
        }
        let qctx = QueryCtx::new(query);
        if let Some(ep) = self.entry {
            let mut cands = self.beam_search(ep, &qctx, NSW_EF_CONSTRUCTION, entries);
            cands.truncate(NSW_M);
            for (nbr, _score) in cands {
                if nbr >= self.neighbors.len() {
                    continue;
                }
                self.neighbors[node_idx].push(nbr);
                if nbr != node_idx
                    && !self.neighbors[nbr].contains(&node_idx)
                    && self.neighbors[nbr].len() < NSW_M
                {
                    self.neighbors[nbr].push(node_idx);
                }
            }
        }
        // Update entry to point to the most recently added node.
        self.entry = Some(node_idx);
    }

    /// Greedy beam search starting from `start`, returning top-`ef` (idx, score) pairs.
    fn beam_search(
        &self,
        start: usize,
        qctx: &QueryCtx<'_>,
        ef: usize,
        entries: &[VecEntry],
    ) -> Vec<(usize, f32)> {
        // Max-heap for best candidates discovered so far.
        let mut best: BinaryHeap<Candidate> = BinaryHeap::new();
        // Min-heap for frontier (lowest-score node expanded next) — we invert via Reverse.
        let mut frontier: BinaryHeap<std::cmp::Reverse<Candidate>> = BinaryHeap::new();
        let mut visited: HashSet<usize> = HashSet::new();

        if start >= entries.len() || entries[start].vector.is_empty() {
            return Vec::new();
        }
        let s_score = qctx.score(&entries[start]);
        visited.insert(start);
        best.push(Candidate { score: OrdF32(s_score), idx: start });
        frontier.push(std::cmp::Reverse(Candidate { score: OrdF32(s_score), idx: start }));

        while let Some(std::cmp::Reverse(curr)) = frontier.pop() {
            // If the worst of our best is already better than current, stop.
            if let Some(worst_best) = best.peek() {
                if best.len() >= ef && curr.score < worst_best.score {
                    break;
                }
            }
            if curr.idx >= self.neighbors.len() {
                continue;
            }
            for &nbr in &self.neighbors[curr.idx] {
                if visited.contains(&nbr) {
                    continue;
                }
                visited.insert(nbr);
                if nbr >= entries.len() || entries[nbr].vector.is_empty() {
                    continue;
                }
                let sc = qctx.score(&entries[nbr]);
                let cand = Candidate { score: OrdF32(sc), idx: nbr };
                if best.len() < ef {
                    frontier.push(std::cmp::Reverse(Candidate { score: OrdF32(sc), idx: nbr }));
                    best.push(cand);
                } else if let Some(worst) = best.peek() {
                    if OrdF32(sc) > worst.score {
                        frontier.push(std::cmp::Reverse(Candidate { score: OrdF32(sc), idx: nbr }));
                        best.push(cand);
                        if best.len() > ef {
                            best.pop();
                        }
                    }
                }
            }
        }

        let mut result: Vec<(usize, f32)> = best.into_iter().map(|c| (c.idx, c.score.0)).collect();
        result.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
        result
    }

    /// Remove all neighbor references pointing to `removed_idx`.
    fn purge_node(&mut self, removed_idx: usize) {
        for nbrs in &mut self.neighbors {
            nbrs.retain(|&n| n != removed_idx);
        }
        if self.neighbors.get(removed_idx).is_some() {
            self.neighbors[removed_idx].clear();
        }
        // Reset entry if it was the removed node.
        if self.entry == Some(removed_idx) {
            self.entry = (0..self.neighbors.len())
                .find(|&i| i != removed_idx && !self.neighbors[i].is_empty());
        }
    }
}

// ---------------------------------------------------------------------------
// VecIndex — public API
// ---------------------------------------------------------------------------

pub struct VecIndex {
    /// 0 = dimension not yet fixed; set on first insert.
    pub dim: usize,
    pub entries: Vec<VecEntry>,
    /// vec_id → position in `entries`. Positions for deleted entries are set to
    /// `usize::MAX` (tombstone; never compacted to keep NSW indices stable).
    id_to_pos: HashMap<String, usize>,
    graph: NswGraph,
    live_count: usize,
}

impl VecIndex {
    pub fn new(dim: usize) -> Self {
        VecIndex {
            dim,
            entries: Vec::new(),
            id_to_pos: HashMap::new(),
            graph: NswGraph::new(),
            live_count: 0,
        }
    }

    pub fn count(&self) -> usize {
        self.live_count
    }

    /// Insert a new vector. Returns `Err` if `vec_id` already exists.
    pub fn insert(
        &mut self,
        id: String,
        vector: Vec<f32>,
        metadata: HashMap<String, MetaVal>,
    ) -> Result<(), String> {
        if self.id_to_pos.contains_key(&id) {
            return Err(format!("vector id '{id}' already exists; use upsert"));
        }
        self.do_insert(id, vector, metadata)
    }

    /// Insert or replace.
    pub fn upsert(
        &mut self,
        id: String,
        vector: Vec<f32>,
        metadata: HashMap<String, MetaVal>,
    ) -> Result<(), String> {
        if self.id_to_pos.contains_key(&id) {
            self.delete(&id);
        }
        self.do_insert(id, vector, metadata)
    }

    fn do_insert(
        &mut self,
        id: String,
        vector: Vec<f32>,
        metadata: HashMap<String, MetaVal>,
    ) -> Result<(), String> {
        // Validate / fix dimension.
        if self.dim == 0 {
            if vector.is_empty() {
                return Err("vector must not be empty".to_string());
            }
            self.dim = vector.len();
        } else if vector.len() != self.dim {
            return Err(format!(
                "dimension mismatch: index expects {}, got {}",
                self.dim,
                vector.len()
            ));
        }

        let idx = self.entries.len();
        let inv_norm = compute_inv_norm(&vector);

        self.entries.push(VecEntry { id: id.clone(), vector, inv_norm, metadata });
        self.id_to_pos.insert(id, idx);
        self.live_count += 1;

        if self.live_count > HNSW_THRESHOLD {
            let query = &self.entries[idx].vector;
            self.graph.add_node(idx, &self.entries, query);
        }

        Ok(())
    }

    /// Delete by vec_id. Returns `false` if not found.
    pub fn delete(&mut self, id: &str) -> bool {
        match self.id_to_pos.get(id).copied() {
            None => false,
            Some(pos) => {
                self.graph.purge_node(pos);
                // Tombstone the entry in-place (keeps NSW indices stable).
                self.entries[pos].id = String::new();
                self.entries[pos].vector.clear();
                self.entries[pos].inv_norm = 0.0;
                self.entries[pos].metadata.clear();
                self.id_to_pos.remove(id);
                self.live_count = self.live_count.saturating_sub(1);
                true
            }
        }
    }

    /// Search for top-k nearest vectors to `query`. Returns hits sorted by
    /// descending cosine similarity, filtered to `threshold ≤ score`.
    pub fn search(
        &self,
        query: &[f32],
        top_k: usize,
        threshold: f32,
    ) -> Result<Vec<SearchHit>, String> {
        if query.len() != self.dim && self.dim != 0 {
            return Err(format!(
                "query dimension mismatch: index expects {}, got {}",
                self.dim,
                query.len()
            ));
        }
        if self.live_count == 0 {
            return Ok(Vec::new());
        }

        let hits = if self.live_count <= HNSW_THRESHOLD || self.graph.entry.is_none() {
            self.brute_force(query, top_k, threshold)
        } else {
            self.nsw_search(query, top_k, threshold)
        };
        Ok(hits)
    }

    fn brute_force(&self, query: &[f32], top_k: usize, threshold: f32) -> Vec<SearchHit> {
        let qctx = QueryCtx::new(query);
        let mut scored: Vec<(f32, usize)> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| !e.vector.is_empty())
            .map(|(i, e)| (qctx.score(e), i))
            .filter(|(score, _)| *score >= threshold)
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal));
        scored.truncate(top_k);
        scored
            .into_iter()
            .map(|(score, i)| {
                let e = &self.entries[i];
                SearchHit {
                    id: e.id.clone(),
                    score,
                    metadata: e.metadata.clone(),
                }
            })
            .collect()
    }

    fn nsw_search(&self, query: &[f32], top_k: usize, threshold: f32) -> Vec<SearchHit> {
        let ep = match self.graph.entry {
            Some(ep) => ep,
            None => return self.brute_force(query, top_k, threshold),
        };

        let qctx = QueryCtx::new(query);
        let ef = NSW_EF_SEARCH.max(top_k * 2);
        let mut candidates = self.graph.beam_search(ep, &qctx, ef, &self.entries);
        candidates.retain(|(_, score)| *score >= threshold);
        candidates.truncate(top_k);
        candidates
            .into_iter()
            .filter_map(|(idx, score)| {
                let e = self.entries.get(idx)?;
                if e.vector.is_empty() {
                    return None; // tombstone
                }
                Some(SearchHit {
                    id: e.id.clone(),
                    score,
                    metadata: e.metadata.clone(),
                })
            })
            .collect()
    }

    // -----------------------------------------------------------------------
    // Persistence (custom binary format, std::io only)
    // -----------------------------------------------------------------------

    pub fn save_to_file(&self, path: &str) -> Result<(), String> {
        let mut buf: Vec<u8> = Vec::new();
        self.encode(&mut buf).map_err(|e| e.to_string())?;
        std::fs::write(path, &buf).map_err(|e| format!("write '{path}': {e}"))?;
        Ok(())
    }

    pub fn load_from_file(path: &str) -> Result<Self, String> {
        let data = std::fs::read(path).map_err(|e| format!("read '{path}': {e}"))?;
        Self::decode(&data)
    }

    fn encode(&self, buf: &mut Vec<u8>) -> std::io::Result<()> {
        buf.write_all(MAGIC)?;
        buf.write_all(&[FORMAT_VERSION])?;
        buf.write_all(&(self.dim as u32).to_le_bytes())?;
        let live: Vec<&VecEntry> = self.entries.iter().filter(|e| !e.vector.is_empty()).collect();
        buf.write_all(&(live.len() as u32).to_le_bytes())?;
        for entry in live {
            write_str(buf, &entry.id)?;
            for &f in &entry.vector {
                buf.write_all(&f.to_le_bytes())?;
            }
            buf.write_all(&(entry.metadata.len() as u32).to_le_bytes())?;
            for (k, v) in &entry.metadata {
                write_str(buf, k)?;
                buf.write_all(&[v.type_byte()])?;
                match v {
                    MetaVal::Nil => {}
                    MetaVal::Bool(b) => buf.write_all(&[*b as u8])?,
                    MetaVal::Int(n) => buf.write_all(&n.to_le_bytes())?,
                    MetaVal::Float(f) => buf.write_all(&f.to_le_bytes())?,
                    MetaVal::Str(s) => write_str(buf, s)?,
                }
            }
        }
        Ok(())
    }

    fn decode(data: &[u8]) -> Result<Self, String> {
        let mut r = Cursor::new(data);
        let mut magic = [0u8; 4];
        r.read_exact(&mut magic).map_err(|e| e.to_string())?;
        if &magic != MAGIC {
            return Err("invalid NVEC file magic".to_string());
        }
        let ver = r.read_u8()?;
        if ver != FORMAT_VERSION {
            return Err(format!("unsupported NVEC format version {ver}"));
        }
        let dim = r.read_u32()? as usize;
        let count = r.read_u32()? as usize;
        let mut index = VecIndex::new(dim);
        for _ in 0..count {
            let id = r.read_str()?;
            let mut vector = vec![0.0f32; dim];
            for f in vector.iter_mut() {
                *f = r.read_f32()?;
            }
            let meta_count = r.read_u32()? as usize;
            let mut metadata = HashMap::new();
            for _ in 0..meta_count {
                let key = r.read_str()?;
                let type_byte = r.read_u8()?;
                let val = match type_byte {
                    0 => MetaVal::Nil,
                    1 => MetaVal::Bool(r.read_u8()? != 0),
                    2 => MetaVal::Int(r.read_i64()?),
                    3 => MetaVal::Float(r.read_f64()?),
                    4 => MetaVal::Str(r.read_str()?),
                    t => return Err(format!("unknown metadata type byte {t}")),
                };
                metadata.insert(key, val);
            }
            index
                .upsert(id, vector, metadata)
                .map_err(|e| e.to_string())?;
        }
        Ok(index)
    }
}

// ---------------------------------------------------------------------------
// Cursor helper for decoding
// ---------------------------------------------------------------------------

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Cursor { data, pos: 0 }
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), String> {
        let end = self.pos + buf.len();
        if end > self.data.len() {
            return Err("unexpected end of NVEC data".to_string());
        }
        buf.copy_from_slice(&self.data[self.pos..end]);
        self.pos = end;
        Ok(())
    }

    fn read_u8(&mut self) -> Result<u8, String> {
        let mut b = [0u8];
        self.read_exact(&mut b)?;
        Ok(b[0])
    }

    fn read_u32(&mut self) -> Result<u32, String> {
        let mut b = [0u8; 4];
        self.read_exact(&mut b)?;
        Ok(u32::from_le_bytes(b))
    }

    fn read_i64(&mut self) -> Result<i64, String> {
        let mut b = [0u8; 8];
        self.read_exact(&mut b)?;
        Ok(i64::from_le_bytes(b))
    }

    fn read_f32(&mut self) -> Result<f32, String> {
        let mut b = [0u8; 4];
        self.read_exact(&mut b)?;
        Ok(f32::from_le_bytes(b))
    }

    fn read_f64(&mut self) -> Result<f64, String> {
        let mut b = [0u8; 8];
        self.read_exact(&mut b)?;
        Ok(f64::from_le_bytes(b))
    }

    fn read_str(&mut self) -> Result<String, String> {
        let len = self.read_u32()? as usize;
        let end = self.pos + len;
        if end > self.data.len() {
            return Err("unexpected end of NVEC string data".to_string());
        }
        let s = std::str::from_utf8(&self.data[self.pos..end])
            .map_err(|e| format!("invalid UTF-8 in NVEC string: {e}"))?
            .to_string();
        self.pos = end;
        Ok(s)
    }
}

fn write_str(buf: &mut Vec<u8>, s: &str) -> std::io::Result<()> {
    buf.write_all(&(s.len() as u32).to_le_bytes())?;
    buf.write_all(s.as_bytes())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(pairs: &[(&str, MetaVal)]) -> HashMap<String, MetaVal> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    #[test]
    fn cosine_identical_vectors() {
        let v = vec![1.0f32, 2.0, 3.0];
        assert!((cosine(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_orthogonal() {
        let a = vec![1.0f32, 0.0];
        let b = vec![0.0f32, 1.0];
        assert!(cosine(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn insert_and_search_basic() {
        let mut idx = VecIndex::new(3);
        idx.insert("a".into(), vec![1.0, 0.0, 0.0], HashMap::new()).unwrap();
        idx.insert("b".into(), vec![0.0, 1.0, 0.0], HashMap::new()).unwrap();
        idx.insert("c".into(), vec![0.9, 0.1, 0.0], HashMap::new()).unwrap();
        let hits = idx.search(&[1.0, 0.0, 0.0], 2, 0.0).unwrap();
        assert_eq!(hits[0].id, "a");
        assert!(hits[0].score > 0.99);
        assert_eq!(hits[1].id, "c");
    }

    #[test]
    fn duplicate_insert_fails() {
        let mut idx = VecIndex::new(2);
        idx.insert("x".into(), vec![1.0, 0.0], HashMap::new()).unwrap();
        assert!(idx.insert("x".into(), vec![0.0, 1.0], HashMap::new()).is_err());
    }

    #[test]
    fn upsert_replaces() {
        let mut idx = VecIndex::new(2);
        idx.upsert("x".into(), vec![1.0, 0.0], HashMap::new()).unwrap();
        idx.upsert("x".into(), vec![0.0, 1.0], HashMap::new()).unwrap();
        assert_eq!(idx.count(), 1);
        let hits = idx.search(&[0.0, 1.0], 1, 0.0).unwrap();
        assert_eq!(hits[0].id, "x");
        assert!(hits[0].score > 0.99);
    }

    #[test]
    fn delete_removes_entry() {
        let mut idx = VecIndex::new(2);
        idx.insert("a".into(), vec![1.0, 0.0], HashMap::new()).unwrap();
        idx.insert("b".into(), vec![0.0, 1.0], HashMap::new()).unwrap();
        assert!(idx.delete("a"));
        assert_eq!(idx.count(), 1);
        let hits = idx.search(&[1.0, 0.0], 5, 0.0).unwrap();
        assert!(!hits.iter().any(|h| h.id == "a"));
    }

    #[test]
    fn threshold_filters() {
        let mut idx = VecIndex::new(2);
        idx.insert("a".into(), vec![1.0, 0.0], HashMap::new()).unwrap();
        idx.insert("b".into(), vec![0.0, 1.0], HashMap::new()).unwrap();
        let hits = idx.search(&[1.0, 0.0], 5, 0.9).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "a");
    }

    #[test]
    fn dimension_mismatch_error() {
        let mut idx = VecIndex::new(3);
        assert!(idx.insert("a".into(), vec![1.0, 0.0], HashMap::new()).is_err());
    }

    #[test]
    fn auto_detect_dimension() {
        let mut idx = VecIndex::new(0);
        idx.insert("a".into(), vec![1.0, 2.0], HashMap::new()).unwrap();
        assert_eq!(idx.dim, 2);
    }

    #[test]
    fn save_load_roundtrip() {
        let tmp = std::env::temp_dir().join("nvec_test_roundtrip.nvecd");
        let path = tmp.to_str().unwrap();
        let mut idx = VecIndex::new(3);
        idx.insert("a".into(), vec![1.0, 2.0, 3.0],
            meta(&[("tag", MetaVal::Str("hello".into())), ("n", MetaVal::Int(7))])).unwrap();
        idx.insert("b".into(), vec![4.0, 5.0, 6.0], HashMap::new()).unwrap();
        idx.save_to_file(path).unwrap();
        let idx2 = VecIndex::load_from_file(path).unwrap();
        assert_eq!(idx2.count(), 2);
        let hits = idx2.search(&[1.0, 2.0, 3.0], 1, 0.0).unwrap();
        assert_eq!(hits[0].id, "a");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn metadata_preserved_in_hits() {
        let mut idx = VecIndex::new(2);
        idx.insert("a".into(), vec![1.0, 0.0],
            meta(&[("label", MetaVal::Str("cat".into())), ("score", MetaVal::Float(0.9))])).unwrap();
        let hits = idx.search(&[1.0, 0.0], 1, 0.0).unwrap();
        assert_eq!(hits.len(), 1);
        match hits[0].metadata.get("label") {
            Some(MetaVal::Str(s)) => assert_eq!(s, "cat"),
            _ => panic!("missing label"),
        }
    }

    #[test]
    fn nsw_search_large_index() {
        // Build an index large enough to trigger NSW path.
        let dim = 8;
        let mut idx = VecIndex::new(dim);
        for i in 0..(HNSW_THRESHOLD + 50) {
            let mut v = vec![0.0f32; dim];
            v[i % dim] = 1.0;
            v[(i + 1) % dim] = 0.5;
            idx.upsert(format!("v{i}"), v, HashMap::new()).unwrap();
        }
        let mut query = vec![0.0f32; dim];
        query[0] = 1.0;
        // NSW search should return without panic.
        let hits = idx.search(&query, 5, 0.0).unwrap();
        assert!(!hits.is_empty());
        // Best hit should have high cosine.
        assert!(hits[0].score > 0.5);
    }
}
