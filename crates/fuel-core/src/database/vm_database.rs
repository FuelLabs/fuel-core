use crate::database::{
    Database,
    Error as DatabaseError,
};
use anyhow::anyhow;
use fuel_core_storage::{
    not_found,
    tables::ContractsState,
    ContractsAssetsStorage,
    ContractsStateKey,
    Error as StorageError,
    Mappable,
    MerkleRoot,
    MerkleRootStorage,
    StorageAsMut,
    StorageInspect,
    StorageMutate,
    StorageRead,
    StorageSize,
};
use fuel_core_types::{
    blockchain::header::ConsensusHeader,
    fuel_tx::{
        Contract,
        StorageSlot,
    },
    fuel_types::{
        BlockHeight,
        Bytes32,
        ContractId,
        Salt,
        Word,
    },
    fuel_vm::InterpreterStorage,
    tai64::Tai64,
};
use primitive_types::U256;
use std::borrow::Cow;

/// Used to store metadata relevant during the execution of a transaction
#[derive(Clone, Debug)]
pub struct VmDatabase {
    current_block_height: BlockHeight,
    current_timestamp: Tai64,
    coinbase: ContractId,
    database: Database,
}

pub trait IncreaseStorageKey {
    fn increase(&mut self) -> anyhow::Result<()>;
}

impl IncreaseStorageKey for U256 {
    fn increase(&mut self) -> anyhow::Result<()> {
        *self = self
            .checked_add(1.into())
            .ok_or_else(|| anyhow!("range op exceeded available keyspace"))?;
        Ok(())
    }
}

impl Default for VmDatabase {
    fn default() -> Self {
        Self {
            current_block_height: Default::default(),
            current_timestamp: Tai64::now(),
            coinbase: Default::default(),
            database: Default::default(),
        }
    }
}

impl VmDatabase {
    pub fn new<T>(
        database: Database,
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

    pub fn default_from_database(database: Database) -> Self {
        Self {
            database,
            ..Default::default()
        }
    }

    pub fn database_mut(&mut self) -> &mut Database {
        &mut self.database
    }
}

impl<M: Mappable> StorageInspect<M> for VmDatabase
where
    Database: StorageInspect<M, Error = StorageError>,
{
    type Error = StorageError;

    fn get(&self, key: &M::Key) -> Result<Option<Cow<M::OwnedValue>>, Self::Error> {
        StorageInspect::<M>::get(&self.database, key)
    }

    fn contains_key(&self, key: &M::Key) -> Result<bool, Self::Error> {
        StorageInspect::<M>::contains_key(&self.database, key)
    }
}

impl<M: Mappable> StorageMutate<M> for VmDatabase
where
    Database: StorageMutate<M, Error = StorageError>,
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

impl<M: Mappable> StorageSize<M> for VmDatabase
where
    Database: StorageSize<M, Error = StorageError>,
{
    fn size_of_value(&self, key: &M::Key) -> Result<Option<usize>, Self::Error> {
        StorageSize::<M>::size_of_value(&self.database, key)
    }
}

impl<M: Mappable> StorageRead<M> for VmDatabase
where
    Database: StorageRead<M, Error = StorageError>,
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

impl<K, M: Mappable> MerkleRootStorage<K, M> for VmDatabase
where
    Database: MerkleRootStorage<K, M, Error = StorageError>,
{
    fn root(&self, key: &K) -> Result<MerkleRoot, Self::Error> {
        MerkleRootStorage::<K, M>::root(&self.database, key)
    }
}

impl ContractsAssetsStorage for VmDatabase {}

impl InterpreterStorage for VmDatabase {
    type DataError = StorageError;

    fn block_height(&self) -> Result<BlockHeight, Self::DataError> {
        Ok(self.current_block_height)
    }

    fn timestamp(&self, height: BlockHeight) -> Result<Word, Self::DataError> {
        let timestamp = match height {
            // panic if $rB is greater than the current block height.
            height if height > self.current_block_height => {
                return Err(anyhow!("block height too high for timestamp").into())
            }
            height if height == self.current_block_height => self.current_timestamp,
            height => self.database.block_time(&height)?,
        };
        Ok(timestamp.0)
    }

