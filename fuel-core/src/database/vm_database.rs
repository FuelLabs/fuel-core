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
    db::Error,
    model::FuelConsensusHeader,
    not_found,
};
use primitive_types::U256;
use std::borrow::Cow;

/// Used to store metadata relevant during the execution of a transaction
#[derive(Clone, Debug)]
pub struct VmDatabase {
    current_block_height: u32,
    current_timestamp: Tai64,
    coinbase: Address,
    database: Database,
}

trait IncreaseStorageKey {
    fn increase(&mut self) -> anyhow::Result<()>;
}

impl IncreaseStorageKey for U256 {
    fn increase(&mut self) -> anyhow::Result<()> {
        *self = self.checked_add(1.into()).ok_or_else(|| {
            Error::Other(anyhow!("range op exceeded available keyspace"))
        })?;
        Ok(())
    }
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
        // TODO: Optimization: Iterate only over `range` elements.
        let mut iterator = self.database.iter_all::<Vec<u8>, Bytes32>(
            Column::ContractsState,
            Some(contract_id.as_ref().to_vec()),
            Some(MultiKey::new(&(contract_id, start_key)).into()),
            Some(IterDirection::Forward),
        );
        let range = range as usize;

        let mut expected_key = U256::from_big_endian(start_key.as_ref());
        let mut results = vec![];

        while results.len() < range {
            let entry = iterator.next().transpose()?;

            if entry.is_none() {
                // We out of `contract_id` prefix
                break
            }

            let (multikey, value) =
                entry.expect("We did a check before, so the entry should be `Some`");
            let actual_key = U256::from_big_endian(&multikey[32..]);

            while (expected_key <= actual_key) && results.len() < range {
                if expected_key == actual_key {
                    // We found expected key, put value into results
                    results.push(Some(Cow::Owned(value)));
                } else {
                    // Iterator moved beyond next expected key, push none until we find the key
                    results.push(None);
                }
                expected_key.increase()?;
            }
        }

        // Fill not initialized slots with `None`.
        while results.len() < range {
            results.push(None);
            expected_key.increase()?;
        }

