use crate::state::in_memory::memory_store::MemoryStore;
use crate::state::in_memory::transaction::MemoryTransactionView;
use crate::state::{
    BatchOperations, Error, KeyValueStore, MultiKey, Transaction, TransactionResult,
};
use fuel_vm::data::{DataError, InterpreterStorage};
use fuel_vm::prelude::{Bytes32, Color, Contract, ContractId, Storage, Word};
use std::fmt::Debug;
use std::marker::PhantomData;

// used for static DI of database dependencies
pub trait Config: Clone {
    type Contracts: KeyValueStore<ContractId, Contract>
        + BatchOperations<ContractId, Contract>
        + Clone
        + Debug
        + Default
        + Send
        + Sync;
    type Balances: KeyValueStore<MultiKey<ContractId, Color>, Word>
        + BatchOperations<MultiKey<ContractId, Color>, Word>
        + Clone
        + Debug
        + Default
        + Send
        + Sync;
    type Storage: KeyValueStore<MultiKey<ContractId, Bytes32>, Bytes32>
        + BatchOperations<MultiKey<ContractId, Bytes32>, Bytes32>
        + Clone
        + Debug
        + Default
        + Send
        + Sync;
}

#[derive(Clone, Debug, Default)]
pub struct MemoryDatabaseConfig {}

impl Config for MemoryDatabaseConfig {
    type Contracts = MemoryStore<ContractId, Contract>;
    type Balances = MemoryStore<MultiKey<ContractId, Color>, Word>;
    type Storage = MemoryStore<MultiKey<ContractId, Bytes32>, Bytes32>;
}

#[derive(Clone, Debug, Default)]
pub struct TransactionConfig<T> {
    _marker: PhantomData<T>,
}

impl<T: Config> Config for TransactionConfig<T> {
    type Contracts = MemoryTransactionView<ContractId, Contract, T::Contracts>;
    type Balances = MemoryTransactionView<MultiKey<ContractId, Color>, Word, T::Balances>;
    type Storage = MemoryTransactionView<MultiKey<ContractId, Bytes32>, Bytes32, T::Storage>;
}

#[derive(Debug, Default, Clone)]
pub struct Database<T: Config> {
    contracts: T::Contracts,
    balances: T::Balances,
    storage: T::Storage,
}

impl<T: Config> Database<T> {
    fn transaction<F, R>(&self, f: F) -> TransactionResult<R>
    where
        F: FnOnce(Database<TransactionConfig<T>>) -> TransactionResult<R> + std::marker::Copy,
    {
        let mut contracts = self.contracts.clone();
        let storage = self.storage.clone();
        let balances = self.balances.clone();

        contracts.transaction(|contracts_view| {
            let mut storage = storage.clone();
            let balances = balances.clone();
            storage.transaction(|storage_view| {
                let mut balances = balances.clone();
                balances.transaction(|balances_view| {
                    let tx_db = Database::<TransactionConfig<T>> {
                        contracts: contracts_view.clone(),
                        balances: balances_view.clone(),
                        storage: storage_view.clone(),
                    };
                    f(tx_db)
                })
            })
        })
    }
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

impl<T: Config> InterpreterStorage for Database<T> {}

impl From<crate::state::Error> for DataError {
    fn from(_: Error) -> Self {
        panic!("DataError is a ZeroVariant enum and cannot be instantiated")
    }
}
