use crate::{
    database::{
        block::FuelBlockSecondaryKeyBlockHeights,
        coin::OwnedCoins,
        message::OwnedMessageIds,
        transactions::{
            OwnedTransactions,
            TransactionStatuses,
        },
        Database,
    },
    state::DataSource,
};
use fuel_core_storage::{
    structured_storage::StructuredStorage,
    tables::{
        merkle::{
            ContractsAssetsMerkleData,
            ContractsAssetsMerkleMetadata,
            ContractsStateMerkleData,
            ContractsStateMerkleMetadata,
            FuelBlockMerkleData,
            FuelBlockMerkleMetadata,
        },
        ContractsAssets,
        ContractsInfo,
        ContractsLatestUtxo,
        ContractsRawCode,
        ContractsState,
        ProcessedTransactions,
        Receipts,
        SealedBlockConsensus,
        SpentMessages,
        Transactions,
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

/// The trait allows selectively inheriting the implementation of storage traits from `StructuredStorage`
/// for the `Database`. Not all default implementations of the `StructuredStorage` are suitable
/// for the `Database`. Sometimes we want to override some of them and add a custom implementation
/// with additional logic. For example, we want to override the `StorageMutate` trait for the `Messages`
/// table to also track the owner of messages.
pub trait UseStructuredImplementation<M>
where
    M: Mappable,
{
}

/// The trait allows to implementation of `UseStructuredImplementation` for the `StructuredStorage` for multiple tables.
macro_rules! use_structured_implementation {
    ($($m:ty),*) => {
        $(
            impl UseStructuredImplementation<$m> for StructuredStorage<DataSource> {}
        )*
    };
}

use_structured_implementation!(
    ContractsRawCode,
    ContractsAssets,
    ContractsState,
    ContractsLatestUtxo,
    ContractsInfo,
    SpentMessages,
    SealedBlockConsensus,
    Transactions,
    ProcessedTransactions,
    Receipts,
    ContractsStateMerkleMetadata,
    ContractsStateMerkleData,
    ContractsAssetsMerkleMetadata,
    ContractsAssetsMerkleData,
    OwnedCoins,
    OwnedMessageIds,
    OwnedTransactions,
    TransactionStatuses,
    FuelBlockSecondaryKeyBlockHeights,
    FuelBlockMerkleData,
    FuelBlockMerkleMetadata
);
#[cfg(feature = "relayer")]
use_structured_implementation!(
    fuel_core_relayer::storage::RelayerMetadata,
    fuel_core_relayer::storage::EventsHistory
);

impl<M> StorageInspect<M> for Database
where
    M: Mappable,
    StructuredStorage<DataSource>:
        StorageInspect<M, Error = StorageError> + UseStructuredImplementation<M>,
{
    type Error = StorageError;

    fn get(&self, key: &M::Key) -> StorageResult<Option<Cow<M::OwnedValue>>> {
        self.data.storage::<M>().get(key)
    }

    fn contains_key(&self, key: &M::Key) -> StorageResult<bool> {
        self.data.storage::<M>().contains_key(key)
    }
}

impl<M> StorageMutate<M> for Database
where
    M: Mappable,
    StructuredStorage<DataSource>:
        StorageMutate<M, Error = StorageError> + UseStructuredImplementation<M>,
{
    fn insert(
        &mut self,
        key: &M::Key,
        value: &M::Value,
    ) -> StorageResult<Option<M::OwnedValue>> {
        self.data.storage_as_mut::<M>().insert(key, value)
    }

    fn remove(&mut self, key: &M::Key) -> StorageResult<Option<M::OwnedValue>> {
        self.data.storage_as_mut::<M>().remove(key)
    }
}

impl<Key, M> MerkleRootStorage<Key, M> for Database
where
    M: Mappable,
    StructuredStorage<DataSource>:
        MerkleRootStorage<Key, M, Error = StorageError> + UseStructuredImplementation<M>,
{
    fn root(&self, key: &Key) -> StorageResult<MerkleRoot> {
        self.data.storage::<M>().root(key)
    }
}

impl<M> StorageSize<M> for Database
where
    M: Mappable,
    StructuredStorage<DataSource>:
        StorageSize<M, Error = StorageError> + UseStructuredImplementation<M>,
{
    fn size_of_value(&self, key: &M::Key) -> StorageResult<Option<usize>> {
        <_ as StorageSize<M>>::size_of_value(&self.data, key)
    }
}

impl<M> StorageRead<M> for Database
where
    M: Mappable,
    StructuredStorage<DataSource>:
        StorageRead<M, Error = StorageError> + UseStructuredImplementation<M>,
{
    fn read(&self, key: &M::Key, buf: &mut [u8]) -> StorageResult<Option<usize>> {
        self.data.storage::<M>().read(key, buf)
    }

    fn read_alloc(&self, key: &M::Key) -> StorageResult<Option<Vec<u8>>> {
        self.data.storage::<M>().read_alloc(key)
    }
}

impl<M> StorageBatchMutate<M> for Database
where
    M: Mappable,
    StructuredStorage<DataSource>:
        StorageBatchMutate<M, Error = StorageError> + UseStructuredImplementation<M>,
{
    fn init_storage<'a, Iter>(&mut self, set: Iter) -> StorageResult<()>
    where
        Iter: 'a + Iterator<Item = (&'a M::Key, &'a M::Value)>,
        M::Key: 'a,
        M::Value: 'a,
    {
        StorageBatchMutate::init_storage(&mut self.data, set)
    }

    fn insert_batch<'a, Iter>(&mut self, set: Iter) -> StorageResult<()>
    where
        Iter: 'a + Iterator<Item = (&'a M::Key, &'a M::Value)>,
        M::Key: 'a,
        M::Value: 'a,
    {
        StorageBatchMutate::insert_batch(&mut self.data, set)
    }

    fn remove_batch<'a, Iter>(&mut self, set: Iter) -> StorageResult<()>
    where
        Iter: 'a + Iterator<Item = &'a M::Key>,
        M::Key: 'a,
    {
        StorageBatchMutate::remove_batch(&mut self.data, set)
    }
}
