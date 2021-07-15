use crate::state::{DataSource, Error, KeyValueStore, MultiKey, TransactableStorage};
use fuel_vm::data::DataError;
use fuel_vm::prelude::{Bytes32, Color, Contract, ContractId, Storage, Word};
use std::sync::{Arc, RwLock};

#[derive(Debug, Default, Clone)]
pub struct Database<T: Config> {
    contracts: DataSource<ContractId, Contract>,
    balances: DataSource<MultiKey<ContractId, Color>, Word>,
    storage: DataSource<MultiKey<ContractId, Bytes32>, Bytes32>,
}

impl<T: Config> Storage<ContractId, Contract> for Database<T> {
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

impl<T: Config> Storage<(ContractId, Color), Word> for Database<T> {
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

impl<T: Config> Storage<(ContractId, Bytes32), Bytes32> for Database<T> {
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

impl From<crate::state::Error> for DataError {
    fn from(_: Error) -> Self {
        panic!("DataError is a ZeroVariant enum and cannot be instantiated")
    }
}
