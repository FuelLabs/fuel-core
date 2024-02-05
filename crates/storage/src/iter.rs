//! The module defines primitives that allow iterating of the storage.

use crate::kv_store::{
    KVItem,
    KeyValueStore,
};

/// A boxed variant of the iterator that can be used as a return type of the traits.
pub struct BoxedIter<'a, T> {
    iter: Box<dyn Iterator<Item = T> + 'a>,
}

impl<'a, T> Iterator for BoxedIter<'a, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

/// The traits simplifies conversion into `BoxedIter`.
pub trait IntoBoxedIter<'a, T> {
    /// Converts `Self` iterator into `BoxedIter`.
    fn into_boxed(self) -> BoxedIter<'a, T>;
}

impl<'a, T, I> IntoBoxedIter<'a, T> for I
where
    I: Iterator<Item = T> + 'a,
{
    fn into_boxed(self) -> BoxedIter<'a, T> {
        BoxedIter {
            iter: Box::new(self),
        }
    }
}

/// A enum for iterating across the database
#[derive(Copy, Clone, Debug, PartialOrd, Eq, PartialEq)]
pub enum IterDirection {
    /// Iterate forward
    Forward,
    /// Iterate backward
    Reverse,
}

impl Default for IterDirection {
    fn default() -> Self {
        Self::Forward
    }
}

/// A trait for iterating over the storage of [`KeyValueStore`].
pub trait IteratorableStore: KeyValueStore {
    /// Returns an iterator over the values in the storage.
    fn iter_all(
        &self,
        column: Self::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<KVItem>;
}
