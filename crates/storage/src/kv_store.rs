//! The module provides plain abstract definition of the key-value store.

use crate::{
    Error as StorageError,
    Result as StorageResult,
};
use std::sync::Arc;

/// The value of the storage. It is wrapped into the `Arc` to provide less cloning of massive objects.
pub type Value = Arc<Vec<u8>>;
/// The pair of key and value from the storage.
pub type KVItem = StorageResult<(Vec<u8>, Value)>;

/// A column of the storage.
pub trait StorageColumn: Copy + core::fmt::Debug {
    /// Returns the name of the column.
    fn name(&self) -> &'static str;

    /// Returns the id of the column.
    fn id(&self) -> u32;

    /// Returns the id of the column as an `usize`.
    fn as_usize(&self) -> usize {
        self.id() as usize
    }
}

// TODO: Use `&mut self` for all mutable methods.
//  It requires refactoring of all services because right now, most of them work with `&self` storage.
/// The definition of the key-value store.
#[impl_tools::autoimpl(for<T: trait> &T, &mut T, Box<T>, Arc<T>)]
pub trait KeyValueStore {
    /// The type of the column.
    type Column: StorageColumn;

    /// Inserts the `Value` into the storage.
    fn put(&self, key: &[u8], column: Self::Column, value: Value) -> StorageResult<()> {
        self.write(key, column, value.as_ref()).map(|_| ())
    }

    /// Put the `Value` into the storage and return the old value.
    fn replace(
        &self,
        key: &[u8],
        column: Self::Column,
        value: Value,
    ) -> StorageResult<Option<Value>> {
        // FIXME: This is a race condition. We should use a transaction.
        let old_value = self.get(key, column)?;
        self.put(key, column, value)?;
        Ok(old_value)
    }

    /// Writes the `buf` into the storage and returns the number of written bytes.
    fn write(&self, key: &[u8], column: Self::Column, buf: &[u8])
        -> StorageResult<usize>;

    /// Removes the value from the storage and returns it.
    fn take(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        // FIXME: This is a race condition. We should use a transaction.
        let old_value = self.get(key, column)?;
        self.delete(key, column)?;
        Ok(old_value)
    }

    /// Removes the value from the storage.
    fn delete(&self, key: &[u8], column: Self::Column) -> StorageResult<()>;

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

    /// Reads the value from the storage into the `buf` and returns the number of read bytes.
    fn read(
        &self,
        key: &[u8],
        column: Self::Column,
        buf: &mut [u8],
    ) -> StorageResult<Option<usize>> {
        self.get(key, column)?
            .map(|value| {
                let read = value.len();
                if read != buf.len() {
                    return Err(StorageError::Other(anyhow::anyhow!(
                        "Buffer size is not equal to the value size"
                    )));
                }
                buf.copy_from_slice(value.as_ref());
                Ok(read)
            })
            .transpose()
    }
}

/// The operation to write into the storage.
#[derive(Debug)]
pub enum WriteOperation {
    /// Insert the value into the storage.
    Insert(Value),
    /// Remove the value from the storage.
    Remove,
}

/// The definition of the key-value store with batch operations.
#[impl_tools::autoimpl(for<T: trait> &T, &mut T, Box<T>, Arc<T>)]
pub trait BatchOperations: KeyValueStore {
    /// Writes the batch of the entries into the storage.
    // TODO: Replace `dyn Iterator` with a generic iterator when `Database` will not use `dyn BatchOperations`.
    fn batch_write(
        &self,
        entries: &mut dyn Iterator<Item = (Vec<u8>, Self::Column, WriteOperation)>,
    ) -> StorageResult<()> {
        // TODO: Optimize implementation for in-memory storages.
        for (key, column, op) in entries {
            match op {
                WriteOperation::Insert(value) => {
                    self.put(&key, column, value)?;
                }
                WriteOperation::Remove => {
                    self.delete(&key, column)?;
                }
            }
        }
        Ok(())
    }
}
