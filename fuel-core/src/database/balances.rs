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

impl MerkleStorage<ContractId, Color, Word> for Database {
    fn insert(
        &mut self,
        parent: &ContractId,
        key: &Color,
        value: &Word,
    ) -> Result<Option<Word>, DataError> {
        let key = MultiKey::new((parent, key));
        Database::insert(self, key.as_ref().to_vec(), BALANCES, *value).map_err(Into::into)
    }

    fn remove(&mut self, parent: &ContractId, key: &Color) -> Result<Option<Word>, DataError> {
        let key = MultiKey::new((parent, key));
        Database::remove(self, key.as_ref(), BALANCES).map_err(Into::into)
    }

    fn get(&self, parent: &ContractId, key: &Color) -> Result<Option<Word>, DataError> {
        let key = MultiKey::new((parent, key));
        self.get(key.as_ref(), BALANCES).map_err(Into::into)
    }

    fn contains_key(&self, parent: &ContractId, key: &Color) -> Result<bool, DataError> {
        let key = MultiKey::new((parent, key));
        self.exists(key.as_ref(), BALANCES).map_err(Into::into)
    }

    fn root(&mut self, parent: &ContractId) -> Result<Bytes32, DataError> {
        let items: Vec<_> = Database::iter_all::<Vec<u8>, Word>(self, BALANCES).try_collect()?;

        let root = items
            .iter()
            .filter_map(|(key, value)| {
                (&key[..parent.len()] == parent.as_ref()).then(|| (key, value))
            })
            .sorted_by_key(|t| t.0)
            .map(|(_, value)| value.to_be_bytes());

        Ok(crypto::ephemeral_merkle_root(root))
    }
}