    fn block_hash(&self, block_height: BlockHeight) -> Result<Bytes32, Self::DataError> {
        // Block header hashes for blocks with height greater than or equal to current block height are zero (0x00**32).
        // https://github.com/FuelLabs/fuel-specs/blob/master/specs/vm/instruction_set.md#bhsh-block-hash
        if block_height >= self.current_block_height || block_height == Default::default()
        {
            Ok(Bytes32::zeroed())
        } else {
            // this will return 0x00**32 for block height 0 as well
            self.database
                .get_block_id(&block_height)?
                .ok_or(not_found!("BlockId"))
                .map(Into::into)
        }
    }

    fn coinbase(&self) -> Result<ContractId, Self::DataError> {
        Ok(self.coinbase)
    }

    fn deploy_contract_with_id(
        &mut self,
        salt: &Salt,
        slots: &[StorageSlot],
        contract: &Contract,
        root: &Bytes32,
        id: &ContractId,
    ) -> Result<(), Self::DataError> {
        self.storage_contract_insert(id, contract)?;
        self.storage_contract_root_insert(id, salt, root)?;

        self.database.init_contract_state(
            id,
            slots.iter().map(|slot| (*slot.key(), *slot.value())),
        )
    }

    fn merkle_contract_state_range(
        &self,
        contract_id: &ContractId,
        start_key: &Bytes32,
        range: usize,
    ) -> Result<Vec<Option<Cow<Bytes32>>>, Self::DataError> {
        use fuel_core_storage::StorageAsRef;

        let mut key = U256::from_big_endian(start_key.as_ref());
        let mut state_key = Bytes32::zeroed();

        let mut results = Vec::new();
        for _ in 0..range {
            key.to_big_endian(state_key.as_mut());
            let multikey = ContractsStateKey::new(contract_id, &state_key);
            results.push(self.database.storage::<ContractsState>().get(&multikey)?);
            key.increase()?;
        }
        Ok(results)
    }

    fn merkle_contract_state_insert_range(
        &mut self,
        contract_id: &ContractId,
        start_key: &Bytes32,
        values: &[Bytes32],
    ) -> Result<usize, Self::DataError> {
        let mut current_key = U256::from_big_endian(start_key.as_ref());
        // verify key is in range
        current_key
            .checked_add(U256::from(values.len()))
            .ok_or_else(|| {
                DatabaseError::Other(anyhow!("range op exceeded available keyspace"))
            })?;

        let mut key_bytes = Bytes32::zeroed();
        let mut found_unset = 0u32;
        for value in values {
            current_key.to_big_endian(key_bytes.as_mut());

            let option = self
                .database
                .storage::<ContractsState>()
                .insert(&(contract_id, &key_bytes).into(), value)?;

            if option.is_none() {
                found_unset = found_unset
                    .checked_add(1)
                    .expect("We've checked it above via `values.len()`");
            }

            current_key.increase()?;
        }

        Ok(found_unset as usize)
    }

