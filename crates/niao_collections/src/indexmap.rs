//! Insertion-ordered hash map / set.
//!
//! Layout: dense `entries` Vec (stable iteration order) + robin-hood hash table
//! of entry indices. O(1) average get/insert; `swap_remove` / `shift_remove`.

use core::borrow::Borrow;
use core::fmt;
use core::hash::{BuildHasher, Hash, Hasher};
use core::iter::FusedIterator;
use core::mem;
use std::collections::hash_map::RandomState;

/// Sentinel for an empty hash slot.
const EMPTY: u32 = u32::MAX;
/// Minimum number of hash slots (power of two).
const MIN_INDICES: usize = 8;
/// Grow when `len * 8/7 > capacity` (~87.5% load — robin-hood friendly).
const LOAD_NUM: usize = 7;
const LOAD_DEN: usize = 8;

/// Fast non-cryptographic hasher (Fx-style). Kept in this module so the
/// `ahash` agent can own a public hasher elsewhere.
#[derive(Clone, Default)]
pub struct FxHasher {
    hash: u64,
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let mut i = 0;
        while i + 8 <= bytes.len() {
            let chunk = u64::from_ne_bytes(bytes[i..i + 8].try_into().unwrap());
            self.hash = self
                .hash
                .wrapping_add(chunk)
                .wrapping_mul(0x517cc1b727220a95);
            i += 8;
        }
        if i < bytes.len() {
            let mut tail = [0u8; 8];
            tail[..bytes.len() - i].copy_from_slice(&bytes[i..]);
            let chunk = u64::from_ne_bytes(tail);
            self.hash = self
                .hash
                .wrapping_add(chunk)
                .wrapping_mul(0x517cc1b727220a95);
        }
    }

    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.hash = self
            .hash
            .wrapping_add(u64::from(i))
            .wrapping_mul(0x517cc1b727220a95);
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.hash = self.hash.wrapping_add(i).wrapping_mul(0x517cc1b727220a95);
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.write_u64(i as u64);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }
}

/// BuildHasher that produces [`FxHasher`].
#[derive(Clone, Default, Debug)]
pub struct FxBuildHasher;

impl BuildHasher for FxBuildHasher {
    type Hasher = FxHasher;

    #[inline]
    fn build_hasher(&self) -> FxHasher {
        FxHasher { hash: 0 }
    }
}

#[derive(Clone)]
struct Entry<K, V> {
    hash: u64,
    key: K,
    value: V,
}

/// Insertion-ordered map: hash table of indices + dense entry storage.
#[derive(Clone)]
pub struct IndexMap<K, V, S = RandomState> {
    entries: Vec<Entry<K, V>>,
    /// Parallel to `indices`: probe distance for robin-hood (0 = empty).
    /// Stored densely so the hot path stays cache-friendly.
    indices: Vec<u32>,
    dibs: Vec<u16>,
    hash_builder: S,
}

impl<K, V> IndexMap<K, V, RandomState> {
    #[inline]
    pub fn new() -> Self {
        Self::with_hasher(RandomState::new())
    }

    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_and_hasher(capacity, RandomState::new())
    }
}

impl<K, V> IndexMap<K, V, FxBuildHasher> {
    /// Map using the in-crate Fx hasher (fast, deterministic).
    #[inline]
    pub fn with_fx() -> Self {
        Self::with_hasher(FxBuildHasher)
    }

    #[inline]
    pub fn with_capacity_fx(capacity: usize) -> Self {
        Self::with_capacity_and_hasher(capacity, FxBuildHasher)
    }
}

impl<K, V, S> IndexMap<K, V, S> {
    #[inline]
    pub fn with_hasher(hash_builder: S) -> Self {
        Self {
            entries: Vec::new(),
            indices: Vec::new(),
            dibs: Vec::new(),
            hash_builder,
        }
    }