        Ok(results)
    }

    fn merkle_contract_state_insert_range(
        &mut self,
        contract_id: &ContractId,
        start_key: &Bytes32,
        values: &[Bytes32],
    ) -> Result<Option<()>, Self::DataError> {
        let mut current_key = U256::from_big_endian(start_key.as_ref());
        // verify key is in range
        current_key
            .checked_add(U256::from(values.len()))
            .ok_or_else(|| {
                Error::Other(anyhow!("range op exceeded available keyspace"))
            })?;

        let mut key_bytes = [0u8; 32];
        let mut found_unset = false;
        for value in values {
            current_key.to_big_endian(&mut key_bytes);

            let option = self.database.insert::<_, _, Bytes32>(
                MultiKey::new(&(contract_id, key_bytes)).as_ref(),
                Column::ContractsState,
                value,
            )?;

            found_unset |= option.is_none();

            current_key.increase()?;
        }

        if found_unset {
            Ok(None)
        } else {
            Ok(Some(()))
        }
    }

    fn merkle_contract_state_remove_range(
        &mut self,
        contract_id: &ContractId,
        start_key: &Bytes32,
        range: Word,
    ) -> Result<Option<()>, Self::DataError> {
        let mut found_unset = false;

        let mut current_key = U256::from_big_endian(start_key.as_ref());

        for _ in 0..range {
            let mut key_bytes = [0u8; 32];
            current_key.to_big_endian(&mut key_bytes);

            let option = self.database.remove::<Bytes32>(
                MultiKey::new(&(contract_id, key_bytes)).as_ref(),
                Column::ContractsState,
            )?;

            found_unset |= option.is_none();

            current_key.increase()?;
        }

        if found_unset {
            Ok(None)
        } else {
            Ok(Some(()))
        }
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
    use test_case::test_case;

    fn u256_to_bytes32(u: U256) -> Bytes32 {
        let mut bytes = [0u8; 32];
        u.to_big_endian(&mut bytes);
        Bytes32::from(bytes)
    }

    fn setup_value(
        db: &VmDatabase,
        contract_id: ContractId,
        start_key: U256,
        i: usize,
        value: &Bytes32,
    ) {
        let key = start_key.add(i);
        let key = u256_to_bytes32(key);
        let multi_key = MultiKey::new(&(contract_id.as_ref(), key.as_ref()));
        db.database
            .insert::<_, _, Bytes32>(&multi_key, Column::ContractsState, value)
            .unwrap();
    }

    const fn key(k: u8) -> [u8; 32] {
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, k,
        ]
    }

    #[test]
    fn read_single_value() {
        let db = VmDatabase::default();

        let contract_id = ContractId::new([0u8; 32]);
        let key = U256::from(10);
        let key_bytes = u256_to_bytes32(key);
        let value = Bytes32::new([1u8; 32]);

        // check that read is unset before insert
        let pre_read_status = db.merkle_contract_state(&contract_id, &key_bytes).unwrap();
        assert!(pre_read_status.is_none());

        // insert expected key
        setup_value(&db, contract_id, key, 0, &value);

        // check that read is set and returns the correct value
        let read_status = db.merkle_contract_state(&contract_id, &key_bytes).unwrap();
        assert!(read_status.is_some());
        assert_eq!(read_status.unwrap().into_owned(), value);
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
                RANGE_LENGTH as Word,
            )
            .unwrap();
        assert_eq!(results.len(), RANGE_LENGTH);
        results
            .iter()
            .enumerate()
            .for_each(|(i, item)| assert!(item.is_none(), "Expected None for idx {}", i));
    }

    #[test]
    fn read_sequential_set_data() {
        let rng = &mut StdRng::seed_from_u64(100);
        let db = VmDatabase::default();

        const RANGE_LENGTH: usize = 10;
        let contract_id = ContractId::new([0u8; 32]);
        let start_key = U256::zero();

        // check range is unset
        db.merkle_contract_state_range(
            &contract_id,
            &u256_to_bytes32(start_key),
            RANGE_LENGTH as Word,
        )
        .unwrap()
        .iter()
        .for_each(|item| assert!(item.is_none()));

        let setup_values = (0..RANGE_LENGTH)
            .map(|_| rng.gen())
            .collect::<Vec<Bytes32>>();

        // setup data
        for (i, value) in setup_values.iter().enumerate().take(RANGE_LENGTH) {
            setup_value(&db, contract_id, start_key, i, value);
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
            let value =
                value.unwrap_or_else(|| panic!("Expected value to be set at {}", i));
            assert_eq!(value.as_ref(), &setup_values[i]);
        }
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
        for (i, value) in setup_values.iter().enumerate().take(RANGE_LENGTH) {
            if let Some(value) = value {
                setup_value(&db, contract_id, start_key, i, value);
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
        let c1_v1 = rng.gen();
        setup_value(&db, contract_id_1, start_key, 0, &c1_v1);
        let c1_v2 = rng.gen();
        setup_value(&db, contract_id_1, start_key, 1, &c1_v2);
        let c2_v2 = rng.gen();
        setup_value(&db, contract_id_2, start_key, 1, &c2_v2);

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
        setup_value(&db, contract_id_1, start_key, 0, &rng.gen());
        setup_value(&db, contract_id_1, start_key, 1, &rng.gen());
        setup_value(&db, contract_id_2, start_key, 1, &rng.gen());

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
        setup_value(&db, contract_id, start_key, 0, &rng.gen());

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

    #[test_case(
        &[], key(0), &[[1; 32]]
        => Ok(false)
        ; "insert single value over uninitialized range"
    )]
    #[test_case(
        &[(key(0), [0; 32])], key(0), &[[1; 32]]
        => Ok(true)
        ; "insert single value over initialized range"
    )]
    #[test_case(
        &[], key(0), &[[1; 32], [2; 32]]
        => Ok(false)
        ; "insert multiple slots over uninitialized range"
    )]
    #[test_case(
        &[(key(1), [0; 32]), (key(2), [0; 32])], key(0), &[[1; 32], [2; 32], [3; 32]]
        => Ok(false)
        ; "insert multiple slots with uninitialized start of the range"
    )]
    #[test_case(
        &[(key(0), [0; 32]), (key(2), [0; 32])], key(0), &[[1; 32], [2; 32], [3; 32]]
        => Ok(false)
        ; "insert multiple slots with uninitialized middle of the range"
    )]
    #[test_case(
        &[(key(0), [0; 32]), (key(1), [0; 32])], key(0), &[[1; 32], [2; 32], [3; 32]]
        => Ok(false)
        ; "insert multiple slots with uninitialized end of the range"
    )]
    #[test_case(
        &[(key(0), [0; 32]), (key(1), [0; 32]), (key(2), [0; 32])], key(0), &[[1; 32], [2; 32], [3; 32]]
        => Ok(true)
        ; "insert multiple slots over initialized range"
    )]
    #[test_case(
        &[(key(0), [0; 32]), (key(1), [0; 32]), (key(2), [0; 32]), (key(3), [0; 32])], key(1), &[[1; 32], [2; 32]]
        => Ok(true)
        ; "insert multiple slots over sub-range of prefilled data"
    )]
    #[test_case(
        &[], *u256_to_bytes32(U256::MAX), &[[1; 32], [2; 32]]
        => Err(())
        ; "insert fails if start_key + range > u256::MAX"
    )]
    fn insert_range(
        prefilled_slots: &[([u8; 32], [u8; 32])],
        start_key: [u8; 32],
        insertion_range: &[[u8; 32]],
    ) -> Result<bool, ()> {
        let mut db = VmDatabase::default();

        let contract_id = ContractId::new([0u8; 32]);

        // prefill db
        for (key, value) in prefilled_slots {
            let multi_key = MultiKey::new(&(contract_id.as_ref(), key));
            db.database
                .insert::<_, _, Bytes32>(
                    &multi_key,
                    Column::ContractsState,
                    Bytes32::new(*value),
                )
                .unwrap();
        }

        // test insert range
        let insert_status = db
            .merkle_contract_state_insert_range(
                &contract_id,
                &Bytes32::new(start_key),
                &insertion_range
                    .iter()
                    .map(|v| Bytes32::new(*v))
                    .collect::<Vec<_>>(),
            )
            .map_err(|_| ())
            .map(|v| v.is_some());

        // check stored data
        let results: Vec<_> = (0..insertion_range.len())
            .filter_map(|i| {
                let current_key =
                    U256::from_big_endian(&start_key).checked_add(i.into())?;
                let current_key = u256_to_bytes32(current_key);
                let result = db
                    .merkle_contract_state(&contract_id, &current_key)
                    .unwrap()
                    .map(Cow::into_owned)
                    .map(|b| *b);
                result
            })
            .collect();

        // verify all data from insertion request is actually inserted if successful
        // or not inserted at all if unsuccessful
        if insert_status.is_ok() {
            assert_eq!(insertion_range, results);
        } else {
            assert_eq!(results.len(), 0);
        }

        insert_status
    }

    #[test_case(
        &[], [0; 32], 1
        => (vec![], false)
        ; "remove single value over uninitialized range"
    )]
    #[test_case(
        &[([0; 32], [0; 32])], [0; 32], 1
        => (vec![], true)
        ; "remove single value over initialized range"
    )]
    #[test_case(
        &[], [0; 32], 2
        => (vec![], false)
        ; "remove multiple slots over uninitialized range"
    )]
    #[test_case(
        &[([0; 32], [0; 32]), (key(1), [0; 32])], [0; 32], 2
        => (vec![], true)
        ; "remove multiple slots over initialized range"
    )]
    #[test_case(
        &[(key(1), [0; 32]), (key(2), [0; 32])], [0; 32], 3
        => (vec![], false)
        ; "remove multiple slots over partially uninitialized start range"
    )]
    #[test_case(
        &[([0; 32], [0; 32]), (key(1), [0; 32])], [0; 32], 3
        => (vec![], false)
        ; "remove multiple slots over partially uninitialized end range"
    )]
    #[test_case(
        &[([0; 32], [0; 32]), (key(2), [0; 32])], [0; 32], 3
        => (vec![], false)
        ; "remove multiple slots over partially uninitialized middle range"
    )]
    fn remove_range(
        prefilled_slots: &[([u8; 32], [u8; 32])],
        start_key: [u8; 32],
        remove_count: Word,
    ) -> (Vec<[u8; 32]>, bool) {
        let mut db = VmDatabase::default();

        let contract_id = ContractId::new([0u8; 32]);

        // prefill db
        for (key, value) in prefilled_slots {
            let multi_key = MultiKey::new(&(contract_id.as_ref(), key));
            db.database
                .insert::<_, _, Bytes32>(
                    &multi_key,
                    Column::ContractsState,
                    Bytes32::new(*value),
                )
                .unwrap();
        }

        // test remove range
        let remove_status = db
            .merkle_contract_state_remove_range(
                &contract_id,
                &Bytes32::new(start_key),
                remove_count,
            )
            .unwrap()
            .is_some();

        // check stored data
        let results: Vec<_> = (0..(remove_count as usize))
            .filter_map(|i| {
                let current_key = U256::from_big_endian(&start_key) + i;
                let current_key = u256_to_bytes32(current_key);
                let result = db
                    .merkle_contract_state(&contract_id, &current_key)
                    .unwrap()
                    .map(Cow::into_owned)
                    .map(|b| *b);
                result
            })
            .collect();

        (results, remove_status)
    }
}
