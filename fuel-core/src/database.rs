use crate::state::in_memory::memory_store::MemoryStore;
use crate::state::in_memory::transaction::MemoryTransactionView;
use crate::state::{DataSource, Error, MultiKey};
use fuel_vm::data::{DataError, InterpreterStorage};
use fuel_vm::prelude::{Bytes32, Color, Contract, ContractId, Storage, Word};
use std::fmt::Debug;
use std::sync::{Arc, Mutex};

pub trait DatabaseTrait: InterpreterStorage + Debug {
    fn transaction(&self) -> DatabaseTransaction;
}

#[derive(Clone, Debug)]
pub struct Database {
    contracts: DataSource<ContractId, Contract>,
    balances: DataSource<MultiKey<ContractId, Color>, Word>,
    storage: DataSource<MultiKey<ContractId, Bytes32>, Bytes32>,
}

impl DatabaseTrait for Database {
    fn transaction(&self) -> DatabaseTransaction {
        self.into()
    }
}

impl Default for Database {
    fn default() -> Self {
        Self {
            contracts: Arc::new(Mutex::new(MemoryStore::default())),
            balances: Arc::new(Mutex::new(MemoryStore::default())),
            storage: Arc::new(Mutex::new(MemoryStore::default())),
        }
    }
}

impl Storage<ContractId, Contract> for Database {
    fn insert(&mut self, key: ContractId, value: Contract) -> Result<Option<Contract>, DataError> {
        self.contracts
            .lock()
            .expect("lock poisoned")
            .put(key, value)
            .map_err(Into::into)
    }

    fn remove(&mut self, key: &ContractId) -> Result<Option<Contract>, DataError> {
        self.contracts
            .lock()
            .expect("lock poisoned")
            .delete(key)
            .map_err(Into::into)
    }

    fn get(&self, key: &ContractId) -> Result<Option<Contract>, DataError> {
        self.contracts
            .lock()
            .expect("lock poisoned")
            .get(key)
            .map_err(Into::into)
    }

    fn contains_key(&self, key: &ContractId) -> Result<bool, DataError> {
        self.contracts
            .lock()
            .expect("lock poisoned")
            .exists(key)
            .map_err(Into::into)
    }
}

impl Storage<(ContractId, Color), Word> for Database {
    fn insert(&mut self, key: (ContractId, Color), value: u64) -> Result<Option<u64>, DataError> {
        let key = MultiKey::new(key);
        self.balances
            .lock()
            .expect("lock poisoned")
            .put(key, value)
            .map_err(Into::into)
    }

    fn remove(&mut self, key: &(ContractId, Color)) -> Result<Option<u64>, DataError> {
        let key = MultiKey::new(*key);
        self.balances
            .lock()
            .expect("lock poisoned")
            .delete(&key)
            .map_err(Into::into)
    }

    fn get(&self, key: &(ContractId, Color)) -> Result<Option<u64>, DataError> {
        let key = MultiKey::new(*key);
        self.balances
            .lock()
            .expect("lock poisoned")
            .get(&key)
            .map_err(Into::into)
    }

    fn contains_key(&self, key: &(ContractId, Color)) -> Result<bool, DataError> {
        let key = MultiKey::new(*key);
        self.balances
            .lock()
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
        let key = MultiKey::new(key);
        self.storage
            .lock()
            .expect("lock poisoned")
            .put(key, value)
            .map_err(Into::into)
    }

    fn remove(&mut self, key: &(ContractId, Bytes32)) -> Result<Option<Bytes32>, DataError> {
        let key = MultiKey::new(*key);
        self.storage
            .lock()
            .expect("lock poisoned")
            .delete(&key)
            .map_err(Into::into)
    }

    fn get(&self, key: &(ContractId, Bytes32)) -> Result<Option<Bytes32>, DataError> {
        let key = MultiKey::new(*key);
        self.storage
            .lock()
            .expect("lock poisoned")
            .get(&key)
            .map_err(Into::into)
    }

    fn contains_key(&self, key: &(ContractId, Bytes32)) -> Result<bool, DataError> {
        let key = MultiKey::new(*key);
        self.storage
            .lock()
            .expect("lock poisoned")
            .exists(&key)
            .map_err(Into::into)
    }
}