    #[inline]
    pub fn with_capacity_and_hasher(capacity: usize, hash_builder: S) -> Self {
        let mut map = Self::with_hasher(hash_builder);
        if capacity > 0 {
            map.reserve_entries(capacity);
            map.rehash_to(indices_for(capacity));
        }
        map
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.entries.capacity()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[inline]
    pub fn clear(&mut self) {
        self.entries.clear();
        for s in &mut self.indices {
            *s = EMPTY;
        }
        for d in &mut self.dibs {
            *d = 0;
        }
    }

    #[inline]
    pub fn hasher(&self) -> &S {
        &self.hash_builder
    }

    #[inline]
    fn reserve_entries(&mut self, additional: usize) {
        self.entries.reserve(additional);
    }

    fn rehash_to(&mut self, new_len: usize) {
        debug_assert!(new_len.is_power_of_two());
        self.indices.clear();
        self.indices.resize(new_len, EMPTY);
        self.dibs.clear();
        self.dibs.resize(new_len, 0);
        let mask = new_len - 1;
        for (idx, entry) in self.entries.iter().enumerate() {
            insert_index(
                &mut self.indices,
                &mut self.dibs,
                mask,
                entry.hash,
                idx as u32,
            );
        }
    }

    #[inline]
    pub fn iter(&self) -> Iter<'_, K, V> {
        Iter {
            inner: self.entries.iter(),
        }
    }

    #[inline]
    pub fn iter_mut(&mut self) -> IterMut<'_, K, V> {
        IterMut {
            inner: self.entries.iter_mut(),
        }
    }

    #[inline]
    pub fn keys(&self) -> Keys<'_, K, V> {
        Keys { inner: self.iter() }
    }

    #[inline]
    pub fn values(&self) -> Values<'_, K, V> {
        Values { inner: self.iter() }
    }

    #[inline]
    pub fn get_index(&self, index: usize) -> Option<(&K, &V)> {
        self.entries.get(index).map(|e| (&e.key, &e.value))
    }
}

