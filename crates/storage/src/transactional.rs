//! The primitives to work with storage in transactional mode.

use crate::{
    kv_store::{
        BatchOperations,
        KeyValueInspect,
        KeyValueMutate,
        StorageColumn,
        Value,
        WriteOperation,
    },
    structured_storage::StructuredStorage,
    Result as StorageResult,
};
use std::{
    collections::{
        hash_map::Entry,
        HashMap,
    },
    sync::Arc,
};

/// Provides a view of the storage at the given height.
/// It guarantees to be atomic, meaning the view is immutable to outside modifications.
pub trait AtomicView: Send + Sync {
    /// The type of the storage view.
    type View;

    /// The type used by the storage to track the commitments at a specific height.
    type Height;

    /// Returns the latest block height.
    fn latest_height(&self) -> Self::Height;

    /// Returns the view of the storage at the given `height`.
    fn view_at(&self, height: &Self::Height) -> StorageResult<Self::View>;

    /// Returns the view of the storage for the latest block height.
    fn latest_view(&self) -> Self::View;
}

/// Storage transaction on top of the storage.
pub type StorageTransaction<S> = StructuredStorage<StorageTransactionInner<S>>;

/// The inner representation of the storage transaction.
#[doc(hidden)]
#[derive(Default, Debug, Clone)]
pub struct StorageTransactionInner<S> {
    pub(crate) changes: Changes,
    pub(crate) policy: ConflictPolicy,
    pub(crate) storage: S,
}

impl<S> StorageTransaction<S> {
    /// Creates a new instance of the structured storage.
    pub fn new_transaction(storage: S) -> Self {
        StructuredStorage::new(StorageTransactionInner {
            changes: Default::default(),
            policy: ConflictPolicy::Overwrite,
            storage,
        })
    }

    /// Creates a new instance of the structured storage with a `ConflictPolicy`.
    pub fn with_policy(self, policy: ConflictPolicy) -> Self {
        StructuredStorage::new(StorageTransactionInner {
            changes: self.inner.changes,
            policy,
            storage: self.inner.storage,
        })
    }

    /// Creates a new instance of the structured storage with a `Changes`.
    pub fn with_changes(self, changes: Changes) -> Self {
        StructuredStorage::new(StorageTransactionInner {
            changes,
            policy: self.inner.policy,
            storage: self.inner.storage,
        })
    }

    /// Returns the changes to the storage.
    pub fn into_changes(self) -> Changes {
        self.inner.changes
    }
}

/// The policy to resolve the conflict during committing of the changes.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictPolicy {
    /// The transaction will fail if there is a conflict.
    Fail,
    /// The transaction will overwrite the conflicting data.
    #[default]
    Overwrite,
}

/// The type is modifiable and may commit the changes into the storage.
#[impl_tools::autoimpl(for<T: trait> &mut T, Box<T>)]
pub trait Modifiable {
    /// Commits the changes into the storage.
    fn commit_changes(&mut self, changes: Changes) -> StorageResult<()>;
}

/// The type describing the list of changes to the storage.
pub type Changes = HashMap<(u32, Vec<u8>), WriteOperation>;

impl<Storage> From<StorageTransaction<Storage>> for Changes {
    fn from(transaction: StorageTransaction<Storage>) -> Self {
        transaction.into_changes()
    }
}

impl<Storage> StorageTransaction<Storage> {
    /// Returns the read transaction without ability to commit the changes.
    pub fn read_transaction(&self) -> StorageTransaction<&Self> {
        StorageTransaction::new_transaction(self)
    }

    /// Returns the write transaction that can commit the changes.
    pub fn write_transaction(&mut self) -> StorageTransaction<&mut Self> {
        StorageTransaction::new_transaction(self)
    }
}

impl<Storage> StorageTransaction<Storage>
where
    Storage: Modifiable,
{
    /// Commits the changes into the storage.
    pub fn commit(mut self) -> StorageResult<Storage> {
        let changes = core::mem::take(&mut self.inner.changes);
        self.inner.storage.commit_changes(changes)?;
        Ok(self.inner.storage)
    }
}

impl<Storage> Modifiable for StorageTransactionInner<Storage> {
    fn commit_changes(&mut self, changes: Changes) -> StorageResult<()> {
        for (key, value) in changes.into_iter() {
            match &self.policy {
                ConflictPolicy::Fail => {
                    let entry = self.changes.entry(key);

                    match entry {
                        Entry::Occupied(occupied) => {
                            return Err(anyhow::anyhow!(
                                "Conflicting operation {value:?} for the {:?}",
                                occupied.key()
                            )
                            .into());
                        }
                        Entry::Vacant(vacant) => {
                            vacant.insert(value);
                        }
                    }
                }
                ConflictPolicy::Overwrite => {
                    self.changes.insert(key, value);
                }
            }
        }
        Ok(())
    }
}

impl<Column, S> KeyValueInspect for StorageTransactionInner<S>
where
    Column: StorageColumn,
    S: KeyValueInspect<Column = Column>,
{
    type Column = Column;

    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        let k = key.to_vec();
        if let Some(operation) = self.changes.get(&(column.id(), k)) {
            match operation {
                WriteOperation::Insert(value) => Ok(Some(value.clone())),
                WriteOperation::Remove => Ok(None),
            }
        } else {
            self.storage.get(key, column)
        }
    }
}

