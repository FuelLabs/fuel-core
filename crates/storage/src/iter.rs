//! Iterators returned by the storage.

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