impl<K, V, S> IndexMap<K, V, S>
where
    K: Hash + Eq,
    S: BuildHasher,
{
    #[inline]
    fn hash_key<Q: ?Sized + Hash>(&self, key: &Q) -> u64 {
        let mut h = self.hash_builder.build_hasher();
        key.hash(&mut h);
        h.finish()
    }

    /// Ensure indices table can hold `self.len() + additional` entries.
    fn reserve(&mut self, additional: usize) {
        let needed = self.entries.len().saturating_add(additional);
        self.reserve_entries(additional);
        if self.indices.is_empty()
            || needed.saturating_mul(LOAD_DEN) > self.indices.len().saturating_mul(LOAD_NUM)
        {
            let n = indices_for(needed.max(1));
            self.rehash_to(n.max(self.indices.len() * 2).max(MIN_INDICES));
        }
    }

    #[inline]
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.reserve(1);
        let hash = self.hash_key(&key);
        if let Some(i) = self.find_index(hash, &key) {
            return Some(mem::replace(&mut self.entries[i].value, value));
        }
        let idx = self.entries.len() as u32;
        self.entries.push(Entry { hash, key, value });
        let mask = self.indices.len() - 1;
        insert_index(&mut self.indices, &mut self.dibs, mask, hash, idx);
        None
    }

    #[inline]
    pub fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let hash = self.hash_key(key);
        self.find_index(hash, key).map(|i| &self.entries[i].value)
    }

    #[inline]
    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let hash = self.hash_key(key);
        self.find_index(hash, key)
            .map(|i| &mut self.entries[i].value)
    }

    #[inline]
    pub fn get_key_value<Q>(&self, key: &Q) -> Option<(&K, &V)>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let hash = self.hash_key(key);
        self.find_index(hash, key)
            .map(|i| (&self.entries[i].key, &self.entries[i].value))
    }

    #[inline]
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let hash = self.hash_key(key);
        self.find_index(hash, key).is_some()
    }

    /// Remove by swapping with the last entry (does not preserve relative order
    /// of remaining keys beyond “last moves into the hole”).
    pub fn swap_remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let hash = self.hash_key(key);
        let i = self.find_index(hash, key)?;
        Some(self.swap_remove_index(i).value)
    }

    /// Remove and shift subsequent entries forward (preserves insertion order).
    pub fn shift_remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let hash = self.hash_key(key);
        let i = self.find_index(hash, key)?;
        Some(self.shift_remove_index(i).value)
    }

    fn swap_remove_index(&mut self, index: usize) -> Entry<K, V> {
        let last = self.entries.len() - 1;
        self.erase_index(self.entries[index].hash, index as u32);
        let removed = if index == last {
            self.entries.pop().unwrap()
        } else {
            let removed = self.entries.swap_remove(index);
            // Update index for the entry that moved into `index`.
            let moved_hash = self.entries[index].hash;
            self.erase_index(moved_hash, last as u32);
            let mask = self.indices.len() - 1;
            insert_index(
                &mut self.indices,
                &mut self.dibs,
                mask,
                moved_hash,
                index as u32,
            );
            removed
        };
        removed
    }

    fn shift_remove_index(&mut self, index: usize) -> Entry<K, V> {
        self.erase_index(self.entries[index].hash, index as u32);
        let removed = self.entries.remove(index);
        // All entries after `index` shifted down — rebuild indices (simple + correct).
        // For large maps this is O(n); matches indexmap's shift cost class.
        if !self.entries.is_empty() {
            let n = self.indices.len();
            self.rehash_to(n);
        } else {
            for s in &mut self.indices {
                *s = EMPTY;
            }
            for d in &mut self.dibs {
                *d = 0;
            }
        }
        removed
    }

    fn erase_index(&mut self, hash: u64, entry_idx: u32) {
        if self.indices.is_empty() {
            return;
        }
        let mask = self.indices.len() - 1;
        let mut pos = (hash as usize) & mask;
        let mut dib = 0u16;
        loop {
            if self.indices[pos] == EMPTY {
                return;
            }
            if self.indices[pos] == entry_idx && self.dibs[pos] == dib {
                // Backward-shift deletion (robin-hood).
                loop {
                    let next = (pos + 1) & mask;
                    if self.indices[next] == EMPTY || self.dibs[next] == 0 {
                        self.indices[pos] = EMPTY;
                        self.dibs[pos] = 0;
                        return;
                    }
                    self.indices[pos] = self.indices[next];
                    self.dibs[pos] = self.dibs[next] - 1;
                    pos = next;
                }
            }
            if dib > self.dibs[pos] {
                return;
            }
            dib = dib.saturating_add(1);
            pos = (pos + 1) & mask;
        }
    }

    #[inline]
    fn find_index<Q>(&self, hash: u64, key: &Q) -> Option<usize>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq,
    {
        if self.indices.is_empty() {
            return None;
        }
        let mask = self.indices.len() - 1;
        let mut pos = (hash as usize) & mask;
        let mut dib = 0u16;
        loop {
            let idx = self.indices[pos];
            if idx == EMPTY {
                return None;
            }
            if dib > self.dibs[pos] {
                return None;
            }
            let i = idx as usize;
            if self.entries[i].hash == hash && self.entries[i].key.borrow() == key {
                return Some(i);
            }
            dib = dib.saturating_add(1);
            pos = (pos + 1) & mask;
        }
    }

    #[inline]
    pub fn get_index_of<Q>(&self, key: &Q) -> Option<usize>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let hash = self.hash_key(key);
        self.find_index(hash, key)
    }
}

impl<K, V, S> Default for IndexMap<K, V, S>
where
    S: Default,
{
    #[inline]
    fn default() -> Self {
        Self::with_hasher(S::default())
    }
}

impl<K, V, S> fmt::Debug for IndexMap<K, V, S>
where
    K: fmt::Debug,
    V: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl<'a, K, V, S> IntoIterator for &'a IndexMap<K, V, S> {
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<K, V, S> IntoIterator for IndexMap<K, V, S> {
    type Item = (K, V);
    type IntoIter = IntoIter<K, V>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            inner: self.entries.into_iter(),
        }
    }
}

pub struct Iter<'a, K, V> {
    inner: std::slice::Iter<'a, Entry<K, V>>,
}

impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|e| (&e.key, &e.value))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<K, V> ExactSizeIterator for Iter<'_, K, V> {}
impl<K, V> FusedIterator for Iter<'_, K, V> {}
impl<K, V> DoubleEndedIterator for Iter<'_, K, V> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back().map(|e| (&e.key, &e.value))
    }
}

pub struct IterMut<'a, K, V> {
    inner: std::slice::IterMut<'a, Entry<K, V>>,
}

