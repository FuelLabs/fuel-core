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
use core::borrow::Borrow;

#[cfg(feature = "alloc")]
use alloc::{
    boxed::Box,
    collections::{
        btree_map,
        BTreeMap,
    },
    vec::Vec,
};

#[cfg(feature = "test-helpers")]
use crate::{
    iter::{
        BoxedIter,
        IterDirection,
        IterableStore,
    },
    kv_store::{
        KVItem,
        KeyItem,
    },
};

/// Provides an atomic view of the storage at the latest height at
/// the moment of view instantiation. All modifications to the storage
/// after this point will not affect the view.
pub trait AtomicView: Send + Sync {
    /// The type of the latest storage view.
    type LatestView;

    /// Returns current the view of the storage.
    fn latest_view(&self) -> StorageResult<Self::LatestView>;
}

/// Provides an atomic view of the storage at the given height.
/// The view is guaranteed to be unmodifiable for the given height.
pub trait HistoricalView: AtomicView {
    /// The type used by the storage to track the commitments at a specific height.
    type Height;
    /// The type of the storage view at `Self::Height`.
    type ViewAtHeight;

    /// Returns the latest height.
    fn latest_height(&self) -> Option<Self::Height>;

    /// Returns the view of the storage at the given `height`.
    fn view_at(&self, height: &Self::Height) -> StorageResult<Self::ViewAtHeight>;
}

/// Storage transaction on top of the storage.
pub type StorageTransaction<S> = StructuredStorage<InMemoryTransaction<S>>;

/// In memory transaction accumulates `Changes` over the storage.
#[derive(Default, Debug, Clone)]
pub struct InMemoryTransaction<S> {
    pub(crate) changes: Changes,
    pub(crate) policy: ConflictPolicy,
    pub(crate) storage: S,
}

impl<S> StorageTransaction<S> {
    /// Creates a new instance of the storage transaction.
    pub fn transaction(storage: S, policy: ConflictPolicy, changes: Changes) -> Self {
        StructuredStorage::new(InMemoryTransaction {
            changes,
            policy,
            storage,
        })
    }

    /// Creates a new instance of the structured storage with a `ConflictPolicy`.
    pub fn with_policy(self, policy: ConflictPolicy) -> Self {
        StructuredStorage::new(InMemoryTransaction {
            changes: self.inner.changes,
            policy,
            storage: self.inner.storage,
        })
    }

    /// Creates a new instance of the structured storage with a `Changes`.
    pub fn with_changes(self, changes: Changes) -> Self {
        StructuredStorage::new(InMemoryTransaction {
            changes,
            policy: self.inner.policy,
            storage: self.inner.storage,
        })
    }

    /// Getter to the changes.
    pub fn changes(&self) -> &Changes {
        &self.inner.changes
    }

    /// Returns the changes to the storage.
    pub fn into_changes(self) -> Changes {
        self.inner.changes
    }

