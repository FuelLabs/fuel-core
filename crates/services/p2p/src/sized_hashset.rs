use std::{
    collections::HashSet,
    hash::Hash,
};

pub struct SizedHashset<T> {
    capacity: usize,
    inner: HashSet<T>,
}

impl<T> SizedHashset<T>
where
    T: Eq + Hash,
{
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            inner: HashSet::new(),
        }
    }

    // Will idempotently insert if there is room in the set.
    // Redundant keys don't take up more room
    pub fn insert_new_if_room(&mut self, k: T) -> bool {
        if self.inner.len() >= self.capacity && !self.inner.contains(&k) {
            false
        } else {
            self.inner.insert(k);
            true
        }
    }

    pub fn remove(&mut self, k: T) {
        self.inner.remove(&k);
    }
}