impl<'a, K, V> Iterator for IterMut<'a, K, V> {
    type Item = (&'a K, &'a mut V);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|e| (&e.key, &mut e.value))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<K, V> ExactSizeIterator for IterMut<'_, K, V> {}
impl<K, V> FusedIterator for IterMut<'_, K, V> {}

pub struct Keys<'a, K, V> {
    inner: Iter<'a, K, V>,
}

impl<'a, K, V> Iterator for Keys<'a, K, V> {
    type Item = &'a K;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(k, _)| k)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<K, V> ExactSizeIterator for Keys<'_, K, V> {}
impl<K, V> FusedIterator for Keys<'_, K, V> {}

pub struct Values<'a, K, V> {
    inner: Iter<'a, K, V>,
}

impl<'a, K, V> Iterator for Values<'a, K, V> {
    type Item = &'a V;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(_, v)| v)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<K, V> ExactSizeIterator for Values<'_, K, V> {}
impl<K, V> FusedIterator for Values<'_, K, V> {}

pub struct IntoIter<K, V> {
    inner: std::vec::IntoIter<Entry<K, V>>,
}

impl<K, V> Iterator for IntoIter<K, V> {
    type Item = (K, V);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|e| (e.key, e.value))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<K, V> ExactSizeIterator for IntoIter<K, V> {}
impl<K, V> FusedIterator for IntoIter<K, V> {}

/// Insertion-ordered set backed by [`IndexMap`].
#[derive(Clone)]
pub struct IndexSet<T, S = RandomState> {
    map: IndexMap<T, (), S>,
}

impl<T> IndexSet<T, RandomState> {
    #[inline]
    pub fn new() -> Self {
        Self {
            map: IndexMap::new(),
        }
    }

    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            map: IndexMap::with_capacity(capacity),
        }
    }
}

impl<T> IndexSet<T, FxBuildHasher> {
    #[inline]
    pub fn with_fx() -> Self {
        Self {
            map: IndexMap::with_fx(),
        }
    }

    #[inline]
    pub fn with_capacity_fx(capacity: usize) -> Self {
        Self {
            map: IndexMap::with_capacity_fx(capacity),
        }
    }
}

impl<T, S> IndexSet<T, S> {
    #[inline]
    pub fn with_hasher(hash_builder: S) -> Self {
        Self {
            map: IndexMap::with_hasher(hash_builder),
        }
    }

    #[inline]
    pub fn with_capacity_and_hasher(capacity: usize, hash_builder: S) -> Self {
        Self {
            map: IndexMap::with_capacity_and_hasher(capacity, hash_builder),
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    #[inline]
    pub fn clear(&mut self) {
        self.map.clear();
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.map.capacity()
    }

    #[inline]
    pub fn iter(&self) -> SetIter<'_, T> {
        SetIter {
            inner: self.map.keys(),
        }
    }

    #[inline]
    pub fn get_index(&self, index: usize) -> Option<&T> {
        self.map.get_index(index).map(|(k, _)| k)
    }
}

impl<T, S> IndexSet<T, S>
where
    T: Hash + Eq,
    S: BuildHasher,
{
    #[inline]
    pub fn insert(&mut self, value: T) -> bool {
        self.map.insert(value, ()).is_none()
    }

    #[inline]
    pub fn contains<Q>(&self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.map.contains_key(value)
    }

    #[inline]
    pub fn swap_remove<Q>(&mut self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.map.swap_remove(value).is_some()
    }

    #[inline]
    pub fn shift_remove<Q>(&mut self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.map.shift_remove(value).is_some()
    }

    #[inline]
    pub fn get<Q>(&self, value: &Q) -> Option<&T>
    where
        T: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.map.get_key_value(value).map(|(k, _)| k)
    }
}

impl<T, S> Default for IndexSet<T, S>
where
    S: Default,
{
    #[inline]
    fn default() -> Self {
        Self::with_hasher(S::default())
    }
}

impl<T, S> fmt::Debug for IndexSet<T, S>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.iter()).finish()
    }
}

impl<'a, T, S> IntoIterator for &'a IndexSet<T, S> {
    type Item = &'a T;
    type IntoIter = SetIter<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub struct SetIter<'a, T> {
    inner: Keys<'a, T, ()>,
}