    /// Resets the changes to the storage.
    pub fn reset_changes(&mut self) {
        self.inner.changes = Default::default();
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

/// The wrapper around the `Vec<u8>` that supports `Borrow<[u8]>`.
/// It allows the use of bytes slices to do lookups in the collections.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Ord,
    PartialOrd,
    Hash,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct ReferenceBytesKey(Vec<u8>);

impl From<Vec<u8>> for ReferenceBytesKey {
    fn from(value: Vec<u8>) -> Self {
        ReferenceBytesKey(value)
    }
}

impl From<ReferenceBytesKey> for Vec<u8> {
    fn from(value: ReferenceBytesKey) -> Self {
        value.0
    }
}

impl Borrow<[u8]> for ReferenceBytesKey {
    fn borrow(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl Borrow<Vec<u8>> for ReferenceBytesKey {
    fn borrow(&self) -> &Vec<u8> {
        &self.0
    }
}

impl AsRef<[u8]> for ReferenceBytesKey {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl AsMut<[u8]> for ReferenceBytesKey {
    fn as_mut(&mut self) -> &mut [u8] {
        self.0.as_mut_slice()
    }
}

impl core::ops::Deref for ReferenceBytesKey {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl core::ops::DerefMut for ReferenceBytesKey {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(not(feature = "std"))]
/// The type describing the list of changes to the storage.
pub type Changes = BTreeMap<u32, BTreeMap<ReferenceBytesKey, WriteOperation>>;

#[cfg(feature = "std")]
/// The type describing the list of changes to the storage.
pub type Changes =
    std::collections::HashMap<u32, BTreeMap<ReferenceBytesKey, WriteOperation>>;

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
    S: Modifiable,
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

impl<Storage> Modifiable for InMemoryTransaction<Storage> {
    fn commit_changes(&mut self, changes: Changes) -> StorageResult<()> {
        #[cfg(not(feature = "std"))]
        use btree_map::Entry;
        #[cfg(feature = "std")]
        use std::collections::hash_map::Entry;

        for (column, value) in changes.into_iter() {
            let btree = match self.changes.entry(column) {
                Entry::Vacant(vacant) => {
                    vacant.insert(value);
                    continue;
                }
                Entry::Occupied(occupied) => occupied.into_mut(),
            };

            for (k, v) in value {
                match &self.policy {
                    ConflictPolicy::Fail => {
                        let entry = btree.entry(k);

                        match entry {
                            btree_map::Entry::Occupied(occupied) => {
                                return Err(anyhow::anyhow!(
                                    "Conflicting operation {v:?} for the {:?}",
                                    occupied.key()
                                )
                                .into());
                            }
                            btree_map::Entry::Vacant(vacant) => {
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

impl<Column, S> InMemoryTransaction<S>
where
    Column: StorageColumn,
    S: KeyValueInspect<Column = Column>,
{
    fn get_from_changes(&self, key: &[u8], column: Column) -> Option<&WriteOperation> {
        self.changes
            .get(&column.id())
            .and_then(|btree| btree.get(key))
    }
}

impl<Column, S> KeyValueInspect for InMemoryTransaction<S>
where
    Column: StorageColumn,
    S: KeyValueInspect<Column = Column>,
{
    type Column = Column;

    fn exists(&self, key: &[u8], column: Self::Column) -> StorageResult<bool> {
        if let Some(operation) = self.get_from_changes(key, column) {
            match operation {
                WriteOperation::Insert(_) => Ok(true),
                WriteOperation::Remove => Ok(false),
            }
        } else {
            self.storage.exists(key, column)
        }
    }

    fn size_of_value(
        &self,
        key: &[u8],
        column: Self::Column,
    ) -> StorageResult<Option<usize>> {
        if let Some(operation) = self.get_from_changes(key, column) {
            match operation {
                WriteOperation::Insert(value) => Ok(Some(value.len())),
                WriteOperation::Remove => Ok(None),
            }
        } else {
            self.storage.size_of_value(key, column)
        }
    }

    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        if let Some(operation) = self.get_from_changes(key, column) {
            match operation {
                WriteOperation::Insert(value) => Ok(Some(value.clone())),
                WriteOperation::Remove => Ok(None),
            }
        } else {
            self.storage.get(key, column)
        }
    }

    fn read(
        &self,
        key: &[u8],
        column: Self::Column,
        buf: &mut [u8],
    ) -> StorageResult<Option<usize>> {
        if let Some(operation) = self.get_from_changes(key, column) {
            match operation {
                WriteOperation::Insert(value) => {
                    let read = value.len();
                    if read != buf.len() {
                        return Err(crate::Error::Other(anyhow::anyhow!(
                            "Buffer size is not equal to the value size"
                        )));
                    }
                    buf.copy_from_slice(value.as_ref());
                    Ok(Some(read))
                }
                WriteOperation::Remove => Ok(None),
            }
        } else {
            self.storage.read(key, column, buf)
        }
    }
}

impl<Column, S> KeyValueMutate for InMemoryTransaction<S>
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
        let k = key.to_vec().into();
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
        let k = key.to_vec().into();
        let entry = self.changes.entry(column.id()).or_default().entry(k);

        match entry {
            btree_map::Entry::Occupied(mut occupied) => {
                let old = occupied.insert(WriteOperation::Insert(value));

                match old {
                    WriteOperation::Insert(value) => Ok(Some(value)),
                    WriteOperation::Remove => Ok(None),
                }
            }
            btree_map::Entry::Vacant(vacant) => {
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
        let k = key.to_vec().into();
        self.changes
            .entry(column.id())
            .or_default()
            .insert(k, WriteOperation::Insert(Value::new(buf.to_vec())));
        Ok(buf.len())
    }

    fn take(&mut self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        let k = key.to_vec().into();
        let entry = self.changes.entry(column.id()).or_default().entry(k);

        match entry {
            btree_map::Entry::Occupied(mut occupied) => {
                let old = occupied.insert(WriteOperation::Remove);

                match old {
                    WriteOperation::Insert(value) => Ok(Some(value)),
                    WriteOperation::Remove => Ok(None),
                }
            }
            btree_map::Entry::Vacant(vacant) => {
                vacant.insert(WriteOperation::Remove);
                self.storage.get(key, column)
            }
        }
    }

    fn delete(&mut self, key: &[u8], column: Self::Column) -> StorageResult<()> {
        let k = key.to_vec().into();
        self.changes
            .entry(column.id())
            .or_default()
            .insert(k, WriteOperation::Remove);
        Ok(())
    }
}

impl<Column, S> BatchOperations for InMemoryTransaction<S>
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
            btree.insert(key.into(), operation);
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
impl<S> IterableStore for InMemoryTransaction<S>
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

    fn iter_store_keys(
        &self,
        _: Self::Column,
        _: Option<&[u8]>,
        _: Option<&[u8]>,
        _: IterDirection,
    ) -> BoxedIter<KeyItem> {
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
    #[allow(unused_imports)]
    use std::sync::Arc;

    impl<Column> Modifiable for InMemoryStorage<Column> {
        fn commit_changes(&mut self, changes: Changes) -> StorageResult<()> {
            for (column, value) in changes.into_iter() {
                for (key, value) in value {
                    match value {
                        WriteOperation::Insert(value) => {
                            self.storage.insert((column, key.into()), value);
                        }
                        WriteOperation::Remove => {
                            self.storage.remove(&(column, key.into()));
                        }
                    }
                }
            }
            Ok(())
        }
    }

    #[test]
    fn committing_changes_modifies_underlying_storage() {
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
    fn committing_changes_from_concurrent_independent_transactions_works() {
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
    fn committing_changes_from_concurrent_overlapping_transactions_fails() {
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

    #[cfg(test)]
    mod key_value_functionality {
        use super::*;
        use crate::column::Column;

        #[test]
        fn get_returns_from_view() {
            // setup
            let storage = InMemoryStorage::<Column>::default();
            let mut view = storage.read_transaction();
            let key = vec![0xA, 0xB, 0xC];
            let expected = Arc::new(vec![1, 2, 3]);
            view.put(&key, Column::Metadata, expected.clone()).unwrap();
            // test
            let ret = view.get(&key, Column::Metadata).unwrap();
            // verify
            assert_eq!(ret, Some(expected))
        }

        #[test]
        fn get_returns_from_data_store_when_key_not_in_view() {
            // setup
            let mut storage = InMemoryStorage::<Column>::default();
            let mut write = storage.write_transaction();
            let key = vec![0xA, 0xB, 0xC];
            let expected = Arc::new(vec![1, 2, 3]);
            write.put(&key, Column::Metadata, expected.clone()).unwrap();
            write.commit().unwrap();

            let view = storage.read_transaction();
            // test
            let ret = view.get(&key, Column::Metadata).unwrap();
            // verify
            assert_eq!(ret, Some(expected))
        }

        #[test]
        fn get_does_not_fetch_from_datastore_if_intentionally_deleted_from_view() {
            // setup
            let mut storage = InMemoryStorage::<Column>::default();
            let mut write = storage.write_transaction();
            let key = vec![0xA, 0xB, 0xC];
            let expected = Arc::new(vec![1, 2, 3]);
            write.put(&key, Column::Metadata, expected.clone()).unwrap();
            write.commit().unwrap();

            let mut view = storage.read_transaction();
            view.delete(&key, Column::Metadata).unwrap();
            // test
            let ret = view.get(&key, Column::Metadata).unwrap();
            let original = storage.get(&key, Column::Metadata).unwrap();
            // verify
            assert_eq!(ret, None);
            // also ensure the original value is still intact and we aren't just passing
            // through None from the data store
            assert_eq!(original, Some(expected))
        }

        #[test]
        fn can_insert_value_into_view() {
            // setup
            let mut storage = InMemoryStorage::<Column>::default();
            let mut write = storage.write_transaction();
            let expected = Arc::new(vec![1, 2, 3]);
            write
                .put(&[0xA, 0xB, 0xC], Column::Metadata, expected.clone())
                .unwrap();
            // test
            let ret = write
                .replace(&[0xA, 0xB, 0xC], Column::Metadata, Arc::new(vec![2, 4, 6]))
                .unwrap();
            // verify
            assert_eq!(ret, Some(expected))
        }

        #[test]
        fn delete_value_from_view_returns_value() {
            // setup
            let mut storage = InMemoryStorage::<Column>::default();
            let mut write = storage.write_transaction();
            let key = vec![0xA, 0xB, 0xC];
            let expected = Arc::new(vec![1, 2, 3]);
            write.put(&key, Column::Metadata, expected.clone()).unwrap();
            // test
            let ret = write.take(&key, Column::Metadata).unwrap();
            let get = write.get(&key, Column::Metadata).unwrap();
            // verify
            assert_eq!(ret, Some(expected));
            assert_eq!(get, None)
        }

        #[test]
        fn delete_returns_datastore_value_when_not_in_view() {
            // setup
            let mut storage = InMemoryStorage::<Column>::default();
            let mut write = storage.write_transaction();
            let key = vec![0xA, 0xB, 0xC];
            let expected = Arc::new(vec![1, 2, 3]);
            write.put(&key, Column::Metadata, expected.clone()).unwrap();
            write.commit().unwrap();

            let mut view = storage.read_transaction();
            // test
            let ret = view.take(&key, Column::Metadata).unwrap();
            let get = view.get(&key, Column::Metadata).unwrap();
            // verify
            assert_eq!(ret, Some(expected));
            assert_eq!(get, None)
        }

        #[test]
        fn delete_does_not_return_datastore_value_when_deleted_twice() {
            // setup
            let mut storage = InMemoryStorage::<Column>::default();
            let mut write = storage.write_transaction();
            let key = vec![0xA, 0xB, 0xC];
            let expected = Arc::new(vec![1, 2, 3]);
            write.put(&key, Column::Metadata, expected.clone()).unwrap();
            write.commit().unwrap();

            let mut view = storage.read_transaction();
            // test
            let ret1 = view.take(&key, Column::Metadata).unwrap();
            let ret2 = view.take(&key, Column::Metadata).unwrap();
            let get = view.get(&key, Column::Metadata).unwrap();
            // verify
            assert_eq!(ret1, Some(expected));
            assert_eq!(ret2, None);
            assert_eq!(get, None)
        }

        #[test]
        fn exists_checks_view_values() {
            // setup
            let mut storage = InMemoryStorage::<Column>::default();
            let mut write = storage.write_transaction();
            let key = vec![0xA, 0xB, 0xC];
            let expected = Arc::new(vec![1, 2, 3]);
            write.put(&key, Column::Metadata, expected).unwrap();
            // test
            let ret = write.exists(&key, Column::Metadata).unwrap();
            // verify
            assert!(ret)
        }

        #[test]
        fn exists_checks_data_store_when_not_in_view() {
            // setup
            let mut storage = InMemoryStorage::<Column>::default();
            let mut write = storage.write_transaction();
            let key = vec![0xA, 0xB, 0xC];
            let expected = Arc::new(vec![1, 2, 3]);
            write.put(&key, Column::Metadata, expected).unwrap();
            write.commit().unwrap();

            let view = storage.read_transaction();
            // test
            let ret = view.exists(&key, Column::Metadata).unwrap();
            // verify
            assert!(ret)
        }

        #[test]
        fn exists_does_not_check_data_store_after_intentional_removal_from_view() {
            // setup
            let mut storage = InMemoryStorage::<Column>::default();
            let mut write = storage.write_transaction();
            let key = vec![0xA, 0xB, 0xC];
            let expected = Arc::new(vec![1, 2, 3]);
            write.put(&key, Column::Metadata, expected).unwrap();
            write.commit().unwrap();

            let mut view = storage.read_transaction();
            view.delete(&key, Column::Metadata).unwrap();
            // test
            let ret = view.exists(&key, Column::Metadata).unwrap();
            let original = storage.exists(&key, Column::Metadata).unwrap();
            // verify
            assert!(!ret);
            // also ensure the original value is still intact and we aren't just passing
            // through None from the data store
            assert!(original)
        }

        #[test]
        fn commit_applies_puts() {
            // setup
            let mut storage = InMemoryStorage::<Column>::default();
            let mut write = storage.write_transaction();
            let key = vec![0xA, 0xB, 0xC];
            let expected = Arc::new(vec![1, 2, 3]);
            write.put(&key, Column::Metadata, expected.clone()).unwrap();
            // test
            write.commit().unwrap();
            let ret = storage.get(&key, Column::Metadata).unwrap();
            // verify
            assert_eq!(ret, Some(expected))
        }

        #[test]
        fn commit_applies_deletes() {
            // setup
            let mut storage = InMemoryStorage::<Column>::default();
            let mut write = storage.write_transaction();
            let key = vec![0xA, 0xB, 0xC];
            let expected = Arc::new(vec![1, 2, 3]);
            write.put(&key, Column::Metadata, expected).unwrap();
            write.commit().unwrap();

            let mut view = storage.write_transaction();
            // test
            view.delete(&key, Column::Metadata).unwrap();
            view.commit().unwrap();
            let ret = storage.get(&key, Column::Metadata).unwrap();
            // verify
            assert_eq!(ret, None)
        }

        #[test]
        fn can_use_unit_value() {
            let key = vec![0x00];

            let mut storage = InMemoryStorage::<Column>::default();
            let mut write = storage.write_transaction();
            let expected = Arc::new(vec![]);
            write.put(&key, Column::Metadata, expected.clone()).unwrap();

            assert_eq!(
                write.get(&key, Column::Metadata).unwrap().unwrap(),
                expected
            );

            assert!(write.exists(&key, Column::Metadata).unwrap());

            assert_eq!(
                write.take(&key, Column::Metadata).unwrap().unwrap(),
                expected
            );

            assert!(!write.exists(&key, Column::Metadata).unwrap());

            write.commit().unwrap();

            assert!(!storage.exists(&key, Column::Metadata).unwrap());

            let mut storage = InMemoryStorage::<Column>::default();
            let mut write = storage.write_transaction();
            write.put(&key, Column::Metadata, expected.clone()).unwrap();
            write.commit().unwrap();

            assert_eq!(
                storage.get(&key, Column::Metadata).unwrap().unwrap(),
                expected
            );
        }

        #[test]
        fn can_use_unit_key() {
            let key: Vec<u8> = Vec::with_capacity(0);

            let mut storage = InMemoryStorage::<Column>::default();
            let mut write = storage.write_transaction();
            let expected = Arc::new(vec![]);
            write.put(&key, Column::Metadata, expected.clone()).unwrap();

            assert_eq!(
                write.get(&key, Column::Metadata).unwrap().unwrap(),
                expected
            );

            assert!(write.exists(&key, Column::Metadata).unwrap());

            assert_eq!(
                write.take(&key, Column::Metadata).unwrap().unwrap(),
                expected
            );

            assert!(!write.exists(&key, Column::Metadata).unwrap());

            write.commit().unwrap();

            assert!(!storage.exists(&key, Column::Metadata).unwrap());

            let mut storage = InMemoryStorage::<Column>::default();
            let mut write = storage.write_transaction();
            write.put(&key, Column::Metadata, expected.clone()).unwrap();
            write.commit().unwrap();

            assert_eq!(
                storage.get(&key, Column::Metadata).unwrap().unwrap(),
                expected
            );
        }

        #[test]
        fn can_use_unit_key_and_value() {
            let key: Vec<u8> = Vec::with_capacity(0);

            let mut storage = InMemoryStorage::<Column>::default();
            let mut write = storage.write_transaction();
            let expected = Arc::new(vec![]);
            write.put(&key, Column::Metadata, expected.clone()).unwrap();

            assert_eq!(
                write.get(&key, Column::Metadata).unwrap().unwrap(),
                expected
            );

            assert!(write.exists(&key, Column::Metadata).unwrap());

            assert_eq!(
                write.take(&key, Column::Metadata).unwrap().unwrap(),
                expected
            );

            assert!(!write.exists(&key, Column::Metadata).unwrap());

            write.commit().unwrap();

            assert!(!storage.exists(&key, Column::Metadata).unwrap());

            let mut storage = InMemoryStorage::<Column>::default();
            let mut write = storage.write_transaction();
            write.put(&key, Column::Metadata, expected.clone()).unwrap();
            write.commit().unwrap();

            assert_eq!(
                storage.get(&key, Column::Metadata).unwrap().unwrap(),
                expected
            );
        }
    }
}
