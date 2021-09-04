#[cfg(feature = "default")]
use crate::database::columns::COLUMN_NUM;
use crate::database::columns::{BALANCES, CONTRACTS, CONTRACTS_CODE_ROOT, CONTRACTS_STATE};
use crate::database::{Database, DatabaseTrait};
use crate::state::in_memory::memory_store::MemoryStore;
use crate::state::in_memory::transaction::MemoryTransactionView;
#[cfg(feature = "default")]
use crate::state::rocks_db::RocksDb;
use crate::state::{ColumnId, DataSource, Error, MultiKey};
use fuel_vm::crypto;
use fuel_vm::data::{DataError, InterpreterStorage, MerkleStorage};
use fuel_vm::prelude::{Address, Bytes32, Color, Contract, ContractId, Salt, Storage, Word};
use itertools::Itertools;
use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Debug;
#[cfg(feature = "default")]
use std::path::Path;
use std::sync::Arc;

impl Storage<ContractId, (Salt, Bytes32)> for Database {
    fn insert(
        &mut self,
        key: &ContractId,
        value: &(Salt, Bytes32),
    ) -> Result<Option<(Salt, Bytes32)>, DataError> {
        Database::insert(&self, key.as_ref(), CONTRACTS_CODE_ROOT, value.clone())
            .map_err(Into::into)
    }

    fn remove(&mut self, key: &ContractId) -> Result<Option<(Salt, Bytes32)>, DataError> {
        Database::remove(&self, key.as_ref(), CONTRACTS_CODE_ROOT).map_err(Into::into)
    }

    fn get(&self, key: &ContractId) -> Result<Option<(Salt, Bytes32)>, DataError> {
        Database::get(&self, key.as_ref(), CONTRACTS_CODE_ROOT).map_err(Into::into)
    }

    fn contains_key(&self, key: &ContractId) -> Result<bool, DataError> {
        Database::exists(&self, key.as_ref(), CONTRACTS_CODE_ROOT).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};

    #[test]
    fn get() {
        let rng = &mut StdRng::seed_from_u64(2322u64);
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let contract: Contract = Contract::from(vec![32u8]);
        let root = contract.root();
        let salt: Salt = rng.gen();

        let database = Database::default();
        database
            .insert(
                contract_id.as_ref().to_vec(),
                CONTRACTS_CODE_ROOT,
                (salt, root).clone(),
            )
            .unwrap();

        assert_eq!(
            Storage::<ContractId, (Salt, Bytes32)>::get(&database, &contract_id)
                .unwrap()
                .unwrap(),
            (salt, root)
        );
    }

    #[test]
    fn put() {
        let rng = &mut StdRng::seed_from_u64(2322u64);
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let contract: Contract = Contract::from(vec![32u8]);
        let root = contract.root();
        let salt: Salt = rng.gen();

        let mut database = Database::default();
        Storage::<ContractId, (Salt, Bytes32)>::insert(&mut database, &contract_id, &(salt, root))
            .unwrap();

        let returned: (Salt, Bytes32) = database
            .get(contract_id.as_ref(), CONTRACTS_CODE_ROOT)
            .unwrap()
            .unwrap();
        assert_eq!(returned, (salt, root));
    }

    #[test]
    fn remove() {
        let rng = &mut StdRng::seed_from_u64(2322u64);
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let contract: Contract = Contract::from(vec![32u8]);
        let root = contract.root();
        let salt: Salt = rng.gen();

        let mut database = Database::default();
        database
            .insert(
                contract_id.as_ref().to_vec(),
                CONTRACTS_CODE_ROOT,
                (salt, root).clone(),
            )
            .unwrap();

        Storage::<ContractId, (Salt, Bytes32)>::remove(&mut database, &contract_id).unwrap();

        assert!(!database
            .exists(contract_id.as_ref(), CONTRACTS_CODE_ROOT)
            .unwrap());
    }

    #[test]
    fn exists() {
        let rng = &mut StdRng::seed_from_u64(2322u64);
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let contract: Contract = Contract::from(vec![32u8]);
        let root = contract.root();
        let salt: Salt = rng.gen();

        let database = Database::default();
        database
            .insert(
                contract_id.as_ref().to_vec(),
                CONTRACTS_CODE_ROOT,
                (salt, root),
            )
            .unwrap();

        assert!(
            Storage::<ContractId, (Salt, Bytes32)>::contains_key(&database, &contract_id).unwrap()
        );
    }
}
