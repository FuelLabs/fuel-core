use crate::database::{
    database_description::DatabaseDescription,
    Database,
};
use fuel_core_storage::{
    structured_storage::StructuredStorage,
    transactional::{
        ConflictPolicy,
        Modifiable,
        ReadTransaction,
        StorageTransaction,
    },
    Error as StorageError,
    Mappable,
    MerkleRoot,
    MerkleRootStorage,
    Result as StorageResult,
    StorageAsMut,
    StorageAsRef,
    StorageBatchMutate,
    StorageInspect,
    StorageMutate,
    StorageRead,
    StorageSize,
};
use std::borrow::Cow;

impl<Description, M> StorageInspect<M> for Database<Description>
where
    Description: DatabaseDescription,
    M: Mappable,
    for<'a> StructuredStorage<&'a Self>: StorageInspect<M, Error = StorageError>,
{
    type Error = StorageError;

    fn get(&self, key: &M::Key) -> StorageResult<Option<Cow<M::OwnedValue>>> {
        let storage = StructuredStorage::new(self);
        let value = storage.storage::<M>().get(key)?;

        if let Some(cow) = value {
            Ok(Some(Cow::Owned(cow.into_owned())))
        } else {
            Ok(None)
        }
    }

    fn contains_key(&self, key: &M::Key) -> StorageResult<bool> {
        StructuredStorage::new(self)
            .storage::<M>()
            .contains_key(key)
    }
}

// TODO: After https://github.com/FuelLabs/fuel-vm/pull/679 implement it only for `feature = "test-helpers"`
impl<Description, M> StorageMutate<M> for Database<Description>
where
    Description: DatabaseDescription,
    M: Mappable,
    for<'a> StructuredStorage<&'a Self>: StorageInspect<M, Error = StorageError>,
    for<'a> StorageTransaction<&'a Self>: StorageMutate<M, Error = StorageError>,
    Self: Modifiable,
{
    fn insert(
        &mut self,
        key: &M::Key,
        value: &M::Value,
    ) -> StorageResult<Option<M::OwnedValue>> {
        let mut transaction = StorageTransaction::transaction(
            &*self,
            ConflictPolicy::Overwrite,
            Default::default(),
        );
        let prev = transaction.storage_as_mut::<M>().insert(key, value)?;
        self.commit_changes(transaction.into_changes())?;
        Ok(prev)
    }

    fn remove(&mut self, key: &M::Key) -> StorageResult<Option<M::OwnedValue>> {
        let mut transaction = StorageTransaction::transaction(
            &*self,
            ConflictPolicy::Overwrite,
            Default::default(),
        );
        let prev = transaction.storage_as_mut::<M>().remove(key)?;
        self.commit_changes(transaction.into_changes())?;
        Ok(prev)
    }
}

impl<M, Description> StorageSize<M> for Database<Description>
where
    Description: DatabaseDescription,
    M: Mappable,
    for<'a> StructuredStorage<&'a Self>: StorageSize<M, Error = StorageError>,
{
    fn size_of_value(&self, key: &M::Key) -> StorageResult<Option<usize>> {
        <_ as StorageSize<M>>::size_of_value(&StructuredStorage::new(self), key)
    }
}

impl<Description, Key, M> MerkleRootStorage<Key, M> for Database<Description>
where
    Description: DatabaseDescription,
    M: Mappable,
    for<'a> StructuredStorage<&'a Self>: StorageInspect<M, Error = StorageError>,
    for<'a> StorageTransaction<&'a Self>: MerkleRootStorage<Key, M, Error = StorageError>,
    Self: Modifiable,
{
    fn root(&self, key: &Key) -> StorageResult<MerkleRoot> {
        // TODO: Use `StructuredStorage` instead of `StorageTransaction` https://github.com/FuelLabs/fuel-vm/pull/679
        self.read_transaction().storage::<M>().root(key)
    }
}

impl<Description, M> StorageRead<M> for Database<Description>
where
    Description: DatabaseDescription,
    M: Mappable,
    for<'a> StructuredStorage<&'a Self>: StorageRead<M, Error = StorageError>,
{
    fn read(&self, key: &M::Key, buf: &mut [u8]) -> StorageResult<Option<usize>> {
        StructuredStorage::new(self).storage::<M>().read(key, buf)
    }

    fn read_alloc(&self, key: &M::Key) -> StorageResult<Option<Vec<u8>>> {
        StructuredStorage::new(self).storage::<M>().read_alloc(key)
    }
}

impl<Description, M> StorageBatchMutate<M> for Database<Description>
where
    Description: DatabaseDescription,
    M: Mappable,
    for<'a> StructuredStorage<&'a Self>: StorageInspect<M, Error = StorageError>,
    for<'a> StorageTransaction<&'a Self>: StorageBatchMutate<M, Error = StorageError>,
    Self: Modifiable,
{
    fn init_storage<'a, Iter>(&mut self, set: Iter) -> StorageResult<()>
    where
        Iter: 'a + Iterator<Item = (&'a M::Key, &'a M::Value)>,
        M::Key: 'a,
        M::Value: 'a,
    {
        let mut transaction = StorageTransaction::transaction(
            &*self,
            ConflictPolicy::Overwrite,
            Default::default(),
        );
        StorageBatchMutate::init_storage(&mut transaction, set)?;
        self.commit_changes(transaction.into_changes())?;
        Ok(())
    }

    fn insert_batch<'a, Iter>(&mut self, set: Iter) -> StorageResult<()>
    where
        Iter: 'a + Iterator<Item = (&'a M::Key, &'a M::Value)>,
        M::Key: 'a,
        M::Value: 'a,
    {
        let mut transaction = StorageTransaction::transaction(
            &*self,
            ConflictPolicy::Overwrite,
            Default::default(),
        );
        StorageBatchMutate::insert_batch(&mut transaction, set)?;
        self.commit_changes(transaction.into_changes())?;
        Ok(())
    }

    fn remove_batch<'a, Iter>(&mut self, set: Iter) -> StorageResult<()>
    where
        Iter: 'a + Iterator<Item = &'a M::Key>,
        M::Key: 'a,
    {
        let mut transaction = StorageTransaction::transaction(
            &*self,
            ConflictPolicy::Overwrite,
            Default::default(),
        );
        StorageBatchMutate::remove_batch(&mut transaction, set)?;
        self.commit_changes(transaction.into_changes())?;
        Ok(())
    }
}