impl InterpreterStorage for Database {}

#[derive(Clone, Debug)]
pub struct DatabaseTransaction {
    // The primary datastores
    contracts: Arc<Mutex<MemoryTransactionView<ContractId, Contract>>>,
    balances: Arc<Mutex<MemoryTransactionView<MultiKey<ContractId, Color>, Word>>>,
    storage: Arc<Mutex<MemoryTransactionView<MultiKey<ContractId, Bytes32>, Bytes32>>>,
    // The inner db impl using these stores
    database: Database,
}

impl Default for DatabaseTransaction {
    fn default() -> Self {
        Database::default().transaction()
    }
}

impl DatabaseTransaction {
    pub fn commit(self) {
        self.contracts
            .lock()
            .expect("lock poisoned")
            .commit()
            .expect("todo: error handling");
        self.balances
            .lock()
            .expect("lock poisoned")
            .commit()
            .expect("todo: error handling");
        self.storage
            .lock()
            .expect("lock poisoned")
            .commit()
            .expect("todo: error handling");
    }
}

impl Storage<ContractId, Contract> for DatabaseTransaction {
    fn insert(&mut self, key: ContractId, value: Contract) -> Result<Option<Contract>, DataError> {
        self.database.insert(key, value)
    }

    fn remove(&mut self, key: &ContractId) -> Result<Option<Contract>, DataError> {
        self.database.remove(key)
    }

    fn get(&self, key: &ContractId) -> Result<Option<Contract>, DataError> {
        self.database.get(key)
    }

    fn contains_key(&self, key: &ContractId) -> Result<bool, DataError> {
        self.database.contains_key(key)
    }
}

impl Storage<(ContractId, Color), Word> for DatabaseTransaction {
    fn insert(&mut self, key: (ContractId, Color), value: Word) -> Result<Option<Word>, DataError> {
        self.database.insert(key, value)
    }

    fn remove(&mut self, key: &(ContractId, Color)) -> Result<Option<Word>, DataError> {
        self.database.remove(key)
    }

    fn get(&self, key: &(ContractId, Color)) -> Result<Option<Word>, DataError> {
        self.database.get(key)
    }

    fn contains_key(&self, key: &(ContractId, Color)) -> Result<bool, DataError> {
        self.database.contains_key(key)
    }
}

impl Storage<(ContractId, Bytes32), Bytes32> for DatabaseTransaction {
    fn insert(
        &mut self,
        key: (ContractId, Bytes32),
        value: Bytes32,
    ) -> Result<Option<Bytes32>, DataError> {
        self.database.insert(key, value)
    }

    fn remove(&mut self, key: &(ContractId, Bytes32)) -> Result<Option<Bytes32>, DataError> {
        self.database.remove(key)
    }

    fn get(&self, key: &(ContractId, Bytes32)) -> Result<Option<Bytes32>, DataError> {
        self.database.get(key)
    }

    fn contains_key(&self, key: &(ContractId, Bytes32)) -> Result<bool, DataError> {
        self.database.contains_key(key)
    }
}

impl From<&Database> for DatabaseTransaction {
    fn from(source: &Database) -> Self {
        let contracts = Arc::new(Mutex::new(MemoryTransactionView::new(
            source.contracts.clone(),
        )));
        let balances = Arc::new(Mutex::new(MemoryTransactionView::new(
            source.balances.clone(),
        )));
        let storage = Arc::new(Mutex::new(MemoryTransactionView::new(
            source.storage.clone(),
        )));

        Self {
            contracts: contracts.clone(),
            balances: balances.clone(),
            storage: storage.clone(),
            database: Database {
                contracts,
                balances,
                storage,
            },
        }
    }
}

impl DatabaseTrait for DatabaseTransaction {
    fn transaction(&self) -> DatabaseTransaction {
        (&self.database).into()
    }
}

impl InterpreterStorage for DatabaseTransaction {}

