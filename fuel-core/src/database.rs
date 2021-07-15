use crate::state::in_memory::memory_store::MemoryStore;
use crate::state::{DataSource, Error, KeyValueStore, MultiKey, TransactableStorage};
use fuel_vm::data::{DataError, InterpreterStorage};
use fuel_vm::prelude::{Bytes32, Color, Contract, ContractId, Storage, Word};
use std::sync::{Arc, RwLock};

#[derive(Debug)]
pub struct Database {
    contracts: DataSource<ContractId, Contract>,
    balances: DataSource<MultiKey<ContractId, Color>, Word>,
    storage: DataSource<MultiKey<ContractId, Bytes32>, Bytes32>,
}

impl Default for Database {
    fn default() -> Self {
        Self {
            contracts: Arc::new(RwLock::new(MemoryStore::default())),
            balances: Arc::new(RwLock::new(MemoryStore::default())),
            storage: Arc::new(RwLock::new(MemoryStore::default())),
        }
    }
}

impl Storage<ContractId, Contract> for Database {
    fn insert(&mut self, key: ContractId, value: Contract) -> Result<Option<Contract>, DataError> {
        self.contracts
            .write()
            .expect("lock poisoned")
            .put(key, value)
            .map_err(Into::into)
    }

    fn remove(&mut self, key: &ContractId) -> Result<Option<Contract>, DataError> {
        self.contracts
            .write()
            .expect("lock poisoned")
            .delete(key)
            .map_err(Into::into)
    }

    fn get(&self, key: &ContractId) -> Result<Option<Contract>, DataError> {
        self.contracts
            .read()
            .expect("lock poisoned")
            .get(key)
            .map_err(Into::into)
    }

    fn contains_key(&self, key: &ContractId) -> Result<bool, DataError> {
        self.contracts
            .read()
            .expect("lock poisoned")
            .exists(key)
            .map_err(Into::into)
    }
}

impl Storage<(ContractId, Color), Word> for Database {
    fn insert(&mut self, key: (ContractId, Color), value: u64) -> Result<Option<u64>, DataError> {
        let key = MultiKey(key);
        self.balances
            .write()
            .expect("lock poisoned")
            .put(key, value)
            .map_err(Into::into)
    }

    fn remove(&mut self, key: &(ContractId, Color)) -> Result<Option<u64>, DataError> {
        let key = MultiKey(*key);
        self.balances
            .write()
            .expect("lock poisoned")
            .delete(&key)
            .map_err(Into::into)
    }

    fn get(&self, key: &(ContractId, Color)) -> Result<Option<u64>, DataError> {
        let key = MultiKey(*key);
        self.balances
            .read()
            .expect("lock poisoned")
            .get(&key)
            .map_err(Into::into)
    }

    fn contains_key(&self, key: &(ContractId, Color)) -> Result<bool, DataError> {
        let key = MultiKey(*key);
        self.balances
            .read()
            .expect("lock poisoned")
            .exists(&key)
            .map_err(Into::into)
    }
}

impl Storage<(ContractId, Bytes32), Bytes32> for Database {
    fn insert(
        &mut self,
        key: (ContractId, Bytes32),
        value: Bytes32,
    ) -> Result<Option<Bytes32>, DataError> {
        let key = MultiKey(key);
        self.storage
            .write()
            .expect("lock poisoned")
            .put(key, value)
            .map_err(Into::into)
    }

    fn remove(&mut self, key: &(ContractId, Bytes32)) -> Result<Option<Bytes32>, DataError> {
        let key = MultiKey(*key);
        self.storage
            .write()
            .expect("lock poisoned")
            .delete(&key)
            .map_err(Into::into)
    }

    fn get(&self, key: &(ContractId, Bytes32)) -> Result<Option<Bytes32>, DataError> {
        let key = MultiKey(*key);
        self.storage
            .read()
            .expect("lock poisoned")
            .get(&key)
            .map_err(Into::into)
    }

    fn contains_key(&self, key: &(ContractId, Bytes32)) -> Result<bool, DataError> {
        let key = MultiKey(*key);
        self.storage
            .read()
            .expect("lock poisoned")
            .exists(&key)
            .map_err(Into::into)
    }
}

impl InterpreterStorage for Database {}

impl From<crate::state::Error> for DataError {
    fn from(_: Error) -> Self {
        panic!("DataError is a ZeroVariant enum and cannot be instantiated")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TODO: verify simple crud for each data type using in-memory impl
}
