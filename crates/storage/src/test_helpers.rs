//! The module to help with tests.

use crate::{
    transactional::{
        Changes,
        Modifiable,
    },
    Error as StorageError,
    Mappable,
    MerkleRoot,
    MerkleRootStorage,
    Result as StorageResult,
    StorageInspect,
    StorageMutate,
};

/// The trait is used to provide a generic mocked implementation for all possible `StorageInspect`,
/// `StorageMutate`, and `MerkleRootStorage` traits.
pub trait MockStorageMethods {
    /// The mocked implementation for the `StorageInspect<M>::get` method.
    fn get<M: Mappable + 'static>(
        &self,
        key: &M::Key,
    ) -> StorageResult<Option<std::borrow::Cow<'_, M::OwnedValue>>>;

    /// The mocked implementation for the `StorageInspect<M>::contains_key` method.
    fn contains_key<M: Mappable + 'static>(&self, key: &M::Key) -> StorageResult<bool>;

    /// The mocked implementation for the `StorageMutate<M>::insert` method.
    fn insert<M: Mappable + 'static>(
        &mut self,
        key: &M::Key,
        value: &M::Value,
    ) -> StorageResult<Option<M::OwnedValue>>;

    /// The mocked implementation for the `StorageMutate<M>::remove` method.
    fn remove<M: Mappable + 'static>(
        &mut self,
        key: &M::Key,
    ) -> StorageResult<Option<M::OwnedValue>>;

    /// The mocked implementation for the `MerkleRootStorage<Key, M>::root` method.
    fn root<Key: 'static, M: Mappable + 'static>(
        &self,
        key: &Key,
    ) -> StorageResult<MerkleRoot>;
}

/// The mocked storage is useful to test functionality build on top of the `StorageInspect`,
/// `StorageMutate`, and `MerkleRootStorage` traits.
#[derive(Default, Debug, Clone)]
pub struct MockStorage<Storage, Data> {
    /// The mocked storage.
    pub storage: Storage,
    /// Additional data to be used in the tests.
    pub data: Data,
}

mockall::mock! {
    /// The basic mocked storage
    pub Basic {}

    impl MockStorageMethods for Basic {
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

    impl Modifiable for Basic {
        fn commit_changes(&mut self, changes: Changes) -> StorageResult<()>;
    }
}

impl<M, Storage, Data> StorageInspect<M> for MockStorage<Storage, Data>
where
    M: Mappable + 'static,
    Storage: MockStorageMethods,
{
    type Error = StorageError;

    fn get(
        &self,
        key: &M::Key,
    ) -> StorageResult<Option<std::borrow::Cow<M::OwnedValue>>> {
        MockStorageMethods::get::<M>(&self.storage, key)
    }

    fn contains_key(&self, key: &M::Key) -> StorageResult<bool> {
        MockStorageMethods::contains_key::<M>(&self.storage, key)
    }
}

impl<M, Storage, Data> StorageMutate<M> for MockStorage<Storage, Data>
where
    M: Mappable + 'static,
    Storage: MockStorageMethods,
{
    fn replace(
        &mut self,
        key: &M::Key,
        value: &M::Value,
    ) -> StorageResult<Option<M::OwnedValue>> {
        MockStorageMethods::insert::<M>(&mut self.storage, key, value)
    }

    fn take(&mut self, key: &M::Key) -> StorageResult<Option<M::OwnedValue>> {
        MockStorageMethods::remove::<M>(&mut self.storage, key)
    }
}

impl<Key, M, Storage, Data> MerkleRootStorage<Key, M> for MockStorage<Storage, Data>
where
    Key: 'static,
    M: Mappable + 'static,
    Storage: MockStorageMethods,
{
    fn root(&self, key: &Key) -> StorageResult<MerkleRoot> {
        MockStorageMethods::root::<Key, M>(&self.storage, key)
    }
}