impl From<crate::state::Error> for DataError {
    fn from(_: Error) -> Self {
        panic!("DataError is a ZeroVariant enum and cannot be instantiated")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod contracts {
        use super::*;
        use crate::state::KeyValueStore;

        #[test]
        fn get() {
            let contract_id: ContractId = ContractId::from([1u8; 32]);
            let contract: Contract = Contract::from(vec![32u8]);

            let contracts = Arc::new(Mutex::new(MemoryStore::default()));
            let database = Database {
                contracts: contracts.clone(),
                ..Default::default()
            };
            contracts
                .lock()
                .unwrap()
                .put(contract_id, contract.clone())
                .unwrap();

            assert_eq!(
                Storage::<ContractId, Contract>::get(&database, &contract_id)
                    .unwrap()
                    .unwrap(),
                contract
            );
        }

        #[test]
        fn put() {
            let contract_id: ContractId = ContractId::from([1u8; 32]);
            let contract: Contract = Contract::from(vec![32u8]);

            let contracts = Arc::new(Mutex::new(MemoryStore::default()));
            let mut database = Database {
                contracts: contracts.clone(),
                ..Default::default()
            };
            Storage::<ContractId, Contract>::insert(&mut database, contract_id, contract.clone())
                .unwrap();

            assert_eq!(
                contracts
                    .lock()
                    .unwrap()
                    .get(&contract_id)
                    .unwrap()
                    .unwrap(),
                contract
            );
        }

        #[test]
        fn remove() {
            let contract_id: ContractId = ContractId::from([1u8; 32]);
            let contract: Contract = Contract::from(vec![32u8]);

            let contracts = Arc::new(Mutex::new(MemoryStore::default()));
            let mut database = Database {
                contracts: contracts.clone(),
                ..Default::default()
            };
            contracts
                .lock()
                .unwrap()
                .put(contract_id, contract.clone())
                .unwrap();

            Storage::<ContractId, Contract>::remove(&mut database, &contract_id).unwrap();

            assert!(!contracts.lock().unwrap().exists(&contract_id).unwrap());
        }

        #[test]
        fn exists() {
            let contract_id: ContractId = ContractId::from([1u8; 32]);
            let contract: Contract = Contract::from(vec![32u8]);

            let contracts = Arc::new(Mutex::new(MemoryStore::default()));
            let database = Database {
                contracts: contracts.clone(),
                ..Default::default()
            };
            contracts
                .lock()
                .unwrap()
                .put(contract_id, contract.clone())
                .unwrap();

            assert!(
                Storage::<ContractId, Contract>::contains_key(&database, &contract_id).unwrap()
            );
        }
    }

    mod balances {
        use super::*;
        use crate::state::KeyValueStore;

        #[test]
        fn get() {
            let balance_id: (ContractId, Color) =
                (ContractId::from([1u8; 32]), Color::new([1u8; 32]));
            let balance: Word = 100;

            let balances = Arc::new(Mutex::new(MemoryStore::default()));
            let database = Database {
                balances: balances.clone(),
                ..Default::default()
            };
            balances
                .lock()
                .unwrap()
                .put(MultiKey::new(balance_id), balance.clone())
                .unwrap();

            assert_eq!(
                Storage::<(ContractId, Color), Word>::get(&database, &balance_id)
                    .unwrap()
                    .unwrap(),
                balance
            );
        }

        #[test]
        fn put() {
            let balance_id: (ContractId, Color) =
                (ContractId::from([1u8; 32]), Color::new([1u8; 32]));
            let balance: Word = 100;

            let balances = Arc::new(Mutex::new(MemoryStore::default()));
            let mut database = Database {
                balances: balances.clone(),
                ..Default::default()
            };
            Storage::<(ContractId, Color), Word>::insert(
                &mut database,
                balance_id,
                balance.clone(),
            )
            .unwrap();

            assert_eq!(
                balances
                    .lock()
                    .unwrap()
                    .get(&MultiKey::new(balance_id))
                    .unwrap()
                    .unwrap(),
                balance
            );
        }

        #[test]
        fn remove() {
            let balance_id: (ContractId, Color) =
                (ContractId::from([1u8; 32]), Color::new([1u8; 32]));
            let balance: Word = 100;

            let balances = Arc::new(Mutex::new(MemoryStore::default()));
            let mut database = Database {
                balances: balances.clone(),
                ..Default::default()
            };
            balances
                .lock()
                .unwrap()
                .put(MultiKey::new(balance_id), balance.clone())
                .unwrap();

            Storage::<(ContractId, Color), Word>::remove(&mut database, &balance_id).unwrap();

            assert!(!balances
                .lock()
                .unwrap()
                .exists(&MultiKey::new(balance_id))
                .unwrap());
        }

        #[test]
        fn exists() {
            let balance_id: (ContractId, Color) =
                (ContractId::from([1u8; 32]), Color::new([1u8; 32]));
            let balance: Word = 100;

            let balances = Arc::new(Mutex::new(MemoryStore::default()));
            let database = Database {
                balances: balances.clone(),
                ..Default::default()
            };
            balances
                .lock()
                .unwrap()
                .put(MultiKey::new(balance_id), balance.clone())
                .unwrap();

            assert!(
                Storage::<(ContractId, Color), Word>::contains_key(&database, &balance_id).unwrap()
            );
        }
    }

    mod storage {
        use super::*;
        use crate::state::KeyValueStore;

        #[test]
        fn get() {
            let storage_id: (ContractId, Bytes32) =
                (ContractId::from([1u8; 32]), Bytes32::from([1u8; 32]));
            let stored_value: Bytes32 = Bytes32::from([2u8; 32]);

            let storage = Arc::new(Mutex::new(MemoryStore::default()));
            let database = Database {
                storage: storage.clone(),
                ..Default::default()
            };
            storage
                .lock()
                .unwrap()
                .put(MultiKey::new(storage_id), stored_value.clone())
                .unwrap();

            assert_eq!(
                Storage::<(ContractId, Bytes32), Bytes32>::get(&database, &storage_id)
                    .unwrap()
                    .unwrap(),
                stored_value
            );
        }

        #[test]
        fn put() {
            let storage_id: (ContractId, Bytes32) =
                (ContractId::from([1u8; 32]), Bytes32::from([1u8; 32]));
            let stored_value: Bytes32 = Bytes32::from([2u8; 32]);

            let storage = Arc::new(Mutex::new(MemoryStore::default()));
            let mut database = Database {
                storage: storage.clone(),
                ..Default::default()
            };
            Storage::<(ContractId, Bytes32), Bytes32>::insert(
                &mut database,
                storage_id,
                stored_value.clone(),
            )
            .unwrap();

            assert_eq!(
                storage
                    .lock()
                    .unwrap()
                    .get(&MultiKey::new(storage_id))
                    .unwrap()
                    .unwrap(),
                stored_value
            );
        }

        #[test]
        fn remove() {
            let storage_id: (ContractId, Bytes32) =
                (ContractId::from([1u8; 32]), Bytes32::from([1u8; 32]));
            let stored_value: Bytes32 = Bytes32::from([2u8; 32]);

            let storage = Arc::new(Mutex::new(MemoryStore::default()));
            let mut database = Database {
                storage: storage.clone(),
                ..Default::default()
            };
            storage
                .lock()
                .unwrap()
                .put(MultiKey::new(storage_id), stored_value.clone())
                .unwrap();

            Storage::<(ContractId, Bytes32), Bytes32>::remove(&mut database, &storage_id).unwrap();

            assert!(!storage
                .lock()
                .unwrap()
                .exists(&MultiKey::new(storage_id))
                .unwrap());
        }

        #[test]
        fn exists() {
            let storage_id: (ContractId, Bytes32) =
                (ContractId::from([1u8; 32]), Bytes32::from([1u8; 32]));
            let stored_value: Bytes32 = Bytes32::from([2u8; 32]);

            let storage = Arc::new(Mutex::new(MemoryStore::default()));
            let database = Database {
                storage: storage.clone(),
                ..Default::default()
            };
            storage
                .lock()
                .unwrap()
                .put(MultiKey::new(storage_id.clone()), stored_value.clone())
                .unwrap();

            assert!(Storage::<(ContractId, Bytes32), Bytes32>::contains_key(
                &database,
                &storage_id
            )
            .unwrap());
        }
    }
}
