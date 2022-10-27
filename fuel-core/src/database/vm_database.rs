use crate::database::Database;
use anyhow::{
    anyhow,
    Context,
};
use chrono::{
    DateTime,
    Utc,
};
use fuel_core_interfaces::{
    common::{
        fuel_storage::{
            Mappable,
            MerkleRoot,
            StorageInspect,
            StorageMutate,
        },
        fuel_tx::Bytes32,
        prelude::{
            Address,
            ContractId,
            InterpreterStorage,
            MerkleRootStorage,
            Word,
        },
    },
    db::{
        Error,
        KvStoreError,
    },
    model::FuelConsensusHeader,
};
use std::borrow::Cow;

/// Used to store metadata relevant during the execution of a transaction
#[derive(Clone, Debug, Default)]
pub struct VmDatabase {
    current_block_height: u32,
    current_timestamp: DateTime<Utc>,
    coinbase: Address,
    database: Database,
}

impl VmDatabase {
    pub fn new<T>(
        database: Database,
        header: &FuelConsensusHeader<T>,
        coinbase: Address,
    ) -> Self {
        Self {
            current_block_height: header.height.into(),
            current_timestamp: header.time,
            coinbase,
            database,
        }
    }

    pub fn block_height(&self) -> u32 {
        self.current_block_height
    }
}

impl<M: Mappable> StorageInspect<M> for VmDatabase
where
    Database: StorageInspect<M, Error = Error>,
{
    type Error = Error;

    fn get(&self, key: &M::Key) -> Result<Option<Cow<M::GetValue>>, Error> {
        StorageInspect::<M>::get(&self.database, key)
    }

    fn contains_key(&self, key: &M::Key) -> Result<bool, Error> {
        StorageInspect::<M>::contains_key(&self.database, key)
    }
}

impl<M: Mappable> StorageMutate<M> for VmDatabase
where
    Database: StorageMutate<M, Error = Error>,
{
    fn insert(
        &mut self,
        key: &M::Key,
        value: &M::SetValue,
    ) -> Result<Option<M::GetValue>, Self::Error> {
        StorageMutate::<M>::insert(&mut self.database, key, value)
    }

    fn remove(&mut self, key: &M::Key) -> Result<Option<M::GetValue>, Self::Error> {
        StorageMutate::<M>::remove(&mut self.database, key)
    }
}

impl<K, M: Mappable> MerkleRootStorage<K, M> for VmDatabase
where
    Database: MerkleRootStorage<K, M, Error = Error>,
{
    fn root(&mut self, key: &K) -> Result<MerkleRoot, Self::Error> {
        MerkleRootStorage::<K, M>::root(&mut self.database, key)
    }
}

impl InterpreterStorage for VmDatabase {
    type DataError = Error;

    fn block_height(&self) -> Result<u32, Self::DataError> {
        Ok(self.current_block_height)
    }

    fn timestamp(&self, height: u32) -> Result<Word, Self::DataError> {
        let timestamp = match height {
            // panic if $rB is greater than the current block height.
            height if height > self.current_block_height => {
                return Err(anyhow!("block height too high for timestamp").into())
            }
            height if height == self.current_block_height => self.current_timestamp,
            height => self.database.block_time(height)?,
        };
        timestamp
            .timestamp_millis()
            .try_into()
            .context("invalid timestamp")
            .map_err(Into::into)
    }

    fn block_hash(&self, block_height: u32) -> Result<Bytes32, Self::DataError> {
        // Block header hashes for blocks with height greater than or equal to current block height are zero (0x00**32).
        // https://github.com/FuelLabs/fuel-specs/blob/master/specs/vm/instruction_set.md#bhsh-block-hash
        if block_height >= self.current_block_height || block_height == 0 {
            Ok(Bytes32::zeroed())
        } else {
            // this will return 0x00**32 for block height 0 as well
            self.database
                .get_block_id(block_height.into())?
                .ok_or_else(|| KvStoreError::NotFound.into())
        }
    }

    fn coinbase(&self) -> Result<Address, Self::DataError> {
        Ok(self.coinbase)
    }

    fn merkle_contract_state_range(
        &self,
        _id: &ContractId,
        _start_key: &Bytes32,
        _range: Word,
    ) -> Result<Vec<Option<Cow<Bytes32>>>, Self::DataError> {
        unimplemented!()
    }

    fn merkle_contract_state_insert_range(
        &mut self,
        _contract: &ContractId,
        _start_key: &Bytes32,
        _values: &[Bytes32],
    ) -> Result<Option<()>, Self::DataError> {
        unimplemented!()
    }

    fn merkle_contract_state_remove_range(
        &mut self,
        _contract: &ContractId,
        _start_key: &Bytes32,
        _range: Word,
    ) -> Result<Option<()>, Self::DataError> {
        unimplemented!()
    }
}
