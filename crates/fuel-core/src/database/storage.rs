use crate::state::generic_database::GenericDatabase;
use fuel_core_storage::{
    structured_storage::StructuredStorage,
    transactional::{
        ConflictPolicy,
        Modifiable,
        StorageTransaction,
    },
    Error as StorageError,
    Mappable,
    Result as StorageResult,
    StorageAsMut,
    StorageBatchMutate,
    StorageInspect,
    StorageMutate,
    StorageWrite,
};

impl<Storage, M> StorageMutate<M> for GenericDatabase<Storage>
where
    M: Mappable,
    Self: Modifiable,
    StructuredStorage<Storage>: StorageInspect<M, Error = StorageError>,
    for<'a> StorageTransaction<&'a Storage>: StorageMutate<M, Error = StorageError>,
{
    fn replace(
        &mut self,
        key: &M::Key,
        value: &M::Value,
    ) -> Result<Option<M::OwnedValue>, Self::Error> {
        let mut transaction = StorageTransaction::transaction(
            self.as_ref(),
            ConflictPolicy::Overwrite,
            Default::default(),
        );
        let prev = transaction.storage_as_mut::<M>().replace(key, value)?;
        self.commit_changes(transaction.into_changes())?;
        Ok(prev)
    }

    fn take(&mut self, key: &M::Key) -> Result<Option<M::OwnedValue>, Self::Error> {
        let mut transaction = StorageTransaction::transaction(
            self.as_ref(),
            ConflictPolicy::Overwrite,
            Default::default(),
        );
        let prev = transaction.storage_as_mut::<M>().take(key)?;
        self.commit_changes(transaction.into_changes())?;
        Ok(prev)
    }
}

impl<Storage, M> StorageWrite<M> for GenericDatabase<Storage>
where
    M: Mappable,
    StructuredStorage<Storage>: StorageInspect<M, Error = StorageError>,
    for<'a> StorageTransaction<&'a Storage>: StorageWrite<M, Error = StorageError>,
    Self: Modifiable,
{
    fn write_bytes(&mut self, key: &M::Key, buf: &[u8]) -> Result<usize, Self::Error> {
        let mut transaction = StorageTransaction::transaction(
            self.as_ref(),
            ConflictPolicy::Overwrite,
            Default::default(),
        );
        let prev = <_ as StorageWrite<M>>::write_bytes(&mut transaction, key, buf)?;
        self.commit_changes(transaction.into_changes())?;
        Ok(prev)
    }

    fn replace_bytes(
        &mut self,
        key: &M::Key,
        buf: &[u8],
    ) -> Result<(usize, Option<Vec<u8>>), Self::Error> {
        let mut transaction = StorageTransaction::transaction(
            self.as_ref(),
            ConflictPolicy::Overwrite,
            Default::default(),
        );
        let prev = <_ as StorageWrite<M>>::replace_bytes(&mut transaction, key, buf)?;
        self.commit_changes(transaction.into_changes())?;
        Ok(prev)
    }

    fn take_bytes(&mut self, key: &M::Key) -> Result<Option<Vec<u8>>, Self::Error> {
        let mut transaction = StorageTransaction::transaction(
            self.as_ref(),
            ConflictPolicy::Overwrite,
            Default::default(),
        );
        let prev = <_ as StorageWrite<M>>::take_bytes(&mut transaction, key)?;
        self.commit_changes(transaction.into_changes())?;
        Ok(prev)
    }
}

impl<Storage, M> StorageBatchMutate<M> for GenericDatabase<Storage>
where
    M: Mappable,
    StructuredStorage<Storage>: StorageInspect<M, Error = StorageError>,
    for<'a> StorageTransaction<&'a Storage>: StorageBatchMutate<M, Error = StorageError>,
    Self: Modifiable,
{
    fn init_storage<'a, Iter>(&mut self, set: Iter) -> StorageResult<()>
    where
        Iter: 'a + Iterator<Item = (&'a M::Key, &'a M::Value)>,
        M::Key: 'a,
        M::Value: 'a,
    {
        let mut transaction = StorageTransaction::transaction(
            self.as_ref(),
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
            self.as_ref(),
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
            self.as_ref(),
            ConflictPolicy::Overwrite,
            Default::default(),
        );
        StorageBatchMutate::remove_batch(&mut transaction, set)?;
        self.commit_changes(transaction.into_changes())?;
        Ok(())
    }
}