impl<Column, S> KeyValueMutate for StorageTransactionInner<S>
where
    Column: StorageColumn,
    S: KeyValueInspect<Column = Column>,
{
    fn put(
        &mut self,
        key: &[u8],
        column: Self::Column,
        value: Value,
    ) -> StorageResult<()> {
        let k = key.to_vec();
        self.changes
            .insert((column.id(), k), WriteOperation::Insert(value));
        Ok(())
    }

    fn replace(
        &mut self,
        key: &[u8],
        column: Self::Column,
        value: Value,
    ) -> StorageResult<Option<Value>> {
        let k = key.to_vec();
        let entry = self.changes.entry((column.id(), k));

        match entry {
            Entry::Occupied(mut occupied) => {
                let old = occupied.insert(WriteOperation::Insert(value));

                match old {
                    WriteOperation::Insert(value) => Ok(Some(value)),
                    WriteOperation::Remove => Ok(None),
                }
            }
            Entry::Vacant(vacant) => {
                vacant.insert(WriteOperation::Insert(value));
                self.storage.get(key, column)
            }
        }
    }

    fn write(
        &mut self,
        key: &[u8],
        column: Self::Column,
        buf: &[u8],
    ) -> StorageResult<usize> {
        let k = key.to_vec();
        self.changes.insert(
            (column.id(), k),
            WriteOperation::Insert(Arc::new(buf.to_vec())),
        );
        Ok(buf.len())
    }

    fn take(&mut self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        let k = key.to_vec();
        let entry = self.changes.entry((column.id(), k));

        match entry {
            Entry::Occupied(mut occupied) => {
                let old = occupied.insert(WriteOperation::Remove);

                match old {
                    WriteOperation::Insert(value) => Ok(Some(value)),
                    WriteOperation::Remove => Ok(None),
                }
            }
            Entry::Vacant(vacant) => {
                vacant.insert(WriteOperation::Remove);
                self.storage.get(key, column)
            }
        }
    }

    fn delete(&mut self, key: &[u8], column: Self::Column) -> StorageResult<()> {
        let k = key.to_vec();
        self.changes
            .insert((column.id(), k), WriteOperation::Remove);
        Ok(())
    }
}

impl<Column, S> BatchOperations for StorageTransactionInner<S>
where
    Column: StorageColumn,
    S: KeyValueInspect<Column = Column>,
{
    fn batch_write<I>(&mut self, entries: I) -> StorageResult<()>
    where
        I: Iterator<Item = (Vec<u8>, Column, WriteOperation)>,
    {
        for (key, column, op) in entries {
            self.changes.insert((column.id(), key), op);
        }
        Ok(())
    }
}

#[cfg(feature = "test-helpers")]
mod test {
    use super::*;
    use crate::structured_storage::test::InMemoryStorage;
    #[cfg(test)]
    use crate::{
        tables::Messages,
        StorageAsMut,
    };

    impl<Column> Modifiable for InMemoryStorage<Column> {
        fn commit_changes(&mut self, changes: Changes) -> StorageResult<()> {
            for (key, value) in changes.into_iter() {
                match value {
                    WriteOperation::Insert(value) => {
                        self.storage.insert(key, value);
                    }
                    WriteOperation::Remove => {
                        self.storage.remove(&key);
                    }
                }
            }
            Ok(())
        }
    }

    #[test]
    fn modification_works() {
        let mut storage = InMemoryStorage::default();
        let mut transaction = StorageTransaction::new_transaction(&mut storage);

        let mut sub_transaction = transaction.write_transaction();

        sub_transaction
            .storage_as_mut::<Messages>()
            .insert(&Default::default(), &Default::default())
            .expect("Should work");

        sub_transaction.commit().unwrap();
        transaction.commit().unwrap();
        assert_eq!(storage.storage().len(), 1);
    }

    #[test]
    fn concurrency_independent_modifications_for_many_transactions_works() {
        let storage = InMemoryStorage::default();
        let mut transactions = StorageTransaction::new_transaction(storage)
            .with_policy(ConflictPolicy::Fail);

        let mut sub_transaction1 = transactions.read_transaction();
        let mut sub_transaction2 = transactions.read_transaction();

        sub_transaction1
            .storage_as_mut::<Messages>()
            .insert(&[0u8; 32].into(), &Default::default())
            .expect("Should work");
        sub_transaction2
            .storage_as_mut::<Messages>()
            .insert(&[1u8; 32].into(), &Default::default())
            .expect("Should work");

        let changes1 = sub_transaction1.into();
        let changes2 = sub_transaction2.into();
        transactions.commit_changes(changes1).unwrap();
        transactions.commit_changes(changes2).unwrap();
    }

    #[test]
    fn concurrency_overlapping_modifications_for_many_transactions_fails() {
        let storage = InMemoryStorage::default();
        let mut transactions = StorageTransaction::new_transaction(storage)
            .with_policy(ConflictPolicy::Fail);

        let mut sub_transaction1 = transactions.read_transaction();
        let mut sub_transaction2 = transactions.read_transaction();

        sub_transaction1
            .storage_as_mut::<Messages>()
            .insert(&[0u8; 32].into(), &Default::default())
            .expect("Should work");
        sub_transaction2
            .storage_as_mut::<Messages>()
            .insert(&[0u8; 32].into(), &Default::default())
            .expect("Should work");

        let changes1 = sub_transaction1.into();
        let changes2 = sub_transaction2.into();
        transactions.commit_changes(changes1).unwrap();
        transactions
            .commit_changes(changes2)
            .expect_err("Should fails because of the modification for the same key");
    }
}