    fn merkle_contract_state_remove_range(
        &mut self,
        contract_id: &ContractId,
        start_key: &Bytes32,
        range: usize,
    ) -> Result<Option<()>, Self::DataError> {
        let mut found_unset = false;

        let mut current_key = U256::from_big_endian(start_key.as_ref());

        let mut key_bytes = Bytes32::zeroed();
        for _ in 0..range {
            current_key.to_big_endian(key_bytes.as_mut());

            let option = self
                .database
                .storage::<ContractsState>()
                .remove(&(contract_id, &key_bytes).into())?;

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
    use test_case::test_case;

    fn u256_to_bytes32(u: U256) -> Bytes32 {
        let mut bytes = [0u8; 32];
        u.to_big_endian(&mut bytes);
        Bytes32::from(bytes)
    }

    const fn key(k: u8) -> [u8; 32] {
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, k,
        ]
    }

    #[test_case(
        &[], key(0), 1
        => Ok(vec![None])
        ; "read single uninitialized value"
    )]
    #[test_case(
        &[(key(0), [0; 32])], key(0), 1
        => Ok(vec![Some([0; 32])])
        ; "read single initialized value"
    )]
    #[test_case(
        &[], key(0), 3
        => Ok(vec![None, None, None])
        ; "read uninitialized range"
    )]
    #[test_case(
        &[(key(1), [1; 32]), (key(2), [2; 32])], key(0), 3
        => Ok(vec![None, Some([1; 32]), Some([2; 32])])
        ; "read uninitialized start range"
    )]
    #[test_case(
        &[(key(0), [0; 32]), (key(2), [2; 32])], key(0), 3
        => Ok(vec![Some([0; 32]), None, Some([2; 32])])
        ; "read uninitialized middle range"
    )]
    #[test_case(
        &[(key(0), [0; 32]), (key(1), [1; 32])], key(0), 3
        => Ok(vec![Some([0; 32]), Some([1; 32]), None])
        ; "read uninitialized end range"
    )]
    #[test_case(
        &[(key(0), [0; 32]), (key(1), [1; 32]), (key(2), [2; 32])], key(0), 3
        => Ok(vec![Some([0; 32]), Some([1; 32]), Some([2; 32])])
        ; "read fully initialized range"
    )]
    #[test_case(
        &[(key(0), [0; 32]), (key(1), [1; 32]), (key(2), [2; 32])], key(0), 2
        => Ok(vec![Some([0; 32]), Some([1; 32])])
        ; "read subset of initialized range"
    )]
    #[test_case(
        &[(key(0), [0; 32]), (key(2), [2; 32])], key(0), 2
        => Ok(vec![Some([0; 32]), None])
        ; "read subset of partially set range without running too far"
    )]
    #[test_case(
        &[], *u256_to_bytes32(U256::MAX), 2
        => Err(())
        ; "read fails on uninitialized range if keyspace exceeded"
    )]
    #[test_case(
        &[(*u256_to_bytes32(U256::MAX), [0; 32])], *u256_to_bytes32(U256::MAX), 2
        => Err(())
        ; "read fails on partially initialized range if keyspace exceeded"
    )]
    fn read_sequential_range(
        prefilled_slots: &[([u8; 32], [u8; 32])],
        start_key: [u8; 32],
        range: usize,
    ) -> Result<Vec<Option<[u8; 32]>>, ()> {
        let mut db = VmDatabase::default();

        let contract_id = ContractId::new([0u8; 32]);

        // prefill db
        for (key, value) in prefilled_slots {
            let key = Bytes32::from(*key);
            let value = Bytes32::new(*value);
            db.database
                .storage::<ContractsState>()
                .insert(&(&contract_id, &key).into(), &value)
                .unwrap();
        }

        // perform sequential read
        Ok(db
            .merkle_contract_state_range(&contract_id, &Bytes32::new(start_key), range)
            .map_err(|_| ())?
            .into_iter()
            .map(|v| v.map(Cow::into_owned).map(|v| *v))
            .collect())
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
            let key = Bytes32::from(*key);
            let value = Bytes32::new(*value);
            db.database
                .storage::<ContractsState>()
                .insert(&(&contract_id, &key).into(), &value)
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
            .map(|v| v == 0);

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
        remove_count: usize,
    ) -> (Vec<[u8; 32]>, bool) {
        let mut db = VmDatabase::default();

        let contract_id = ContractId::new([0u8; 32]);

        // prefill db
        for (key, value) in prefilled_slots {
            let key = Bytes32::from(*key);
            let value = Bytes32::new(*value);
            db.database
                .storage::<ContractsState>()
                .insert(&(&contract_id, &key).into(), &value)
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
        let results: Vec<_> = (0..remove_count)
            .filter_map(|i| {
                let (current_key, overflow) =
                    U256::from_big_endian(&start_key).overflowing_add(i.into());

                if overflow {
                    return None
                }

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
