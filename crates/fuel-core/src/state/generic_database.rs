use fuel_core_storage::{
    iter::{
        BoxedIter,
        IterDirection,
        IterableStore,
    },
    kv_store::{
        KVItem,
        KeyValueInspect,
        Value,
    },
    structured_storage::StructuredStorage,
    Error as StorageError,
    Mappable,
    MerkleRoot,
    MerkleRootStorage,
    Result as StorageResult,
    StorageAsRef,
    StorageInspect,
    StorageRead,
    StorageSize,
};
use std::borrow::Cow;

#[derive(Debug, Clone)]
pub struct GenericDatabase<Storage> {
    storage: StructuredStorage<Storage>,
}

impl<Storage> GenericDatabase<Storage> {
    pub fn inner_storage(&self) -> &Storage {
        self.storage.as_ref()
    }

    pub fn from_storage(storage: Storage) -> Self {
        Self {
            storage: StructuredStorage::new(storage),
        }
    }

    pub fn into_inner(self) -> Storage {
        self.storage.into_inner()
    }
}

impl<M, Storage> StorageInspect<M> for GenericDatabase<Storage>
where
    M: Mappable,
    StructuredStorage<Storage>: StorageInspect<M, Error = StorageError>,
{
    type Error = StorageError;

    fn get(&self, key: &M::Key) -> Result<Option<Cow<M::OwnedValue>>, Self::Error> {
        self.storage.storage::<M>().get(key)
    }

    fn contains_key(&self, key: &M::Key) -> Result<bool, Self::Error> {
        self.storage.storage::<M>().contains_key(key)
    }
}

impl<Storage, M> StorageSize<M> for GenericDatabase<Storage>
where
    M: Mappable,
    StructuredStorage<Storage>: StorageSize<M, Error = StorageError>,
{
    fn size_of_value(&self, key: &M::Key) -> Result<Option<usize>, Self::Error> {
        <_ as StorageSize<M>>::size_of_value(&self.storage, key)
    }
}

impl<Storage, M> StorageRead<M> for GenericDatabase<Storage>
where
    M: Mappable,
    StructuredStorage<Storage>: StorageRead<M, Error = StorageError>,
{
    fn read(&self, key: &M::Key, buf: &mut [u8]) -> Result<Option<usize>, Self::Error> {
        self.storage.storage::<M>().read(key, buf)
    }

    fn read_alloc(&self, key: &M::Key) -> Result<Option<Vec<u8>>, Self::Error> {
        self.storage.storage::<M>().read_alloc(key)
    }
}

impl<Key, M, Storage> MerkleRootStorage<Key, M> for GenericDatabase<Storage>
where
    M: Mappable,
    StructuredStorage<Storage>: MerkleRootStorage<Key, M, Error = StorageError>,
{
    fn root(&self, key: &Key) -> Result<MerkleRoot, Self::Error> {
        self.storage.storage::<M>().root(key)
    }
}

impl<Storage> KeyValueInspect for GenericDatabase<Storage>
where
    Storage: KeyValueInspect,
{
    type Column = Storage::Column;

    fn exists(&self, key: &[u8], column: Self::Column) -> StorageResult<bool> {
        self.storage.exists(key, column)
    }

    fn size_of_value(
        &self,
        key: &[u8],
        column: Self::Column,
    ) -> StorageResult<Option<usize>> {
        KeyValueInspect::size_of_value(&self.storage, key, column)
    }

    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        KeyValueInspect::get(&self.storage, key, column)
    }

    fn read(
        &self,
        key: &[u8],
        column: Self::Column,
        buf: &mut [u8],
    ) -> StorageResult<Option<usize>> {
        KeyValueInspect::read(&self.storage, key, column, buf)
    }
}

impl<Storage> IterableStore for GenericDatabase<Storage>
where
    Storage: IterableStore,
{
    fn iter_store(
        &self,
        column: Self::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<KVItem> {
        self.storage.iter_store(column, prefix, start, direction)
    }

    fn iter_store_keys(
        &self,
        column: Self::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<fuel_core_storage::kv_store::KeyItem> {
        self.storage
            .iter_store_keys(column, prefix, start, direction)
    }
}

impl<Storage> AsRef<Storage> for GenericDatabase<Storage> {
    fn as_ref(&self) -> &Storage {
        self.storage.as_ref()
    }
}

impl<Storage> AsMut<Storage> for GenericDatabase<Storage> {
    fn as_mut(&mut self) -> &mut Storage {
        self.storage.as_mut()
    }
}

impl<Storage> core::ops::Deref for GenericDatabase<Storage> {
    type Target = Storage;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl<Storage> core::ops::DerefMut for GenericDatabase<Storage> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut()
    }
}
