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
        btree_map::Entry,
        BTreeMap,
        HashMap,
    },
    sync::Arc,
};

#[cfg(feature = "test-helpers")]
use crate::{
    iter::{
        BoxedIter,
        IterDirection,
        IterableStore,
    },
    kv_store::KVItem,
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
    /// Creates a new instance of the storage transaction.
    pub fn transaction(storage: S, policy: ConflictPolicy, changes: Changes) -> Self {
        StructuredStorage::new(StorageTransactionInner {
            changes,
            policy,
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
pub type Changes = HashMap<u32, BTreeMap<Vec<u8>, WriteOperation>>;

impl<Storage> From<StorageTransaction<Storage>> for Changes {
    fn from(transaction: StorageTransaction<Storage>) -> Self {
        transaction.into_changes()
    }
}

/// The trait to convert the type into the storage transaction.
pub trait IntoTransaction: Sized {
    /// Converts the type into the storage transaction consuming it.
    fn into_transaction(self) -> StorageTransaction<Self>;
}

impl<S> IntoTransaction for S
where
    S: KeyValueInspect,
{
    fn into_transaction(self) -> StorageTransaction<Self> {
        StorageTransaction::transaction(
            self,
            ConflictPolicy::Overwrite,
            Default::default(),
        )
    }
}

/// Creates a new instance of the storage read transaction.
pub trait ReadTransaction {
    /// Returns the read transaction without ability to commit the changes.
    fn read_transaction(&self) -> StorageTransaction<&Self>;
}

impl<S> ReadTransaction for S
where
    S: KeyValueInspect,
{
    fn read_transaction(&self) -> StorageTransaction<&Self> {
        StorageTransaction::transaction(
            self,
            ConflictPolicy::Overwrite,
            Default::default(),
        )
    }
}

/// Creates a new instance of the storage write transaction.
pub trait WriteTransaction {
    /// Returns the write transaction that can commit the changes.
    fn write_transaction(&mut self) -> StorageTransaction<&mut Self>;
}

impl<S> WriteTransaction for S
where
    S: KeyValueInspect + Modifiable,
{
    fn write_transaction(&mut self) -> StorageTransaction<&mut Self> {
        StorageTransaction::transaction(
            self,
            ConflictPolicy::Overwrite,
            Default::default(),
        )
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
        for (column, value) in changes.into_iter() {
            let btree = self.changes.entry(column).or_default();
            for (k, v) in value {
                match &self.policy {
                    ConflictPolicy::Fail => {
                        let entry = btree.entry(k);

                        match entry {
                            Entry::Occupied(occupied) => {
                                return Err(anyhow::anyhow!(
                                    "Conflicting operation {v:?} for the {:?}",
                                    occupied.key()
                                )
                                .into());
                            }
                            Entry::Vacant(vacant) => {
                                vacant.insert(v);
                            }
                        }
                    }
                    ConflictPolicy::Overwrite => {
                        btree.insert(k, v);
                    }
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
        if let Some(operation) = self
            .changes
            .get(&column.id())
            .and_then(|btree| btree.get(&k))
        {
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
            .entry(column.id())
            .or_default()
            .insert(k, WriteOperation::Insert(value));
        Ok(())
    }

    fn replace(
        &mut self,
        key: &[u8],
        column: Self::Column,
        value: Value,
    ) -> StorageResult<Option<Value>> {
        let k = key.to_vec();
        let entry = self.changes.entry(column.id()).or_default().entry(k);

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
        self.changes
            .entry(column.id())
            .or_default()
            .insert(k, WriteOperation::Insert(Arc::new(buf.to_vec())));
        Ok(buf.len())
    }

    fn take(&mut self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        let k = key.to_vec();
        let entry = self.changes.entry(column.id()).or_default().entry(k);

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
            .entry(column.id())
            .or_default()
            .insert(k, WriteOperation::Remove);
        Ok(())
    }
}

impl<Column, S> BatchOperations for StorageTransactionInner<S>
where
    Column: StorageColumn,
    S: KeyValueInspect<Column = Column>,
{
    fn batch_write<I>(&mut self, column: Column, entries: I) -> StorageResult<()>
    where
        I: Iterator<Item = (Vec<u8>, WriteOperation)>,
    {
        let btree = self.changes.entry(column.id()).or_default();
        entries.for_each(|(key, operation)| {
            btree.insert(key, operation);
        });
        Ok(())
    }
}

// The `IterableStore` should only be implemented for the actual storage,
// not the storage transaction, to maximize the performance.
//
// Another reason is that only actual storage with finalized updates should be
// used to iterate over its entries to avoid inconsistent results.
//
// We implement `IterableStore` for `StorageTransactionInner` only to allow
// using it in the tests and benchmarks as a type(not even implementation of it).
#[cfg(feature = "test-helpers")]
impl<S> IterableStore for StorageTransactionInner<S>
where
    S: IterableStore,
{
    fn iter_store(
        &self,
        _: Self::Column,
        _: Option<&[u8]>,
        _: Option<&[u8]>,
        _: IterDirection,
    ) -> BoxedIter<KVItem> {
        unimplemented!()
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
            for (column, value) in changes.into_iter() {
                for (key, value) in value {
                    match value {
                        WriteOperation::Insert(value) => {
                            self.storage.insert((column, key), value);
                        }
                        WriteOperation::Remove => {
                            self.storage.remove(&(column, key));
                        }
                    }
                }
            }
            Ok(())
        }
    }

    #[test]
    fn modification_works() {
        let mut storage = InMemoryStorage::default();
        let mut transaction = storage.write_transaction();

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
        let mut transactions =
            storage.into_transaction().with_policy(ConflictPolicy::Fail);

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
        let mut transactions =
            storage.into_transaction().with_policy(ConflictPolicy::Fail);

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
