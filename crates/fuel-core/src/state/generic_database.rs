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
    tables::BlobData,
    Error as StorageError,
    Mappable,
    MerkleRoot,
    MerkleRootStorage,
    PredicateStorageRequirements,
    Result as StorageResult,
    StorageAsRef,
    StorageInspect,
    StorageRead,
    StorageSize,
};
use std::{
    borrow::Cow,
    fmt::Debug,
};

#[derive(Debug, Clone)]
pub struct GenericDatabase<Storage, Metadata> {
    storage: StructuredStorage<Storage>,
    metadata: Option<Metadata>,
}

impl<Storage, Metadata> GenericDatabase<Storage, Metadata> {
    pub fn inner_storage(&self) -> &Storage {
        self.storage.as_ref()
    }

    pub fn metadata(&self) -> Option<&Metadata> {
        self.metadata.as_ref()
    }

    pub fn from_storage_and_metadata(
        storage: Storage,
        metadata: Option<Metadata>,
    ) -> Self {
        Self {
            storage: StructuredStorage::new(storage),
            metadata,
        }
    }

    pub fn into_inner(self) -> (Storage, Option<Metadata>) {
        (self.storage.into_storage(), self.metadata)
    }
}

impl<M, Storage, Metadata> StorageInspect<M> for GenericDatabase<Storage, Metadata>
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

impl<M, Storage, Metadata> StorageSize<M> for GenericDatabase<Storage, Metadata>
where
    M: Mappable,
    StructuredStorage<Storage>: StorageSize<M, Error = StorageError>,
{
    fn size_of_value(&self, key: &M::Key) -> Result<Option<usize>, Self::Error> {
        <_ as StorageSize<M>>::size_of_value(&self.storage, key)
    }
}

impl<M, Storage, Metadata> StorageRead<M> for GenericDatabase<Storage, Metadata>
where
    M: Mappable,
    StructuredStorage<Storage>: StorageRead<M, Error = StorageError>,
{
    fn read(
        &self,
        key: &M::Key,
        offset: usize,
        buf: &mut [u8],
    ) -> Result<bool, Self::Error> {
        self.storage.storage::<M>().read(key, offset, buf)
    }

    fn read_alloc(&self, key: &M::Key) -> Result<Option<Vec<u8>>, Self::Error> {
        self.storage.storage::<M>().read_alloc(key)
    }
}

impl<Key, M, Storage, Metadata> MerkleRootStorage<Key, M>
    for GenericDatabase<Storage, Metadata>
where
    M: Mappable,
    StructuredStorage<Storage>: MerkleRootStorage<Key, M, Error = StorageError>,
{
    fn root(&self, key: &Key) -> Result<MerkleRoot, Self::Error> {
        self.storage.storage::<M>().root(key)
    }
}

impl<Storage, Metadata> KeyValueInspect for GenericDatabase<Storage, Metadata>
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
        offset: usize,
        buf: &mut [u8],
    ) -> StorageResult<bool> {
        KeyValueInspect::read(&self.storage, key, column, offset, buf)
    }
}

impl<Storage, Metadata> IterableStore for GenericDatabase<Storage, Metadata>
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

impl<Storage, Metadata> AsRef<Storage> for GenericDatabase<Storage, Metadata> {
    fn as_ref(&self) -> &Storage {
        self.storage.as_ref()
    }
}

impl<Storage, Metadata> AsMut<Storage> for GenericDatabase<Storage, Metadata> {
    fn as_mut(&mut self) -> &mut Storage {
        self.storage.as_mut()
    }
}

impl<Storage, Metadata> core::ops::Deref for GenericDatabase<Storage, Metadata> {
    type Target = Storage;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl<Storage, Metadata> core::ops::DerefMut for GenericDatabase<Storage, Metadata> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut()
    }
}

impl<Storage, Error, Metadata> PredicateStorageRequirements
    for GenericDatabase<Storage, Metadata>
where
    Self: StorageRead<BlobData, Error = Error>,
    Error: Debug,
{
    fn storage_error_to_string(error: Error) -> String {
        format!("{:?}", error)
    }
}
