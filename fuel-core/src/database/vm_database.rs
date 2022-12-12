use crate::{
    database::{
        Column,
        Database,
    },
    state::{
        IterDirection,
        MultiKey,
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
        fuel_tx::Bytes32,
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
        Error,
        Transactional,
    },
    model::FuelConsensusHeader,
    not_found,
};
use primitive_types::U256;
use std::{
    borrow::Cow,
    ops::Deref,
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
        let mut iterator = self.database.iter_all::<Vec<u8>, Bytes32>(
            Column::ContractsState,
            Some(contract_id.as_ref().to_vec()),
            Some(MultiKey::new(&(contract_id, start_key)).into()),
            Some(IterDirection::Forward),
        );

        let mut current_key = U256::from_big_endian(start_key.as_ref());

        let mut range_count = 0;

        let mut results = vec![];

        while range_count < range {
            let entry_option = iterator.next();

            if let Some(entry) = entry_option {
                let entry = entry?;
                let multikey = entry.0;
                let value = entry.1;

                let state_contract_id =
                    ContractId::new(multikey[..32].try_into().map_err(|e| {
                        anyhow::Error::from(e).context("Invalid state key length")
                    })?);
                let state_key = U256::from_big_endian(&multikey[32..]);

                if &state_contract_id != contract_id {
                    // Iterator moved beyond contract range, populate with None until end of range
                    for _ in range_count..range {
                        results.push(None);
                        current_key =
                            current_key.checked_add(1.into()).ok_or(Error::Other(
                                anyhow!("current_key overflowed during computation"),
                            ))?;
                    }
                    // Iterator no longer useful, return
                    return Ok(results)
                } else if state_key != current_key {
                    while (state_key != current_key) && (range_count < range) {
                        // Iterator moved beyond next expected key, push none and increment range
                        // count until we find the current key
                        results.push(None);
                        range_count += 1;
                        current_key =
                            current_key.checked_add(1.into()).ok_or(Error::Other(
                                anyhow!("current_key overflowed during computation"),
                            ))?;
                    }
                }
                // State key matches, put value into results
                if state_key == current_key {
                    results.push(Some(Cow::Owned(value)));
                    current_key =
                        current_key.checked_add(1.into()).ok_or(Error::Other(
                            anyhow!("current_key overflowed during computation"),
                        ))?;
                }
            } else {
                // No iterator returned, populate with None until end of range
                for _ in range_count..range {
                    results.push(None);
                    range_count += 1;
                    current_key =
                        current_key.checked_add(1.into()).ok_or(Error::Other(
                            anyhow!("current_key overflowed during computation"),
                        ))?;
                }
            };
            range_count += 1;
        }

        Ok(results)
    }

    fn merkle_contract_state_insert_range(
        &mut self,
        contract_id: &ContractId,
        start_key: &Bytes32,
        values: &[Bytes32],
    ) -> Result<Option<()>, Self::DataError> {
        let mut found_unset = false;

        let mut current_key = U256::from_big_endian(start_key.as_ref());
        let mut key_bytes = [0u8; 32];

        let transaction = self.database.transaction();
        let transaction_db = transaction.deref();

        for value in values {
            current_key.to_big_endian(&mut key_bytes);

            let option = transaction_db.insert::<_, _, Bytes32>(
                MultiKey::new(&(contract_id, key_bytes)).as_ref(),
                Column::ContractsState,
                value,
            )?;

            found_unset |= option.is_none();

            current_key =
                current_key
                    .checked_add(1.into())
                    .ok_or(Error::Other(anyhow!(
                        "current_key overflowed during computation"
                    )))?;
        }

        transaction.commit()?;

        // let get_result = self.database.get::<Bytes32>(
        //     MultiKey::new(&(contract_id, start_key)).as_ref(),
        //     Column::ContractsState,
        // );

        Ok((!found_unset).then(|| ()))
    }

    fn merkle_contract_state_remove_range(
        &mut self,
        contract_id: &ContractId,
        start_key: &Bytes32,
        range: Word,
    ) -> Result<Option<()>, Self::DataError> {
        let mut found_unset = false;

        let mut current_key = U256::from_big_endian(start_key.as_ref());

        let transaction = self.database.transaction();
        let transaction_db = transaction.deref();

        for _ in 0..range {
            let mut key_bytes = [0u8; 32];
            current_key.to_big_endian(&mut key_bytes);

            let option = transaction_db.remove::<Bytes32>(
                MultiKey::new(&(contract_id, key_bytes)).as_ref(),
                Column::ContractsState,
            )?;

            found_unset |= option.is_none();

            current_key =
                current_key
                    .checked_add(1.into())
                    .ok_or(Error::Other(anyhow!(
                        "current_key overflowed during computation"
                    )))?;
        }

        transaction.commit()?;

        Ok((!found_unset).then(|| ()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use itertools::Itertools;
    use rand::{
        rngs::StdRng,
        Rng,
        SeedableRng,
    };
    use std::ops::{
        Add,
        Sub,
    };

    fn u256_to_bytes32(u: U256) -> Bytes32 {
        let mut bytes = [0u8; 32];
        u.to_big_endian(&mut bytes);
        Bytes32::from(bytes)
    }

    #[test]
    fn read_single_value() {
        let mut db = VmDatabase::default();

        let contract_id = ContractId::new([0u8; 32]);
        let key = Bytes32::new([0u8; 32]);
        let value = Bytes32::new([1u8; 32]);

        // check that read is unset before insert
        let pre_read_status = db.merkle_contract_state(&contract_id, &key).unwrap();
        assert!(pre_read_status.is_none());

        // insert expected key
        let _ = db
            .merkle_contract_state_insert(&contract_id, &key, &value)
            .unwrap();

        // check that read is set and returns the correct value
        let read_status = db.merkle_contract_state(&contract_id, &key).unwrap();
        assert!(read_status.is_some());
        assert_eq!(read_status.unwrap().into_owned(), value);
    }

    #[test]
    fn read_sequential_set_data() {
        let rng = &mut StdRng::seed_from_u64(100);
        let db = VmDatabase::default();

        const RANGE_LENGTH: usize = 10;
        let contract_id = ContractId::new([0u8; 32]);
        let start_key = U256::zero();

        // check range is unset
        db.merkle_contract_state_range(&contract_id, &u256_to_bytes32(start_key), 10)
            .unwrap()
            .iter()
            .for_each(|item| assert!(item.is_none()));

        let setup_values = (0..RANGE_LENGTH)
            .map(|_| rng.gen())
            .collect::<Vec<Bytes32>>();

        // setup data
        for i in 0..RANGE_LENGTH {
            let key = start_key.add(i);
            let key = u256_to_bytes32(key);
            let multi_key = MultiKey::new(&(contract_id.as_ref(), key.as_ref()));
            db.database
                .insert::<_, _, Bytes32>(
                    &multi_key,
                    Column::ContractsState,
                    &setup_values[i],
                )
                .unwrap();
        }

        // perform sequential read
        let results = db
            .merkle_contract_state_range(
                &contract_id,
                &u256_to_bytes32(start_key),
                RANGE_LENGTH as u64,
            )
            .unwrap();

        // verify a vector of the correct length is returned, and all values are set correctly
        assert_eq!(results.len(), RANGE_LENGTH);
        for (i, value) in results.into_iter().enumerate() {
            let value = value.expect(&format!("Expected value to be set at {}", i));
            assert_eq!(value.as_ref(), &setup_values[i]);
        }
    }

    #[test]
    fn read_range_unset() {
        // ensure we pad the correct number of results even if the iterator is empty
        const RANGE_LENGTH: usize = 10;
        let contract_id = ContractId::new([0u8; 32]);
        let start_key = U256::zero();

        let db = VmDatabase::default();
        // perform sequential read
        let results = db
            .merkle_contract_state_range(
                &contract_id,
                &u256_to_bytes32(start_key),
                RANGE_LENGTH as u64,
            )
            .unwrap();
        assert_eq!(results.len(), RANGE_LENGTH);
        results
            .iter()
            .enumerate()
            .for_each(|(i, item)| assert!(item.is_none(), "Expected None for idx {}", i));
    }

    #[test]
    fn read_over_unset_region_same_contract() {
        let rng = &mut StdRng::seed_from_u64(100);
        let db = VmDatabase::default();

        const RANGE_LENGTH: usize = 10;
        let contract_id = ContractId::new([0u8; 32]);
        let start_key = U256::zero();

        // only set even keys to some value
        let setup_values = (0..RANGE_LENGTH)
            .map(|i| if i % 2 == 0 { Some(rng.gen()) } else { None })
            .collect::<Vec<Option<Bytes32>>>();

        // setup only some of the data in the range
        for i in 0..RANGE_LENGTH {
            if let Some(value) = setup_values[i] {
                let key = start_key.add(i);
                let key = u256_to_bytes32(key);
                let multi_key = MultiKey::new(&(contract_id.as_ref(), key.as_ref()));
                db.database
                    .insert::<_, _, Bytes32>(&multi_key, Column::ContractsState, &value)
                    .unwrap();
            }
        }

        // perform sequential read
        let results = db
            .merkle_contract_state_range(
                &contract_id,
                &u256_to_bytes32(start_key),
                RANGE_LENGTH as u64,
            )
            .unwrap();

        // verify a vector of the correct length is returned, and all values are set correctly
        assert_eq!(results.len(), RANGE_LENGTH);
        for (i, value) in results.into_iter().enumerate() {
            assert_eq!(value.map(Cow::into_owned), setup_values[i]);
        }
    }

    #[test]
    fn read_overrun_contract_id() {
        // setup two contracts in the same database
        // to ensure we don't return data from other contracts
        let rng = &mut StdRng::seed_from_u64(100);
        let db = VmDatabase::default();

        let contract_id_1 = ContractId::new([0u8; 32]);
        let contract_id_2 = ContractId::new([1u8; 32]);
        let start_key = U256::zero();

        // setup test values for the database
        let key = u256_to_bytes32(start_key.add(0));
        let c1_k1 = MultiKey::new(&(contract_id_1.as_ref(), key.as_ref()));
        let c1_v1: Bytes32 = rng.gen();
        db.database
            .insert::<_, _, Bytes32>(&c1_k1, Column::ContractsState, &c1_v1)
            .unwrap();

        let key = u256_to_bytes32(start_key.add(1));
        let c1_k2 = MultiKey::new(&(contract_id_1.as_ref(), key.as_ref()));
        let c1_v2: Bytes32 = rng.gen();
        db.database
            .insert::<_, _, Bytes32>(&c1_k2, Column::ContractsState, &c1_v2)
            .unwrap();

        let key = u256_to_bytes32(start_key.add(0));
        let c2_k1 = MultiKey::new(&(contract_id_2.as_ref(), key.as_ref()));
        let c2_v1: Bytes32 = rng.gen();
        db.database
            .insert::<_, _, Bytes32>(&c2_k1, Column::ContractsState, &c2_v1)
            .unwrap();

        let key = u256_to_bytes32(start_key.add(1));
        let c2_k2 = MultiKey::new(&(contract_id_2.as_ref(), key.as_ref()));
        let c2_v2: Bytes32 = rng.gen();
        db.database
            .insert::<_, _, Bytes32>(&c2_k2, Column::ContractsState, &c2_v2)
            .unwrap();

        // perform sequential read
        const READ_RANGE: usize = 4;
        let results = db
            .merkle_contract_state_range(
                &contract_id_1,
                &u256_to_bytes32(start_key),
                READ_RANGE as u64,
            )
            .unwrap()
            .into_iter()
            .map(|v| v.map(Cow::into_owned))
            .collect_vec();

        // verify a vector of the correct length is returned, and all values are set correctly
        assert_eq!(results.len(), READ_RANGE);
        assert_eq!(results[0], Some(c1_v1));
        assert_eq!(results[1], Some(c1_v2));
        assert_eq!(results[2], None);
        assert_eq!(results[3], None);
    }

    #[test]
    fn read_range_overruns_keyspace_mismatched_contract_id() {
        // ensure that we don't pad extra results past u256::max when there are multiple contracts
        let rng = &mut StdRng::seed_from_u64(100);
        let db = VmDatabase::default();

        let contract_id_1 = ContractId::new([0u8; 32]);
        let contract_id_2 = ContractId::new([1u8; 32]);
        let start_key = U256::max_value().sub(2);

        // setup test values for the database
        let key = u256_to_bytes32(start_key.add(0));
        let c1_k1 = MultiKey::new(&(contract_id_1.as_ref(), key.as_ref()));
        let c1_v1: Bytes32 = rng.gen();
        db.database
            .insert::<_, _, Bytes32>(&c1_k1, Column::ContractsState, &c1_v1)
            .unwrap();

        let key = u256_to_bytes32(start_key.add(1));
        let c1_k2 = MultiKey::new(&(contract_id_1.as_ref(), key.as_ref()));
        let c1_v2: Bytes32 = rng.gen();
        db.database
            .insert::<_, _, Bytes32>(&c1_k2, Column::ContractsState, &c1_v2)
            .unwrap();

        let key = u256_to_bytes32(start_key.add(0));
        let c2_k1 = MultiKey::new(&(contract_id_2.as_ref(), key.as_ref()));
        let c2_v1: Bytes32 = rng.gen();
        db.database
            .insert::<_, _, Bytes32>(&c2_k1, Column::ContractsState, &c2_v1)
            .unwrap();

        let key = u256_to_bytes32(start_key.add(1));
        let c2_k2 = MultiKey::new(&(contract_id_2.as_ref(), key.as_ref()));
        let c2_v2: Bytes32 = rng.gen();
        db.database
            .insert::<_, _, Bytes32>(&c2_k2, Column::ContractsState, &c2_v2)
            .unwrap();

        // perform sequential read
        const READ_RANGE: usize = 4;
        let result = db.merkle_contract_state_range(
            &contract_id_1,
            &u256_to_bytes32(start_key),
            READ_RANGE as u64,
        );

        // assert read fails since start key + READ_RANGE > u256::max
        assert!(result.is_err());
    }

    #[test]
    fn read_range_overruns_keyspace_partially_set_key_range() {
        // ensure that we don't pad extra results when keyspace is exhausted and some keys are set
        let rng = &mut StdRng::seed_from_u64(100);
        let db = VmDatabase::default();

        let contract_id = ContractId::new([0u8; 32]);
        // leave enough room for one valid unset storage slot before exhausting the keyspace
        let start_key = U256::max_value().sub(1);

        // setup test values for the database
        let key = u256_to_bytes32(start_key);
        let key = MultiKey::new(&(contract_id.as_ref(), key.as_ref()));
        let value: Bytes32 = rng.gen();
        db.database
            .insert::<_, _, Bytes32>(&key, Column::ContractsState, &value)
            .unwrap();

        // perform sequential read (u256::max - 1, u256::max, invalid key)
        const READ_RANGE: usize = 3;
        let result = db.merkle_contract_state_range(
            &contract_id,
            &u256_to_bytes32(start_key),
            READ_RANGE as u64,
        );

        // assert read fails since start key + READ_RANGE > u256::max
        assert!(result.is_err());
    }

    #[test]
    fn read_range_overruns_keyspace_unset_key_range() {
        // ensure that we don't pad extra results when keyspace is exhausted and no keys are set
        let db = VmDatabase::default();

        let contract_id = ContractId::new([0u8; 32]);
        // leave enough room for one valid unset storage slot before exhausting the keyspace
        let start_key = U256::max_value().sub(1);

        // perform sequential read (u256::max - 1, u256::max, invalid key)
        const READ_RANGE: usize = 3;
        let result = db.merkle_contract_state_range(
            &contract_id,
            &u256_to_bytes32(start_key),
            READ_RANGE as u64,
        );

        // assert read fails since start key + READ_RANGE > u256::max
        assert!(result.is_err());
    }

    #[test]
    fn insert_single_unset() {
        let mut db = VmDatabase::default();
        let read_db = db.clone();

        let contract_id = ContractId::new([0u8; 32]);
        let zero_bytes32 = Bytes32::new([0u8; 32]);
        let value_1 = Bytes32::new([1u8; 32]);

        let pre_insert_read = read_db
            .merkle_contract_state(&contract_id, &zero_bytes32)
            .unwrap();

        let insert_status_0 = db
            .merkle_contract_state_insert_range(&contract_id, &zero_bytes32, &[value_1])
            .unwrap();

        let post_insert_read_0 = read_db
            .merkle_contract_state(&contract_id, &zero_bytes32)
            .unwrap();

        assert!(pre_insert_read.is_none());
        assert!(insert_status_0.is_none());
        assert!(!post_insert_read_0.is_none());
        assert_eq!(post_insert_read_0.unwrap().as_ref(), &value_1);
    }

    #[test]
    fn insert_single_set() {
        let mut db = VmDatabase::default();
        let read_db = db.clone();

        let contract_id = ContractId::new([0u8; 32]);
        let zero_bytes32 = Bytes32::new([0u8; 32]);
        let value_1 = Bytes32::new([1u8; 32]);
        let value_2 = Bytes32::new([2u8; 32]);

        let insert_status_0 = db
            .merkle_contract_state_insert_range(&contract_id, &zero_bytes32, &[value_1])
            .unwrap();

        let post_insert_read_0 = read_db
            .merkle_contract_state(&contract_id, &zero_bytes32)
            .unwrap();

        let insert_status_1 = db
            .merkle_contract_state_insert_range(&contract_id, &zero_bytes32, &[value_2])
            .unwrap();

        let post_insert_read_1 = read_db
            .merkle_contract_state(&contract_id, &zero_bytes32)
            .unwrap();

        assert!(insert_status_0.is_none());
        assert!(!post_insert_read_0.is_none());
        assert_eq!(post_insert_read_0.unwrap().as_ref(), &value_1);
        assert!(!insert_status_1.is_none());
        assert!(!post_insert_read_1.is_none());
        assert_eq!(post_insert_read_1.unwrap().as_ref(), &value_2);
    }

    #[test]
    fn insert_range_over_unset() {
        let mut db = VmDatabase::default();
        let read_db = db.clone();

        let contract_id = ContractId::new([0u8; 32]);
        let zero_bytes32 = Bytes32::new([0u8; 32]);
        let value_1 = Bytes32::new([1u8; 32]);
        let value_2 = Bytes32::new([2u8; 32]);
        let value_3 = Bytes32::new([3u8; 32]);

        let u256_zero = U256::from_big_endian(zero_bytes32.as_ref());

        let mut key_1 = [0u8; 32];
        let u256_1 = u256_zero.checked_add(1.into()).unwrap();
        u256_1.to_big_endian(&mut key_1);

        let mut key_2 = [0u8; 32];
        let u256_2 = u256_zero.checked_add(2.into()).unwrap();
        u256_2.to_big_endian(&mut key_2);

        let pre_insert_read_0 = read_db
            .merkle_contract_state(&contract_id, &zero_bytes32)
            .unwrap();
        let pre_insert_read_1 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_1))
            .unwrap();
        let pre_insert_read_2 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_2))
            .unwrap();

        let insert_status = db
            .merkle_contract_state_insert_range(
                &contract_id,
                &zero_bytes32,
                &[value_1, value_2, value_3],
            )
            .unwrap();

        let read_0 = read_db
            .merkle_contract_state(&contract_id, &zero_bytes32)
            .unwrap();
        let read_1 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_1))
            .unwrap();
        let read_2 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_2))
            .unwrap();

        assert!(pre_insert_read_0.is_none());
        assert!(pre_insert_read_1.is_none());
        assert!(pre_insert_read_2.is_none());
        assert!(insert_status.is_none());
        assert!(!read_0.is_none());
        assert_eq!(read_0.unwrap().as_ref(), &value_1);
        assert!(!read_1.is_none());
        assert_eq!(read_1.unwrap().as_ref(), &value_2);
        assert!(!read_2.is_none());
        assert_eq!(read_2.unwrap().as_ref(), &value_3);
    }

    #[test]
    fn insert_range_partially_unset_start() {
        let mut db = VmDatabase::default();
        let read_db = db.clone();

        let contract_id = ContractId::new([0u8; 32]);
        let zero_bytes32 = Bytes32::new([0u8; 32]);
        let value_0 = Bytes32::new([0u8; 32]);
        let value_1 = Bytes32::new([1u8; 32]);
        let value_2 = Bytes32::new([2u8; 32]);
        let value_3 = Bytes32::new([3u8; 32]);

        let u256_zero = U256::from_big_endian(zero_bytes32.as_ref());

        let mut key_1 = [0u8; 32];
        let u256_1 = u256_zero.checked_add(1.into()).unwrap();
        u256_1.to_big_endian(&mut key_1);

        let mut key_2 = [0u8; 32];
        let u256_2 = u256_zero.checked_add(2.into()).unwrap();
        u256_2.to_big_endian(&mut key_2);

        let _insert_status_1 = db
            .merkle_contract_state_insert(&contract_id, &Bytes32::new(key_1), &value_0)
            .unwrap();
        let _insert_status_2 = db
            .merkle_contract_state_insert(&contract_id, &Bytes32::new(key_2), &value_0)
            .unwrap();

        let pre_insert_read_0 = read_db
            .merkle_contract_state(&contract_id, &zero_bytes32)
            .unwrap();
        let pre_insert_read_1 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_1))
            .unwrap();
        let pre_insert_read_2 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_2))
            .unwrap();

        let insert_status = db
            .merkle_contract_state_insert_range(
                &contract_id,
                &zero_bytes32,
                &[value_1, value_2, value_3],
            )
            .unwrap();

        let read_0 = read_db
            .merkle_contract_state(&contract_id, &zero_bytes32)
            .unwrap();
        let read_1 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_1))
            .unwrap();
        let read_2 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_2))
            .unwrap();

        assert!(pre_insert_read_0.is_none());
        assert!(!pre_insert_read_1.is_none());
        assert!(!pre_insert_read_2.is_none());
        assert!(insert_status.is_none());
        assert!(!read_0.is_none());
        assert_eq!(read_0.unwrap().as_ref(), &value_1);
        assert!(!read_1.is_none());
        assert_eq!(read_1.unwrap().as_ref(), &value_2);
        assert!(!read_2.is_none());
        assert_eq!(read_2.unwrap().as_ref(), &value_3);
    }

    #[test]
    fn insert_range_partially_unset_middle() {
        let mut db = VmDatabase::default();
        let read_db = db.clone();

        let contract_id = ContractId::new([0u8; 32]);
        let zero_bytes32 = Bytes32::new([0u8; 32]);
        let value_1 = Bytes32::new([1u8; 32]);
        let value_2 = Bytes32::new([2u8; 32]);
        let value_3 = Bytes32::new([3u8; 32]);

        let u256_zero = U256::from_big_endian(zero_bytes32.as_ref());

        let mut key_1 = [0u8; 32];
        let u256_1 = u256_zero.checked_add(1.into()).unwrap();
        u256_1.to_big_endian(&mut key_1);

        let mut key_2 = [0u8; 32];
        let u256_2 = u256_zero.checked_add(2.into()).unwrap();
        u256_2.to_big_endian(&mut key_2);

        let _insert_status_0 = db
            .merkle_contract_state_insert(&contract_id, &zero_bytes32, &zero_bytes32)
            .unwrap();
        let _insert_status_2 = db
            .merkle_contract_state_insert(
                &contract_id,
                &Bytes32::new(key_2),
                &zero_bytes32,
            )
            .unwrap();

        let pre_insert_read_0 = read_db
            .merkle_contract_state(&contract_id, &zero_bytes32)
            .unwrap();
        let pre_insert_read_1 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_1))
            .unwrap();
        let pre_insert_read_2 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_2))
            .unwrap();

        let insert_status = db
            .merkle_contract_state_insert_range(
                &contract_id,
                &zero_bytes32,
                &[value_1, value_2, value_3],
            )
            .unwrap();

        let read_0 = read_db
            .merkle_contract_state(&contract_id, &zero_bytes32)
            .unwrap();
        let read_1 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_1))
            .unwrap();
        let read_2 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_2))
            .unwrap();

        assert!(!pre_insert_read_0.is_none());
        assert!(pre_insert_read_1.is_none());
        assert!(!pre_insert_read_2.is_none());
        assert!(insert_status.is_none());
        assert!(!read_0.is_none());
        assert_eq!(read_0.unwrap().as_ref(), &value_1);
        assert!(!read_1.is_none());
        assert_eq!(read_1.unwrap().as_ref(), &value_2);
        assert!(!read_2.is_none());
        assert_eq!(read_2.unwrap().as_ref(), &value_3);
    }

    #[test]
    fn insert_range_partially_unset_end() {
        let mut db = VmDatabase::default();
        let read_db = db.clone();

        let contract_id = ContractId::new([0u8; 32]);
        let zero_bytes32 = Bytes32::new([0u8; 32]);
        let value_0 = Bytes32::new([0u8; 32]);
        let value_1 = Bytes32::new([1u8; 32]);
        let value_2 = Bytes32::new([2u8; 32]);
        let value_3 = Bytes32::new([3u8; 32]);

        let u256_zero = U256::from_big_endian(zero_bytes32.as_ref());

        let mut key_1 = [0u8; 32];
        let u256_1 = u256_zero.checked_add(1.into()).unwrap();
        u256_1.to_big_endian(&mut key_1);

        let mut key_2 = [0u8; 32];
        let u256_2 = u256_zero.checked_add(2.into()).unwrap();
        u256_2.to_big_endian(&mut key_2);

        let _insert_status_0 = db
            .merkle_contract_state_insert(&contract_id, &zero_bytes32, &value_0)
            .unwrap();
        let _insert_status_1 = db
            .merkle_contract_state_insert(&contract_id, &Bytes32::new(key_1), &value_0)
            .unwrap();

        let pre_insert_read_0 = read_db
            .merkle_contract_state(&contract_id, &zero_bytes32)
            .unwrap();
        let pre_insert_read_1 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_1))
            .unwrap();
        let pre_insert_read_2 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_2))
            .unwrap();

        let insert_status = db
            .merkle_contract_state_insert_range(
                &contract_id,
                &zero_bytes32,
                &[value_1, value_2, value_3],
            )
            .unwrap();

        let read_0 = read_db
            .merkle_contract_state(&contract_id, &zero_bytes32)
            .unwrap();
        let read_1 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_1))
            .unwrap();
        let read_2 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_2))
            .unwrap();

        assert!(!pre_insert_read_0.is_none());
        assert!(!pre_insert_read_1.is_none());
        assert!(pre_insert_read_2.is_none());
        assert!(insert_status.is_none());
        assert!(!read_0.is_none());
        assert_eq!(read_0.unwrap().as_ref(), &value_1);
        assert!(!read_1.is_none());
        assert_eq!(read_1.unwrap().as_ref(), &value_2);
        assert!(!read_2.is_none());
        assert_eq!(read_2.unwrap().as_ref(), &value_3);
    }

    #[test]
    fn insert_range_fully_set() {
        let mut db = VmDatabase::default();
        let read_db = db.clone();

        let contract_id = ContractId::new([0u8; 32]);
        let zero_bytes32 = Bytes32::new([0u8; 32]);
        let value_0 = Bytes32::new([0u8; 32]);
        let value_1 = Bytes32::new([1u8; 32]);
        let value_2 = Bytes32::new([2u8; 32]);
        let value_3 = Bytes32::new([3u8; 32]);

        let u256_zero = U256::from_big_endian(zero_bytes32.as_ref());

        let mut key_1 = [0u8; 32];
        let u256_1 = u256_zero.checked_add(1.into()).unwrap();
        u256_1.to_big_endian(&mut key_1);

        let mut key_2 = [0u8; 32];
        let u256_2 = u256_zero.checked_add(2.into()).unwrap();
        u256_2.to_big_endian(&mut key_2);

        let _insert_status_0 = db
            .merkle_contract_state_insert(&contract_id, &zero_bytes32, &value_0)
            .unwrap();
        let _insert_status_1 = db
            .merkle_contract_state_insert(&contract_id, &Bytes32::new(key_1), &value_0)
            .unwrap();
        let _insert_status_2 = db
            .merkle_contract_state_insert(&contract_id, &Bytes32::new(key_2), &value_0)
            .unwrap();

        let pre_insert_read_0 = read_db
            .merkle_contract_state(&contract_id, &zero_bytes32)
            .unwrap();
        let pre_insert_read_1 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_1))
            .unwrap();
        let pre_insert_read_2 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_2))
            .unwrap();

        let insert_status = db
            .merkle_contract_state_insert_range(
                &contract_id,
                &zero_bytes32,
                &[value_1, value_2, value_3],
            )
            .unwrap();

        let read_0 = read_db
            .merkle_contract_state(&contract_id, &zero_bytes32)
            .unwrap();
        let read_1 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_1))
            .unwrap();
        let read_2 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_2))
            .unwrap();

        assert!(!pre_insert_read_0.is_none());
        assert!(!pre_insert_read_1.is_none());
        assert!(!pre_insert_read_2.is_none());
        assert!(!insert_status.is_none());
        assert!(!read_0.is_none());
        assert_eq!(read_0.unwrap().as_ref(), &value_1);
        assert!(!read_1.is_none());
        assert_eq!(read_1.unwrap().as_ref(), &value_2);
        assert!(!read_2.is_none());
        assert_eq!(read_2.unwrap().as_ref(), &value_3);
    }

    #[test]
    fn remove_single_unset() {
        let mut db = VmDatabase::default();
        // let mut db_mut = &mut db;

        let contract_id = ContractId::new([0u8; 32]);
        let key = Bytes32::new([0u8; 32]);

        let read_db = db.clone();

        let pre_read = read_db.merkle_contract_state(&contract_id, &key).unwrap();

        let clear_status = db
            .merkle_contract_state_remove_range(&contract_id, &key, 1)
            .unwrap();

        let read = db.merkle_contract_state(&contract_id, &key).unwrap();

        assert!(pre_read.is_none());
        assert!(clear_status.is_none());
        assert!(read.is_none());
    }

    #[test]
    fn remove_single_set() {
        let mut db = VmDatabase::default();

        let contract_id = ContractId::new([0u8; 32]);
        let key = Bytes32::new([0u8; 32]);
        let value = Bytes32::new([1u8; 32]);

        let insert_status = db
            .merkle_contract_state_insert(&contract_id, &key, &value)
            .unwrap();

        let clear_status = db
            .merkle_contract_state_remove_range(&contract_id, &key, 1)
            .unwrap();

        let read_status = db.merkle_contract_state(&contract_id, &key).unwrap();

        assert!(insert_status.is_none());
        assert!(!clear_status.is_none());
        assert!(read_status.is_none());
    }

    #[test]
    fn remove_range_over_unset() {
        let mut db = VmDatabase::default();

        let contract_id = ContractId::new([0u8; 32]);
        let zero_bytes32 = Bytes32::new([0u8; 32]);

        let clear_status = db
            .merkle_contract_state_remove_range(&contract_id, &zero_bytes32, 3)
            .unwrap();

        assert!(clear_status.is_none());
    }

    #[test]
    fn remove_range_partially_unset_start() {
        let mut db = VmDatabase::default();
        let read_db = db.clone();

        let contract_id = ContractId::new([0u8; 32]);
        let zero_bytes32 = Bytes32::new([0u8; 32]);
        let value = Bytes32::new([1u8; 32]);

        let u256_zero = U256::from_big_endian(zero_bytes32.as_ref());

        let mut key_1 = [0u8; 32];
        let u256_1 = u256_zero.checked_add(1.into()).unwrap();
        u256_1.to_big_endian(&mut key_1);

        let mut key_2 = [0u8; 32];
        let u256_2 = u256_zero.checked_add(2.into()).unwrap();
        u256_2.to_big_endian(&mut key_2);

        let _insert_status_1 = db
            .merkle_contract_state_insert(&contract_id, &Bytes32::new(key_1), &value)
            .unwrap();
        let _insert_status_2 = db
            .merkle_contract_state_insert(&contract_id, &Bytes32::new(key_2), &value)
            .unwrap();

        let pre_clear_read_0 = read_db
            .merkle_contract_state(&contract_id, &zero_bytes32)
            .unwrap();
        let pre_clear_read_1 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_1))
            .unwrap();
        let pre_clear_read_2 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_2))
            .unwrap();

        let clear_status = db
            .merkle_contract_state_remove_range(&contract_id, &zero_bytes32, 3)
            .unwrap();

        let read_0 = read_db
            .merkle_contract_state(&contract_id, &zero_bytes32)
            .unwrap();
        let read_1 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_1))
            .unwrap();
        let read_2 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_2))
            .unwrap();

        assert!(pre_clear_read_0.is_none());
        assert!(!pre_clear_read_1.is_none());
        assert!(!pre_clear_read_2.is_none());
        assert!(clear_status.is_none());
        assert!(read_0.is_none());
        assert!(read_1.is_none());
        assert!(read_2.is_none());
    }

    #[test]
    fn remove_range_partially_unset_middle() {
        let mut db = VmDatabase::default();
        let read_db = db.clone();

        let contract_id = ContractId::new([0u8; 32]);
        let zero_bytes32 = Bytes32::new([0u8; 32]);
        let value = Bytes32::new([1u8; 32]);

        let u256_zero = U256::from_big_endian(zero_bytes32.as_ref());

        let mut key_1 = [0u8; 32];
        let u256_1 = u256_zero.checked_add(1.into()).unwrap();
        u256_1.to_big_endian(&mut key_1);

        let mut key_2 = [0u8; 32];
        let u256_2 = u256_zero.checked_add(2.into()).unwrap();
        u256_2.to_big_endian(&mut key_2);

        let _insert_status_0 = db
            .merkle_contract_state_insert(&contract_id, &zero_bytes32, &value)
            .unwrap();
        let _insert_status_2 = db
            .merkle_contract_state_insert(&contract_id, &Bytes32::new(key_2), &value)
            .unwrap();

        let pre_clear_read_0 = read_db
            .merkle_contract_state(&contract_id, &zero_bytes32)
            .unwrap();
        let pre_clear_read_1 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_1))
            .unwrap();
        let pre_clear_read_2 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_2))
            .unwrap();

        let clear_status = db
            .merkle_contract_state_remove_range(&contract_id, &zero_bytes32, 3)
            .unwrap();

        let read_0 = read_db
            .merkle_contract_state(&contract_id, &zero_bytes32)
            .unwrap();
        let read_1 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_1))
            .unwrap();
        let read_2 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_2))
            .unwrap();

        assert!(!pre_clear_read_0.is_none());
        assert!(pre_clear_read_1.is_none());
        assert!(!pre_clear_read_2.is_none());
        assert!(clear_status.is_none());
        assert!(read_0.is_none());
        assert!(read_1.is_none());
        assert!(read_2.is_none());
    }

    #[test]
    fn remove_range_partially_unset_end() {
        let mut db = VmDatabase::default();
        let read_db = db.clone();

        let contract_id = ContractId::new([0u8; 32]);
        let zero_bytes32 = Bytes32::new([0u8; 32]);
        let value = Bytes32::new([1u8; 32]);

        let u256_zero = U256::from_big_endian(zero_bytes32.as_ref());

        let mut key_1 = [0u8; 32];
        let u256_1 = u256_zero.checked_add(1.into()).unwrap();
        u256_1.to_big_endian(&mut key_1);

        let mut key_2 = [0u8; 32];
        let u256_2 = u256_zero.checked_add(2.into()).unwrap();
        u256_2.to_big_endian(&mut key_2);

        let _insert_status_0 = db
            .merkle_contract_state_insert(&contract_id, &zero_bytes32, &value)
            .unwrap();
        let _insert_status_1 = db
            .merkle_contract_state_insert(&contract_id, &Bytes32::new(key_1), &value)
            .unwrap();

        let pre_clear_read_0 = read_db
            .merkle_contract_state(&contract_id, &zero_bytes32)
            .unwrap();
        let pre_clear_read_1 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_1))
            .unwrap();
        let pre_clear_read_2 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_2))
            .unwrap();

        let clear_status = db
            .merkle_contract_state_remove_range(&contract_id, &zero_bytes32, 3)
            .unwrap();

        let read_0 = read_db
            .merkle_contract_state(&contract_id, &zero_bytes32)
            .unwrap();
        let read_1 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_1))
            .unwrap();
        let read_2 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_2))
            .unwrap();

        assert!(!pre_clear_read_0.is_none());
        assert!(!pre_clear_read_1.is_none());
        assert!(pre_clear_read_2.is_none());
        assert!(clear_status.is_none());
        assert!(read_0.is_none());
        assert!(read_1.is_none());
        assert!(read_2.is_none());
    }

    #[test]
    fn remove_range_fully_set() {
        let mut db = VmDatabase::default();
        let read_db = db.clone();

        let contract_id = ContractId::new([0u8; 32]);
        let zero_bytes32 = Bytes32::new([0u8; 32]);
        let value = Bytes32::new([1u8; 32]);

        let u256_zero = U256::from_big_endian(zero_bytes32.as_ref());

        let mut key_1 = [0u8; 32];
        let u256_1 = u256_zero.checked_add(1.into()).unwrap();
        u256_1.to_big_endian(&mut key_1);

        let mut key_2 = [0u8; 32];
        let u256_2 = u256_zero.checked_add(2.into()).unwrap();
        u256_2.to_big_endian(&mut key_2);

        let insert_status_0 = db
            .merkle_contract_state_insert(&contract_id, &zero_bytes32, &value)
            .unwrap();
        let insert_status_1 = db
            .merkle_contract_state_insert(&contract_id, &Bytes32::new(key_1), &value)
            .unwrap();
        let insert_status_2 = db
            .merkle_contract_state_insert(&contract_id, &Bytes32::new(key_2), &value)
            .unwrap();

        let pre_clear_read_0 = read_db
            .merkle_contract_state(&contract_id, &zero_bytes32)
            .unwrap();
        let pre_clear_read_1 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_1))
            .unwrap();
        let pre_clear_read_2 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_2))
            .unwrap();

        let clear_status = db
            .merkle_contract_state_remove_range(&contract_id, &zero_bytes32, 3)
            .unwrap();

        let read_0 = read_db
            .merkle_contract_state(&contract_id, &zero_bytes32)
            .unwrap();
        let read_1 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_1))
            .unwrap();
        let read_2 = read_db
            .merkle_contract_state(&contract_id, &Bytes32::new(key_2))
            .unwrap();

        assert!(insert_status_0.is_none());
        assert!(insert_status_1.is_none());
        assert!(insert_status_2.is_none());
        assert!(!pre_clear_read_0.is_none());
        assert!(!pre_clear_read_1.is_none());
        assert!(!pre_clear_read_2.is_none());
        assert!(!clear_status.is_none());
        assert!(read_0.is_none());
        assert!(read_1.is_none());
        assert!(read_2.is_none());
    }
}
