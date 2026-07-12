//! Slice shuffle and random choice (Fisher–Yates).

use crate::rng::Rng;

/// In-place shuffle and random element selection for slices.
pub trait SliceRandom {
    type Item;
    fn shuffle(&mut self, rng: &mut impl Rng);
    fn choose<'a>(&'a self, rng: &mut impl Rng) -> Option<&'a Self::Item>;
}

impl<T> SliceRandom for [T] {
    type Item = T;
    fn shuffle(&mut self, rng: &mut impl Rng) {
        if self.len() <= 1 {
            return;
        }
        for i in (1..self.len()).rev() {
            let j = rng.gen_range_usize(0, i + 1);
            self.swap(i, j);
        }
    }

    fn choose<'a>(&'a self, rng: &mut impl Rng) -> Option<&'a T> {
        if self.is_empty() {
            None
        } else {
            Some(&self[rng.gen_range_usize(0, self.len())])
        }
    }
}

impl<T> SliceRandom for Vec<T> {
    type Item = T;
    fn shuffle(&mut self, rng: &mut impl Rng) {
        self.as_mut_slice().shuffle(rng);
    }

    fn choose<'a>(&'a self, rng: &mut impl Rng) -> Option<&'a T> {
        self.as_slice().choose(rng)
    }
}
