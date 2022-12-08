use crate::{
    database::{
        Column,
        Database,
    },
    state::{
        IterDirection,
        MultiKey,
        WriteOperation,
    },
};
use anyhow::anyhow;
use fuel_core_interfaces::{
    common::{
        fuel_storage::{
            Mappable,
            MerkleRoot,
            StorageInspect,
            StorageMutate,
        },
        fuel_tx::{
            Bytes32,
            Bytes64,
        },
        prelude::{
            Address,
            ContractId,
            InterpreterStorage,
            MerkleRootStorage,
            Word,
        },
        tai64::Tai64,
    },
    db::{
        ContractsState,
        Error,
    },
    model::FuelConsensusHeader,
    not_found,
};
use primitive_types::U256;
use std::{
    borrow::Cow,
    ops::Deref,
    thread::current,
};

/// Used to store metadata relevant during the execution of a transaction
#[derive(Clone, Debug)]
pub struct VmDatabase {
    current_block_height: u32,
    current_timestamp: Tai64,
    coinbase: Address,
    database: Database,
}

impl Default for VmDatabase {
    fn default() -> Self {
        Self {
            current_block_height: 0,
            current_timestamp: Tai64::now(),
            coinbase: Default::default(),
            database: Default::default(),
        }
    }
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
        Ok(timestamp.0)
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
                .ok_or_else(|| not_found!("BlockId").into())
        }
    }

    fn coinbase(&self) -> Result<Address, Self::DataError> {
        Ok(self.coinbase)
    }

    fn merkle_contract_state_range(
        &self,
        contract_id: &ContractId,
        start_key: &Bytes32,
        range: Word,
    ) -> Result<Vec<Option<Cow<Bytes32>>>, Self::DataError> {
        let mut iterator = self.database.iter_all::<Bytes64, Bytes32>(
            Column::ContractsState,
            Some(contract_id.as_ref().to_vec()),
            Some(MultiKey::new(&(contract_id, start_key)).into()),
            Some(IterDirection::Forward),
        );

        let mut current_key = U256::from_big_endian(start_key.as_ref());

        let mut range_count = 0;

        let mut results: Vec<Option<Cow<Bytes32>>> = vec![];

        while range_count < range {
            let entry_option = iterator.next();

            if let Some(entry) = entry_option {
                let entry = entry?;
                let multikey = entry.0;
                let value = entry.1;

                let state_contract_id =
                    Bytes32::new(multikey.as_ref()[0..32].try_into()?);
                let state_key = U256::from_big_endian(&multikey.as_ref()[32..]);

                if state_contract_id != contract_id {
                    // Iterator moved beyond contract range, populate with None until end of range
                    for _ in range_count..range {
                        results.push(None);
                    }
                    // Iterator no longer useful, return
                    return results.into()
                } else if state_key != current_key {
                    while (state_key != current_key) && (range_count < range) {
                        // Iterator moved beyond next expected key, push none and increment range
                        // count until we find the current key
                        results.push(None);
                        range_count += 1;
                        current_key.checked_add(1.into()).ok_or(Error::Other(
                            anyhow!("current_key overflowed during computation"),
                        ))?;
                    }
                }
                // State key matches, put value into results
                if state_key == current_key {
                    results.push(value.into());
                }
            } else {
                // No iterator returned, populate with None until end of range
                for _ in range_count..range {
                    results.push(None);
                }
            };
            range_count += 1;
        }

        return Ok(results)
    }

    fn merkle_contract_state_insert_range(
        &mut self,
        contract: &ContractId,
        start_key: &Bytes32,
        values: &[Bytes32],
    ) -> Result<Option<()>, Self::DataError> {
        let mut found_unset = false;

        let mut current_key = U256::from_big_endian(start_key.as_ref());

        let transaction = self.database.transaction();
        let transaction_db = transaction.deref();

        for value in values {
            current_key =
                current_key
                    .checked_add(1.into())
                    .ok_or(Error::Other(anyhow!(
                        "current_key overflowed during computation"
                    )))?;
            let mut key_bytes = [0u8; 32];
            current_key.to_big_endian(&mut key_bytes);

            let option = transaction_db.insert(
                MultiKey::new(&(contract, key_bytes)).into(),
                Column::ContractsState,
                value.into_vec(),
            )?;

            found_unset |= option.is_none();
        }

        transaction.commit()?;

        return Ok((!found_unset).then(|| ()))
    }

    fn merkle_contract_state_remove_range(
        &mut self,
        contract: &ContractId,
        start_key: &Bytes32,
        range: Word,
    ) -> Result<Option<()>, Self::DataError> {
        let mut found_unset = false;

        let mut current_key = U256::from_big_endian(start_key.as_ref());

        let transaction = self.database.transaction();
        let transaction_db = transaction.deref();

        for _ in 0..range {
            current_key =
                current_key
                    .checked_add(1.into())
                    .ok_or(Error::Other(anyhow!(
                        "current_key overflowed during computation"
                    )))?;
            let mut key_bytes = [0u8; 32];
            current_key.to_big_endian(&mut key_bytes);

            let option = transaction_db.remove(
                MultiKey::new(&(contract, key_bytes)).into(),
                Column::ContractsState,
            )?;

            found_unset |= option.is_none();
        }

        transaction.commit()?;

        return Ok((!found_unset).then(|| ()))
    }
}
