//! The module provides plain abstract definition of the key-value store.

use crate::{
    Direction,
    NextEntry,
    Result as StorageResult,
    transactional::ReferenceBytesKey,
};

#[cfg(feature = "alloc")]
use alloc::{
    boxed::Box,
    string::String,
    vec::Vec,
};

#[cfg(feature = "std")]
use core::ops::Deref;

/// The key of the storage.
pub type Key = ReferenceBytesKey;
/// The value of the storage. It is wrapped into the `Arc` to provide less cloning of massive objects.
pub type Value = alloc::sync::Arc<[u8]>;

/// The pair of key and value from the storage.
pub type KVItem = StorageResult<(Key, Value)>;
/// The key from the storage.
pub type KeyItem = StorageResult<Key>;

/// A column of the storage.
pub trait StorageColumn: Copy + core::fmt::Debug {
    /// Returns the name of the column.
    fn name(&self) -> String;

    /// Returns the id of the column.
    fn id(&self) -> u32;

    /// Returns the id of the column as an `usize`.
    fn as_usize(&self) -> usize {
        self.id() as usize
    }
}

/// The definition of the key-value inspection store.
#[impl_tools::autoimpl(for<T: trait> &T, &mut T, Box<T>)]
pub trait KeyValueInspect {
    /// The type of the column.
    type Column: StorageColumn;

    /// Checks if the value exists in the storage.
    fn exists(&self, key: &[u8], column: Self::Column) -> StorageResult<bool> {
        Ok(self.size_of_value(key, column)?.is_some())
    }

    /// Returns the size of the value in the storage.
    fn size_of_value(
        &self,
        key: &[u8],
        column: Self::Column,
    ) -> StorageResult<Option<usize>> {
        Ok(self.get(key, column)?.map(|value| value.len()))
    }

    /// Returns the value from the storage.
    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>>;

    /// Returns the next key and value from the storage.
    fn get_next(
        &self,
        start_key: &[u8],
        column: Self::Column,
        direction: Direction,
        max_iterations: usize,
    ) -> StorageResult<NextEntry<Key, Value>>;

    /// Reads the value from the storage into the `buf` and returns the whether the value exists.
    fn read(
        &self,
        key: &[u8],
        column: Self::Column,
        offset: usize,
        buf: &mut [u8],
    ) -> StorageResult<bool> {
        let Some(value) = self.get(key, column)? else {
            return Ok(false);
        };

        let bytes_len = value.as_ref().len();
        let start = offset;
        let buf_len = buf.len();
        let end = offset.saturating_add(buf_len);

        if end > bytes_len {
            return Err(anyhow::anyhow!(
                        "Offset `{offset}` + buf_len `{buf_len}` read until {end} which is out of bounds `{bytes_len}` for key `{:?}`",
                        key
                    )
                    .into());
        }

        let starting_from_offset = &value.as_ref()[start..end];
        buf[..].copy_from_slice(starting_from_offset);
        Ok(true)
    }
}

#[cfg(feature = "std")]
impl<T> KeyValueInspect for std::sync::Arc<T>
where
    T: KeyValueInspect,
{
    type Column = T::Column;

    fn exists(&self, key: &[u8], column: Self::Column) -> StorageResult<bool> {
        self.deref().exists(key, column)
    }

    fn size_of_value(
        &self,
        key: &[u8],
        column: Self::Column,
    ) -> StorageResult<Option<usize>> {
        self.deref().size_of_value(key, column)
    }

    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        self.deref().get(key, column)
    }

    fn get_next(
        &self,
        start_key: &[u8],
        column: Self::Column,
        direction: Direction,
        max_iterations: usize,
    ) -> StorageResult<NextEntry<Key, Value>> {
        self.deref()
            .get_next(start_key, column, direction, max_iterations)
    }

    fn read(
        &self,
        key: &[u8],
        column: Self::Column,
        offset: usize,
        buf: &mut [u8],
    ) -> StorageResult<bool> {
        self.deref().read(key, column, offset, buf)
    }
}

/// The definition of the key-value mutation store.
#[impl_tools::autoimpl(for<T: trait> &mut T, Box<T>)]
pub trait KeyValueMutate: KeyValueInspect {
    /// Inserts the `Value` into the storage.
    fn put(
        &mut self,
        key: &[u8],
        column: Self::Column,
        value: Value,
    ) -> StorageResult<()> {
        self.write(key, column, value.as_ref()).map(|_| ())
    }

    /// Put the `Value` into the storage and return the old value.
    fn replace(
        &mut self,
        key: &[u8],
        column: Self::Column,
        value: Value,
    ) -> StorageResult<Option<Value>> {
        let old_value = self.get(key, column)?;
        self.put(key, column, value)?;
        Ok(old_value)
    }

    /// Writes the `buf` into the storage and returns the number of written bytes.
    fn write(
        &mut self,
        key: &[u8],
        column: Self::Column,
        buf: &[u8],
    ) -> StorageResult<usize>;

    /// Removes the value from the storage and returns it.
    fn take(&mut self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        let old_value = self.get(key, column)?;
        self.delete(key, column)?;
        Ok(old_value)
    }

    /// Removes the value from the storage.
    fn delete(&mut self, key: &[u8], column: Self::Column) -> StorageResult<()>;
}

/// The operation to write into the storage.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum WriteOperation {
    /// Insert the value into the storage.
    Insert(Value),
    /// Remove the value from the storage.
    Remove,
}

/// The definition of the key-value store with batch operations.
#[impl_tools::autoimpl(for<T: trait> &mut T, Box<T>)]
pub trait BatchOperations: KeyValueMutate {
    /// Writes the batch of the entries into the storage.
    fn batch_write<I>(&mut self, column: Self::Column, entries: I) -> StorageResult<()>
    where
        I: Iterator<Item = (Vec<u8>, WriteOperation)>;
}

/// An iterator that allows to iterate over the next items in the storage.
pub struct StorageNextIterator<'a, S>
where
    S: KeyValueInspect,
{
    storage: &'a S,
    column: S::Column,
    start_key: Option<alloc::borrow::Cow<'a, Key>>,
    direction: Direction,
    max_iterations: usize,
}

impl<'a, S> StorageNextIterator<'a, S>
where
    S: KeyValueInspect,
{
    /// Creates a new `StorageNextIterator` with the given storage, column, start key, and direction.
    pub fn new(
        storage: &'a S,
        column: S::Column,
        start_key: alloc::borrow::Cow<'a, Key>,
        direction: Direction,
        max_iterations: usize,
    ) -> Self {
        Self {
            storage,
            column,
            start_key: Some(start_key),
            direction,
            max_iterations,
        }
    }

    fn next(&mut self, start_key: &Key) -> StorageResult<NextEntry<'a, Key, Value>> {
        let next = self.storage.get_next(
            start_key,
            self.column,
            self.direction,
            self.max_iterations,
        )?;

        self.max_iterations = self.max_iterations.saturating_sub(next.iterations);

        // If we have a next item, we update the start_key to the current key
        self.start_key = next.entry.as_ref().map(|(key, _)| key.clone());

        Ok(next)
    }
}

impl<'a, S> Iterator for StorageNextIterator<'a, S>
where
    S: KeyValueInspect,
{
    type Item = StorageResult<NextEntry<'a, Key, Value>>;

    fn next(&mut self) -> Option<Self::Item> {
        let key = match self.start_key.take() {
            Some(key) => key,
            None => return None, // No more items to iterate
        };

        Some(self.next(&key))
    }
}
