//! Persistent Hash Array Mapped Trie (HAMT), branching factor 32.
//!
//! Supports exactly the operations the Niao runtime needs: `new`, `update`,
//! `get`, `len`, `keys`, `iter`, `ptr_eq`, `Clone`, and `FromIterator`.

use crate::hash_key;
use std::hash::Hash;
use std::rc::Rc;

const BITS: u32 = 5;
const MASK64: u64 = 31;

#[derive(Clone)]
struct Node<K, V> {
    /// Bit `i` set means slot `i` (5 bits of the hash) is occupied.
    bitmap: u32,
    /// Occupied children, densely packed in ascending slot order.
    children: Vec<Child<K, V>>,
}

#[derive(Clone)]
enum Child<K, V> {
    Entry(K, V),
    /// Keys whose full 64-bit hashes collided.
    Collision(Vec<(K, V)>),
    Node(Rc<Node<K, V>>),
}

/// A persistent hash map with structural sharing.
#[derive(Clone)]
pub struct HashMap<K, V> {
    root: Rc<Node<K, V>>,
    len: usize,
}

/// Dense index of `bitpos` within `bitmap` = popcount of the lower bits.
#[inline]
fn child_index(bitmap: u32, bitpos: u32) -> usize {
    (bitmap & (bitpos - 1)).count_ones() as usize
}

impl<K, V> HashMap<K, V> {
    /// An empty map.
    pub fn new() -> Self {
        HashMap {
            root: Rc::new(Node {
                bitmap: 0,
                children: Vec::new(),
            }),
            len: 0,
        }
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.len
    }

    /// True when the map has no entries.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// True when both handles share the same root allocation.
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.root, &other.root)
    }
}

impl<K, V> Default for HashMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Hash + Eq, V> HashMap<K, V> {
    /// Borrow the value for `key`, if present.
    pub fn get(&self, key: &K) -> Option<&V> {
        let hash = hash_key(key);
        let mut node = &*self.root;
        let mut shift = 0u32;
        loop {
            let frag = ((hash >> shift) & MASK64) as u32;
            let bitpos = 1u32 << frag;
            if node.bitmap & bitpos == 0 {
                return None;
            }
            let idx = child_index(node.bitmap, bitpos);
            match &node.children[idx] {
                Child::Entry(k, v) => return if k == key { Some(v) } else { None },
                Child::Collision(items) => {
                    return items.iter().find(|(k, _)| k == key).map(|(_, v)| v)
                }
                Child::Node(sub) => {
                    node = &**sub;
                    shift += BITS;
                }
            }
        }
    }

    /// True when `key` is present.
    pub fn contains_key(&self, key: &K) -> bool {
        self.get(key).is_some()
    }
}

impl<K: Hash + Eq + Clone, V: Clone> HashMap<K, V> {
    /// Return a new map with `key` set to `value`. The original is unchanged;
    /// untouched sub-tries are shared.
    pub fn update(&self, key: K, value: V) -> Self {
        let hash = hash_key(&key);
        let mut root = self.root.clone();
        let added = insert_node(&mut root, 0, hash, key, value);
        HashMap {
            root,
            len: self.len + usize::from(added),
        }
    }

    /// Iterate keys (unordered). Allocates a `Vec` of references.
    pub fn keys(&self) -> std::vec::IntoIter<&K> {
        let mut out = Vec::with_capacity(self.len);
        collect_keys(&self.root, &mut out);
        out.into_iter()
    }

    /// Iterate key/value pairs (unordered). Allocates a `Vec` of references.
    pub fn iter(&self) -> std::vec::IntoIter<(&K, &V)> {
        let mut out = Vec::with_capacity(self.len);
        collect_entries(&self.root, &mut out);
        out.into_iter()
    }
}

/// Insert into `node` (path-copying via `make_mut`); returns whether a new key
/// was added (vs. an existing key's value replaced).
fn insert_node<K, V>(node: &mut Rc<Node<K, V>>, shift: u32, hash: u64, key: K, value: V) -> bool
where
    K: Hash + Eq + Clone,
    V: Clone,
{
    debug_assert!(shift < 64, "HAMT recursion exceeded hash width");
    let n = Rc::make_mut(node);
    let frag = ((hash >> shift) & MASK64) as u32;
    let bitpos = 1u32 << frag;
    let idx = child_index(n.bitmap, bitpos);

    if n.bitmap & bitpos == 0 {
        n.children.insert(idx, Child::Entry(key, value));
        n.bitmap |= bitpos;
        return true;
    }

    // Occupied slot: take the child out, transform it, put it back.
    let child = std::mem::replace(&mut n.children[idx], Child::Collision(Vec::new()));
    let (new_child, added) = match child {
        Child::Node(mut sub) => {
            let a = insert_node(&mut sub, shift + BITS, hash, key, value);
            (Child::Node(sub), a)
        }
        Child::Collision(mut items) => {
            if let Some(pos) = items.iter().position(|kv| kv.0 == key) {
                items[pos].1 = value;
                (Child::Collision(items), false)
            } else {
                items.push((key, value));
                (Child::Collision(items), true)
            }
        }
        Child::Entry(ek, ev) => {
            if ek == key {
                (Child::Entry(ek, value), false)
            } else {
                let ehash = hash_key(&ek);
                (merge(shift + BITS, ehash, ek, ev, hash, key, value), true)
            }
        }
    };
    n.children[idx] = new_child;
    added
}

