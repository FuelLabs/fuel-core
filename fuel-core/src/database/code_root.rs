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
