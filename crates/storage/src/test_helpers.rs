//! The module to help with tests.

use crate::{
    transactional::{
        StorageTransaction,
        Transaction,
        Transactional,
    },
    Error as StorageError,
    Mappable,
    MerkleRoot,
    MerkleRootStorage,
    Result as StorageResult,
    StorageInspect,
    StorageMutate,
};

/// The empty transactional storage.
#[derive(Default, Clone, Copy, Debug)]
pub struct EmptyStorage;

impl AsRef<EmptyStorage> for EmptyStorage {
    fn as_ref(&self) -> &EmptyStorage {
        self
    }
}

impl AsMut<EmptyStorage> for EmptyStorage {
    fn as_mut(&mut self) -> &mut EmptyStorage {
        self
    }
}

impl Transactional for EmptyStorage {
    type Storage = EmptyStorage;

    fn transaction(&self) -> StorageTransaction<Self::Storage> {
        StorageTransaction::new(EmptyStorage)
    }
}

impl Transaction<EmptyStorage> for EmptyStorage {
    fn commit(&mut self) -> StorageResult<()> {
        Ok(())
    }
}

/// The trait is used to provide a generic mocked implementation for all possible `StorageInspect`,
/// `StorageMutate`, and `MerkleRootStorage` traits.
pub trait MockStorageMethods {
    /// The mocked implementation fot the `StorageInspect<M>::get` method.
    fn get<M: Mappable + 'static>(
        &self,
        key: &M::Key,
    ) -> StorageResult<Option<std::borrow::Cow<'_, M::OwnedValue>>>;

    /// The mocked implementation fot the `StorageInspect<M>::contains_key` method.
    fn contains_key<M: Mappable + 'static>(&self, key: &M::Key) -> StorageResult<bool>;

    /// The mocked implementation fot the `StorageMutate<M>::insert` method.
    fn insert<M: Mappable + 'static>(
        &mut self,
        key: &M::Key,
        value: &M::Value,
    ) -> StorageResult<Option<M::OwnedValue>>;

    /// The mocked implementation fot the `StorageMutate<M>::remove` method.
    fn remove<M: Mappable + 'static>(
        &mut self,
        key: &M::Key,
    ) -> StorageResult<Option<M::OwnedValue>>;

    /// The mocked implementation fot the `MerkleRootStorage<Key, M>::root` method.
    fn root<Key: 'static, M: Mappable + 'static>(
        &self,
        key: &Key,
    ) -> StorageResult<MerkleRoot>;
}

mockall::mock! {
    /// The mocked storage is useful to test functionality build on top of the `StorageInspect`,
    /// `StorageMutate`, and `MerkleRootStorage` traits.
    pub Storage {}

    impl MockStorageMethods for Storage {
        fn get<M: Mappable + 'static>(
            &self,
            key: &M::Key,
        ) -> StorageResult<Option<std::borrow::Cow<'static, M::OwnedValue>>>;

        fn contains_key<M: Mappable + 'static>(&self, key: &M::Key) -> StorageResult<bool>;

        fn insert<M: Mappable + 'static>(
            &mut self,
            key: &M::Key,
            value: &M::Value,
        ) -> StorageResult<Option<M::OwnedValue>>;

        fn remove<M: Mappable + 'static>(
            &mut self,
            key: &M::Key,
        ) -> StorageResult<Option<M::OwnedValue>>;

        fn root<Key: 'static, M: Mappable + 'static>(&self, key: &Key) -> StorageResult<MerkleRoot>;
    }

    impl Transactional for Storage {
        type Storage = Self;

        fn transaction(&self) -> StorageTransaction<Self>;
    }

    impl Transaction<Self> for Storage {
        fn commit(&mut self) -> StorageResult<()>;
    }
}

impl MockStorage {
    /// Packs `self` into one more `MockStorage` and implements `Transactional` trait by this move.
    pub fn into_transactional(self) -> MockStorage {
        let mut db = MockStorage::default();
        db.expect_transaction()
            .return_once(move || StorageTransaction::new(self));
        db
    }
}

impl AsRef<MockStorage> for MockStorage {
    fn as_ref(&self) -> &MockStorage {
        self
    }
}

impl AsMut<MockStorage> for MockStorage {
    fn as_mut(&mut self) -> &mut MockStorage {
        self
    }
}

impl<M> StorageInspect<M> for MockStorage
where
    M: Mappable + 'static,
{
    type Error = StorageError;

    fn get(
        &self,
        key: &M::Key,
    ) -> StorageResult<Option<std::borrow::Cow<M::OwnedValue>>> {
        MockStorageMethods::get::<M>(self, key)
    }

    fn contains_key(&self, key: &M::Key) -> StorageResult<bool> {
        MockStorageMethods::contains_key::<M>(self, key)
    }
}

impl<M> StorageMutate<M> for MockStorage
where
    M: Mappable + 'static,
{
    fn insert(
        &mut self,
        key: &M::Key,
        value: &M::Value,
    ) -> StorageResult<Option<M::OwnedValue>> {
        MockStorageMethods::insert::<M>(self, key, value)
    }

    fn remove(&mut self, key: &M::Key) -> StorageResult<Option<M::OwnedValue>> {
        MockStorageMethods::remove::<M>(self, key)
    }
}

impl<Key, M> MerkleRootStorage<Key, M> for MockStorage
where
    Key: 'static,
    M: Mappable + 'static,
{
    fn root(&self, key: &Key) -> StorageResult<MerkleRoot> {
        MockStorageMethods::root::<Key, M>(self, key)
    }
}
