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

    pub fn insert(&mut self, k: T) -> bool {
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