impl<'a, T> Iterator for SetIter<'a, T> {
    type Item = &'a T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<T> ExactSizeIterator for SetIter<'_, T> {}
impl<T> FusedIterator for SetIter<'_, T> {}

#[inline]
fn indices_for(entries: usize) -> usize {
    let min = (entries.saturating_mul(LOAD_DEN) / LOAD_NUM).next_power_of_two();
    min.max(MIN_INDICES)
}

#[inline]
fn insert_index(indices: &mut [u32], dibs: &mut [u16], mask: usize, hash: u64, entry_idx: u32) {
    let mut pos = (hash as usize) & mask;
    let mut dib = 0u16;
    let mut idx = entry_idx;
    loop {
        if indices[pos] == EMPTY {
            indices[pos] = idx;
            dibs[pos] = dib;
            return;
        }
        if dibs[pos] < dib {
            // Robin-hood: steal from the richer slot.
            mem::swap(&mut indices[pos], &mut idx);
            mem::swap(&mut dibs[pos], &mut dib);
        }
        dib = dib.saturating_add(1);
        pos = (pos + 1) & mask;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_preservation() {
        let mut m = IndexMap::with_fx();
        for i in 0..100 {
            m.insert(i, i * 10);
        }
        let keys: Vec<_> = m.keys().copied().collect();
        assert_eq!(keys, (0..100).collect::<Vec<_>>());
        // Update existing — order unchanged.
        m.insert(50, 999);
        let keys2: Vec<_> = m.keys().copied().collect();
        assert_eq!(keys2, keys);
        assert_eq!(m.get(&50), Some(&999));
    }

    #[test]
    fn swap_and_shift_remove() {
        let mut m = IndexMap::with_capacity_fx(8);
        for i in 0..5 {
            m.insert(format!("k{i}"), i);
        }
        assert_eq!(m.swap_remove("k1"), Some(1));
        // After swap_remove, last element (k4) moved into the hole; order is not shift-stable.
        assert!(!m.contains_key("k1"));
        assert_eq!(m.len(), 4);

        let mut m2 = IndexMap::with_fx();
        for i in 0..5 {
            m2.insert(i, i);
        }
        assert_eq!(m2.shift_remove(&2), Some(2));
        let keys: Vec<_> = m2.keys().copied().collect();
        assert_eq!(keys, vec![0, 1, 3, 4]);
    }

    #[test]
    fn set_basics() {
        let mut s = IndexSet::with_fx();
        assert!(s.insert("a"));
        assert!(!s.insert("a"));
        assert!(s.insert("b"));
        assert!(s.insert("c"));
        assert!(s.shift_remove("b"));
        let v: Vec<_> = s.iter().copied().collect();
        assert_eq!(v, vec!["a", "c"]);
    }

    #[test]
    fn get_index_and_iter() {
        let mut m = IndexMap::with_fx();
        m.insert("x", 1);
        m.insert("y", 2);
        m.insert("z", 3);
        assert_eq!(m.get_index(1), Some((&"y", &2)));
        assert_eq!(m.get_index_of(&"z"), Some(2));
        let pairs: Vec<_> = m.iter().map(|(k, v)| (*k, *v)).collect();
        assert_eq!(pairs, vec![("x", 1), ("y", 2), ("z", 3)]);
    }

    #[test]
    fn hundred_k_insert_smoke() {
        let n = 100_000;
        let mut m = IndexMap::with_capacity_fx(n);
        for i in 0..n {
            m.insert(i, i);
        }
        assert_eq!(m.len(), n);
        assert_eq!(m.get(&(n / 2)), Some(&(n / 2)));
        for i in (0..n).step_by(7) {
            assert_eq!(m.swap_remove(&i), Some(i));
        }
        assert_eq!(m.len(), n - n.div_ceil(7));
    }

    #[test]
    fn no_alloc_get_hot_path() {
        // Pre-sized map: repeated get must not grow.
        let mut m = IndexMap::with_capacity_fx(64);
        for i in 0..32u32 {
            m.insert(i, i);
        }
        let cap = m.capacity();
        for _ in 0..10_000 {
            let _ = m.get(&7u32);
        }
        assert_eq!(m.capacity(), cap);
    }
}
