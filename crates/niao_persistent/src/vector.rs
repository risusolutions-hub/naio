//! Persistent bit-partitioned vector trie (branching factor 32).
//!
//! Supports exactly the operations the Niao runtime needs: `new`, `push_back`,
//! `update`, `get`, `len`, `ptr_eq`, `iter`, `Clone`, and `FromIterator`.

use std::rc::Rc;

const BITS: usize = 5;
const WIDTH: usize = 1 << BITS; // 32
const MASK: usize = WIDTH - 1;

#[derive(Clone)]
enum Node<T> {
    Branch(Vec<Rc<Node<T>>>),
    Leaf(Vec<T>),
}

/// A persistent vector with O(log32 n) `push_back`/`update`/`get` and cheap
/// structural-sharing `clone`.
#[derive(Clone)]
pub struct Vector<T> {
    root: Rc<Node<T>>,
    len: usize,
    /// `BITS * (height - 1)`; zero when the root is a leaf.
    shift: usize,
}

impl<T> Vector<T> {
    /// An empty vector.
    pub fn new() -> Self {
        Vector {
            root: Rc::new(Node::Leaf(Vec::new())),
            len: 0,
            shift: 0,
        }
    }

    /// Number of elements.
    pub fn len(&self) -> usize {
        self.len
    }

    /// True when the vector has no elements.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// True when both handles share the same root allocation (i.e. one is an
    /// unmodified clone of the other).
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.root, &other.root)
    }

    /// Borrow the element at `index`, if present.
    pub fn get(&self, index: usize) -> Option<&T> {
        if index >= self.len {
            return None;
        }
        let mut node = &*self.root;
        let mut shift = self.shift;
        loop {
            match node {
                Node::Branch(children) => {
                    let idx = (index >> shift) & MASK;
                    node = &**children.get(idx)?;
                    shift = shift.saturating_sub(BITS);
                }
                Node::Leaf(vals) => return vals.get(index & MASK),
            }
        }
    }

    /// Iterate elements in order. Allocates a `Vec` of references.
    pub fn iter(&self) -> std::vec::IntoIter<&T> {
        let mut out = Vec::with_capacity(self.len);
        collect_refs(&self.root, &mut out);
        out.into_iter()
    }

    /// Max elements addressable by a tree of the current height.
    fn capacity(&self) -> usize {
        1usize
            .checked_shl((self.shift + BITS) as u32)
            .unwrap_or(usize::MAX)
    }
}

impl<T: Clone> Vector<T> {
    /// Append `value`, mutating this handle to the new version. Untouched
    /// sub-trees remain shared with any prior clones.
    pub fn push_back(&mut self, value: T) {
        if self.len == self.capacity() {
            // Grow: wrap the old root in a new branch and raise the height.
            let old = self.root.clone();
            self.root = Rc::new(Node::Branch(vec![old]));
            self.shift += BITS;
        }
        let idx = self.len;
        let shift = self.shift;
        push_into(&mut self.root, shift, idx, value);
        self.len += 1;
    }

    /// Return a new vector with `index` set to `value`. Panics if out of bounds.
    pub fn update(&self, index: usize, value: T) -> Self {
        assert!(index < self.len, "Vector::update index out of bounds");
        let mut next = self.clone();
        set_into(&mut next.root, next.shift, index, value);
        next
    }
}

fn push_into<T: Clone>(node: &mut Rc<Node<T>>, shift: usize, idx: usize, value: T) {
    match Rc::make_mut(node) {
        Node::Leaf(vals) => vals.push(value),
        Node::Branch(children) => {
            let sub = (idx >> shift) & MASK;
            if sub < children.len() {
                push_into(&mut children[sub], shift - BITS, idx, value);
            } else {
                children.push(new_path(shift - BITS, value));
            }
        }
    }
}

fn set_into<T: Clone>(node: &mut Rc<Node<T>>, shift: usize, index: usize, value: T) {
    match Rc::make_mut(node) {
        Node::Leaf(vals) => vals[index & MASK] = value,
        Node::Branch(children) => {
            let sub = (index >> shift) & MASK;
            set_into(&mut children[sub], shift - BITS, index, value);
        }
    }
}

fn new_path<T>(shift: usize, value: T) -> Rc<Node<T>> {
    if shift == 0 {
        Rc::new(Node::Leaf(vec![value]))
    } else {
        Rc::new(Node::Branch(vec![new_path(shift - BITS, value)]))
    }
}

fn collect_refs<'a, T>(node: &'a Rc<Node<T>>, out: &mut Vec<&'a T>) {
    match &**node {
        Node::Leaf(vals) => out.extend(vals.iter()),
        Node::Branch(children) => {
            for c in children {
                collect_refs(c, out);
            }
        }
    }
}

impl<T> Default for Vector<T> {
    fn default() -> Self {
        Vector::new()
    }
}

impl<T: Clone> FromIterator<T> for Vector<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut v = Vector::new();
        for x in iter {
            v.push_back(x);
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_get_len_across_height_growth() {
        let mut v = Vector::new();
        for i in 0..5000usize {
            v.push_back(i);
        }
        assert_eq!(v.len(), 5000);
        for i in 0..5000usize {
            assert_eq!(v.get(i), Some(&i), "get({i})");
        }
        assert_eq!(v.get(5000), None);
    }

    #[test]
    fn snapshots_are_independent() {
        let mut mid = Vector::new();
        for i in 0..500usize {
            mid.push_back(i);
        }
        let mut full = mid.clone();
        for i in 500..1000usize {
            full.push_back(i);
        }
        assert_eq!(mid.len(), 500);
        assert_eq!(full.len(), 1000);
        assert_eq!(mid.get(499), Some(&499));
        assert_eq!(mid.get(500), None);
        assert_eq!(full.get(999), Some(&999));
    }

    #[test]
    fn update_preserves_original() {
        let base: Vector<i64> = (0..1000i64).collect();
        let changed = base.update(250, 9999);
        assert_eq!(base.get(250), Some(&250));
        assert_eq!(changed.get(250), Some(&9999));
        assert_eq!(changed.get(251), Some(&251));
        assert_eq!(changed.len(), 1000);
    }

    #[test]
    fn ptr_eq_tracks_sharing() {
        let a: Vector<i64> = (0..100i64).collect();
        let b = a.clone();
        assert!(a.ptr_eq(&b));
        let c = a.update(10, -1);
        assert!(!a.ptr_eq(&c));
    }

    #[test]
    fn from_iter_and_iter_roundtrip() {
        let v: Vector<i64> = (0..2000i64).collect();
        let collected: Vec<i64> = v.iter().copied().collect();
        let expect: Vec<i64> = (0..2000i64).collect();
        assert_eq!(collected, expect);
    }
}
