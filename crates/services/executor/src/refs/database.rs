use fuel_core_storage::{
    tables::{
        ContractsAssets,
        ContractsInfo,
        ContractsRawCode,
        ContractsState,
    },
    transactional::Transaction,
    ContractsAssetsStorage,
    InterpreterStorage,
    Mappable,
    MerkleRoot,
    MerkleRootStorage,
    StorageInspect,
    StorageMutate,
    StorageRead,
    StorageSize,
};
use fuel_core_types::{
    blockchain::{
        header::ConsensusHeader,
        primitives::BlockId,
    },
    fuel_tx::{
        Address,
        ContractId,
    },
    fuel_types::{
        BlockHeight,
        Bytes32,
    },
    services::txpool::TransactionStatus,
    tai64::Tai64,
};
use std::{
    borrow::Cow,
    ops::DerefMut,
};

pub trait ExecutorDatabaseTrait<D> {
    type T: Transaction<D> + DerefMut<Target = D> + 'static;

    fn transaction(&self) -> Self::T;
}

pub trait FuelBlockTrait {
    type Error;

    fn latest_height(&self) -> Result<BlockHeight, Self::Error>;
    fn block_time(&self, height: &BlockHeight) -> Result<Tai64, Self::Error>;
    fn get_block_id(&self, height: &BlockHeight) -> Result<Option<BlockId>, Self::Error>;
}

pub trait FuelStateTrait {
    type Error;

    fn init_contract_state<S: Iterator<Item = (Bytes32, Bytes32)>>(
        &mut self,
        contract_id: &ContractId,
        slots: S,
    ) -> Result<(), Self::Error>;
}

pub trait TxIdOwnerRecorder {
    type Error;

    fn record_tx_id_owner(
        &self,
        owner: &Address,
        block_height: BlockHeight,
        tx_idx: u16,
        tx_id: &Bytes32,
    ) -> Result<Option<Bytes32>, Self::Error>;

    fn update_tx_status(
        &self,
        id: &Bytes32,
        status: TransactionStatus,
    ) -> Result<Option<TransactionStatus>, Self::Error>;
}

pub struct ExecutorVmDatabase<D> {
    pub current_block_height: BlockHeight,
    pub current_timestamp: Tai64,
    pub coinbase: ContractId,
    pub database: D,
}

impl<D> ExecutorVmDatabase<D> {
    pub fn new<T>(
        database: D,
        header: &ConsensusHeader<T>,
        coinbase: ContractId,
    ) -> Self {
        Self {
            current_block_height: header.height,
            current_timestamp: header.time,
            coinbase,
            database,
        }
    }
}

use fuel_core_storage::Error as StorageError;
use fuel_core_types::fuel_tx::Word;

impl<D> ContractsAssetsStorage for ExecutorVmDatabase<D> where
    D: MerkleRootStorage<ContractId, ContractsAssets, Error = StorageError>
{
}

impl<D, M: Mappable> StorageMutate<M> for ExecutorVmDatabase<D>
where
    D: StorageMutate<M, Error = StorageError>,
{
    fn insert(
        &mut self,
        key: &M::Key,
        value: &M::Value,
    ) -> Result<Option<M::OwnedValue>, Self::Error> {
        StorageMutate::<M>::insert(&mut self.database, key, value)
    }

    fn remove(&mut self, key: &M::Key) -> Result<Option<M::OwnedValue>, Self::Error> {
        StorageMutate::<M>::remove(&mut self.database, key)
    }
}

impl<D, M: Mappable> StorageInspect<M> for ExecutorVmDatabase<D>
where
    D: StorageInspect<M, Error = StorageError>,
{
    type Error = StorageError;

    fn get(&self, key: &M::Key) -> Result<Option<Cow<M::OwnedValue>>, Self::Error> {
        StorageInspect::<M>::get(&self.database, key)
    }

    fn contains_key(&self, key: &M::Key) -> Result<bool, Self::Error> {
        StorageInspect::<M>::contains_key(&self.database, key)
    }
}

impl<D, K, M: Mappable> MerkleRootStorage<K, M> for ExecutorVmDatabase<D>
where
    D: MerkleRootStorage<K, M, Error = StorageError>,
{
    fn root(&self, key: &K) -> Result<MerkleRoot, Self::Error> {
        MerkleRootStorage::<K, M>::root(&self.database, key)
    }
}

impl<D, M: Mappable> StorageRead<M> for ExecutorVmDatabase<D>
where
    D: StorageRead<M, Error = StorageError>,
{
    fn read(&self, key: &M::Key, buf: &mut [u8]) -> Result<Option<usize>, Self::Error> {
        StorageRead::<M>::read(&self.database, key, buf)
    }

    fn read_alloc(
        &self,
        key: &<M as Mappable>::Key,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        StorageRead::<M>::read_alloc(&self.database, key)
    }
}

impl<D, M: Mappable> StorageSize<M> for ExecutorVmDatabase<D>
where
    D: StorageSize<M, Error = StorageError>,
{
    fn size_of_value(&self, key: &M::Key) -> Result<Option<usize>, Self::Error> {
        StorageSize::<M>::size_of_value(&self.database, key)
    }
}

impl<D> InterpreterStorage for ExecutorVmDatabase<D>
where
    D: StorageInspect<ContractsInfo, Error = StorageError>
        + StorageMutate<ContractsInfo, Error = StorageError>
        + StorageInspect<ContractsState, Error = StorageError>
        + MerkleRootStorage<ContractId, ContractsState, Error = StorageError>
        + StorageMutate<ContractsRawCode, Error = StorageError>
        + MerkleRootStorage<ContractId, ContractsAssets, Error = StorageError>
        + FuelBlockTrait<Error = StorageError>
        + FuelStateTrait<Error = StorageError>
        + StorageRead<ContractsRawCode, Error = StorageError>,
{
    type DataError = StorageError;

    fn block_height(&self) -> Result<BlockHeight, Self::DataError> {
        Ok(self.current_block_height)
    }

    fn timestamp(&self, height: BlockHeight) -> Result<Word, Self::DataError> {
        let timestamp = match height {
            // panic if $rB is greater than the current block height.
            height if height > self.current_block_height => {
                return Err(anyhow::anyhow!("block height too high for timestamp").into())
            }
            height if height == self.current_block_height => self.current_timestamp,
            height => self.database.block_time(&height)?,
        };
        Ok(timestamp.0)
    }

    fn block_hash(&self, _block_height: BlockHeight) -> Result<Bytes32, Self::DataError> {
        todo!()
    }

    fn coinbase(&self) -> Result<ContractId, Self::DataError> {
        Ok(self.coinbase)
    }

    fn merkle_contract_state_range(
        &self,
        _id: &ContractId,
        _start_key: &Bytes32,
        _range: usize,
    ) -> Result<Vec<Option<Cow<Bytes32>>>, Self::DataError> {
        todo!()
    }

    fn merkle_contract_state_insert_range(
        &mut self,
        _contract: &ContractId,
        _start_key: &Bytes32,
        _values: &[Bytes32],
    ) -> Result<Option<()>, Self::DataError> {
        todo!()
    }

    fn merkle_contract_state_remove_range(
        &mut self,
        _contract: &ContractId,
        _start_key: &Bytes32,
        _range: usize,
    ) -> Result<Option<()>, Self::DataError> {
        todo!()
    }
}
