use crate::state::{Error, KeyValueStore, MultiKey, TransactableStorage};
use fuel_vm::data::DataError;
use fuel_vm::prelude::{Bytes32, Color, Contract, ContractId, Storage, Word};

// used for static DI of database dependencies
pub trait Config {
    type Contracts: TransactableStorage<ContractId, Contract>;
    type Balances: TransactableStorage<MultiKey<ContractId, Color>, Word>;
    type Storage: TransactableStorage<MultiKey<ContractId, Bytes32>, Bytes32>;
}

#[derive(Debug, Default, Clone)]
pub struct Database<T: Config> {
    contracts: T::Contracts,
    balances: T::Balances,
    storage: T::Storage,
}

impl<T: Config> Storage<ContractId, Contract> for Database<T> {
    fn insert(&mut self, key: ContractId, value: Contract) -> Result<Option<Contract>, DataError> {
        self.contracts.put(key, value).map_err(Into::into)
    }

    fn remove(&mut self, key: &ContractId) -> Result<Option<Contract>, DataError> {
        self.contracts.delete(key).map_err(Into::into)
    }

    fn get(&self, key: &ContractId) -> Result<Option<Contract>, DataError> {
        self.contracts.get(key).map_err(Into::into)
    }

    fn contains_key(&self, key: &ContractId) -> Result<bool, DataError> {
        self.contracts.exists(key).map_err(Into::into)
    }
}

impl<T: Config> Storage<(ContractId, Color), Word> for Database<T> {
    fn insert(&mut self, key: (ContractId, Color), value: u64) -> Result<Option<u64>, DataError> {
        let key = MultiKey(key);
        self.balances.put(key, value).map_err(Into::into)
    }

    fn remove(&mut self, key: &(ContractId, Color)) -> Result<Option<u64>, DataError> {
        let key = MultiKey(*key);
        self.balances.delete(&key).map_err(Into::into)
    }

    fn get(&self, key: &(ContractId, Color)) -> Result<Option<u64>, DataError> {
        let key = MultiKey(*key);
        self.balances.get(&key).map_err(Into::into)
    }

    fn contains_key(&self, key: &(ContractId, Color)) -> Result<bool, DataError> {
        let key = MultiKey(*key);
        self.balances.exists(&key).map_err(Into::into)
    }
}

impl<T: Config> Storage<(ContractId, Bytes32), Bytes32> for Database<T> {
    fn insert(
        &mut self,
        key: (ContractId, Bytes32),
        value: Bytes32,
    ) -> Result<Option<Bytes32>, DataError> {
        let key = MultiKey(key);
        self.storage.put(key, value).map_err(Into::into)
    }

    fn remove(&mut self, key: &(ContractId, Bytes32)) -> Result<Option<Bytes32>, DataError> {
        let key = MultiKey(*key);
        self.storage.delete(&key).map_err(Into::into)
    }

    fn get(&self, key: &(ContractId, Bytes32)) -> Result<Option<Bytes32>, DataError> {
        let key = MultiKey(*key);
        self.storage.get(&key).map_err(Into::into)
    }

    fn contains_key(&self, key: &(ContractId, Bytes32)) -> Result<bool, DataError> {
        let key = MultiKey(*key);
        self.storage.exists(&key).map_err(Into::into)
    }
}

impl From<crate::state::Error> for DataError {
    fn from(_: Error) -> Self {
        panic!("DataError is a ZeroVariant enum and cannot be instantiated")
    }
}
