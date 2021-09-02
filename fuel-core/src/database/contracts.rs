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

impl Storage<ContractId, Contract> for Database {
    fn insert(
        &mut self,
        key: &ContractId,
        value: &Contract,
    ) -> Result<Option<Contract>, DataError> {
        Database::insert(self, key.as_ref(), CONTRACTS, value.clone()).map_err(Into::into)
    }

    fn remove(&mut self, key: &ContractId) -> Result<Option<Contract>, DataError> {
        Database::remove(self, key.as_ref(), CONTRACTS).map_err(Into::into)
    }

    fn get(&self, key: &ContractId) -> Result<Option<Contract>, DataError> {
        self.get(key.as_ref(), CONTRACTS).map_err(Into::into)
    }

    fn contains_key(&self, key: &ContractId) -> Result<bool, DataError> {
        self.exists(key.as_ref(), CONTRACTS).map_err(Into::into)
    }
}
