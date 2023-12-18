use crate::database::{
    Column,
    Database,
    Error as DatabaseError,
    Result as DatabaseResult,
};
use fuel_core_storage::iter::{
    BoxedIter,
    IterDirection,
};
use std::{
    fmt::Debug,
    sync::Arc,
};

pub type DataSource = Arc<dyn TransactableStorage<Column = Column>>;
pub type Value = Arc<Vec<u8>>;
pub type KVItem = DatabaseResult<(Vec<u8>, Value)>;

/// A column of the storage.
pub trait StorageColumn: Clone {
    /// Returns the name of the column.
    fn name(&self) -> &'static str;

    /// Returns the id of the column.
    fn id(&self) -> u32;
}

pub trait KeyValueStore {
    /// The type of the column.
    type Column: StorageColumn;

    /// Inserts the `Value` into the storage.
    fn put(&self, key: &[u8], column: Self::Column, value: Value) -> DatabaseResult<()> {
        self.write(key, column, value.as_ref()).map(|_| ())
    }

    /// Put the `Value` into the storage and return the old value.
    fn replace(
        &self,
        key: &[u8],
        column: Self::Column,
        value: Value,
    ) -> DatabaseResult<Option<Value>> {
        // FIXME: This is a race condition. We should use a transaction.
        let old_value = self.get(key, column.clone())?;
        self.put(key, column, value)?;
        Ok(old_value)
    }

    /// Writes the `buf` into the storage and returns the number of written bytes.
    fn write(
        &self,
        key: &[u8],
        column: Self::Column,
        buf: &[u8],
    ) -> DatabaseResult<usize>;

    /// Removes the value from the storage and returns it.
    fn take(&self, key: &[u8], column: Self::Column) -> DatabaseResult<Option<Value>> {
        // FIXME: This is a race condition. We should use a transaction.
        let old_value = self.get(key, column.clone())?;
        self.delete(key, column)?;
        Ok(old_value)
    }

    /// Removes the value from the storage.
    fn delete(&self, key: &[u8], column: Self::Column) -> DatabaseResult<()>;

    /// Checks if the value exists in the storage.
    fn exists(&self, key: &[u8], column: Self::Column) -> DatabaseResult<bool> {
        Ok(self.size_of_value(key, column)?.is_some())
    }

    /// Returns the size of the value in the storage.
    fn size_of_value(
        &self,
        key: &[u8],
        column: Self::Column,
    ) -> DatabaseResult<Option<usize>> {
        Ok(self.get(key, column.clone())?.map(|value| value.len()))
    }

    /// Returns the value from the storage.
    fn get(&self, key: &[u8], column: Self::Column) -> DatabaseResult<Option<Value>>;

    /// Reads the value from the storage into the `buf` and returns the number of read bytes.
    fn read(
        &self,
        key: &[u8],
        column: Self::Column,
        mut buf: &mut [u8],
    ) -> DatabaseResult<Option<usize>> {
        self.get(key, column.clone())?
            .map(|value| {
                let read = value.len();
                std::io::Write::write_all(&mut buf, value.as_ref())
                    .map_err(|e| DatabaseError::Other(anyhow::anyhow!(e)))?;
                Ok(read)
            })
            .transpose()
    }

    /// Returns an iterator over the values in the storage.
    fn iter_all(
        &self,
        column: Self::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<KVItem>;
}

pub trait BatchOperations: KeyValueStore {
    fn batch_write(
        &self,
        entries: &mut dyn Iterator<Item = (Vec<u8>, Self::Column, WriteOperation)>,
    ) -> DatabaseResult<()> {
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

#[derive(Debug)]
pub enum WriteOperation {
    Insert(Value),
    Remove,
}

pub trait TransactableStorage: BatchOperations + Debug + Send + Sync {
    fn checkpoint(&self) -> DatabaseResult<Database> {
        Err(DatabaseError::Other(anyhow::anyhow!(
            "Checkpoint is not supported"
        )))
    }

    fn flush(&self) -> DatabaseResult<()>;
}

pub mod in_memory;
#[cfg(feature = "rocksdb")]
pub mod rocks_db;