/// Combine two distinct keys into a sub-trie starting at `shift`.
fn merge<K, V>(shift: u32, h1: u64, k1: K, v1: V, h2: u64, k2: K, v2: V) -> Child<K, V> {
    if shift >= 64 {
        return Child::Collision(vec![(k1, v1), (k2, v2)]);
    }
    let f1 = ((h1 >> shift) & MASK64) as u32;
    let f2 = ((h2 >> shift) & MASK64) as u32;
    if f1 == f2 {
        let child = merge(shift + BITS, h1, k1, v1, h2, k2, v2);
        Child::Node(Rc::new(Node {
            bitmap: 1u32 << f1,
            children: vec![child],
        }))
    } else {
        let bitmap = (1u32 << f1) | (1u32 << f2);
        let children = if f1 < f2 {
            vec![Child::Entry(k1, v1), Child::Entry(k2, v2)]
        } else {
            vec![Child::Entry(k2, v2), Child::Entry(k1, v1)]
        };
        Child::Node(Rc::new(Node { bitmap, children }))
    }
}

fn collect_keys<'a, K, V>(node: &'a Rc<Node<K, V>>, out: &mut Vec<&'a K>) {
    for child in &node.children {
        match child {
            Child::Entry(k, _) => out.push(k),
            Child::Collision(items) => {
                for (k, _) in items {
                    out.push(k);
                }
            }
            Child::Node(sub) => collect_keys(sub, out),
        }
    }
}

fn collect_entries<'a, K, V>(node: &'a Rc<Node<K, V>>, out: &mut Vec<(&'a K, &'a V)>) {
    for child in &node.children {
        match child {
            Child::Entry(k, v) => out.push((k, v)),
            Child::Collision(items) => {
                for (k, v) in items {
                    out.push((k, v));
                }
            }
            Child::Node(sub) => collect_entries(sub, out),
        }
    }
}

impl<K: Hash + Eq + Clone, V: Clone> FromIterator<(K, V)> for HashMap<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut m = HashMap::new();
        for (k, v) in iter {
            m = m.update(k, v);
        }
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap as StdMap;

    #[test]
    fn insert_and_get_many() {
        let mut m = HashMap::new();
        for i in 0..2000u64 {
            m = m.update(i, i * i);
        }
        assert_eq!(m.len(), 2000);
        for i in 0..2000u64 {
            assert_eq!(m.get(&i), Some(&(i * i)), "get({i})");
        }
        assert_eq!(m.get(&999999), None);
    }

    #[test]
    fn overwrite_keeps_len_and_snapshots() {
        let before = {
            let mut m = HashMap::new();
            for i in 0..100u64 {
                m = m.update(i, i);
            }
            m
        };
        let after = before.update(50, 5000);
        assert_eq!(before.len(), 100);
        assert_eq!(after.len(), 100);
        assert_eq!(before.get(&50), Some(&50));
        assert_eq!(after.get(&50), Some(&5000));
    }

    #[test]
    fn string_keys_match_std() {
        let mut m = HashMap::new();
        let mut std = StdMap::new();
        for i in 0..1000 {
            let k = format!("key-{i}");
            m = m.update(k.clone(), i);
            std.insert(k, i);
        }
        assert_eq!(m.len(), std.len());
        for (k, v) in std.iter() {
            assert_eq!(m.get(k), Some(v));
        }
        let mut keys: Vec<String> = m.keys().cloned().collect();
        keys.sort();
        let mut expect: Vec<String> = std.keys().cloned().collect();
        expect.sort();
        assert_eq!(keys, expect);
    }

    #[test]
    fn ptr_eq_tracks_sharing() {
        let a: HashMap<u64, u64> = (0..50u64).map(|i| (i, i)).collect();
        let b = a.clone();
        assert!(a.ptr_eq(&b));
        let c = a.update(10, 99);
        assert!(!a.ptr_eq(&c));
        assert_eq!(a.get(&10), Some(&10));
        assert_eq!(c.get(&10), Some(&99));
    }
}
