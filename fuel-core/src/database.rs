use crate::database::columns::{BALANCES, CONTRACTS, STATE};
use crate::state::in_memory::memory_store::MemoryStore;
use crate::state::in_memory::transaction::MemoryTransactionView;
use crate::state::{ColumnId, DataSource, Error, MultiKey};
use fuel_vm::data::{DataError, InterpreterStorage};
use fuel_vm::prelude::{Bytes32, Color, Contract, ContractId, Storage, Word};
use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Debug;
use std::sync::Arc;

pub(crate) mod columns {
    pub const CONTRACTS: u32 = 1;
    pub const BALANCES: u32 = 2;
    pub const STATE: u32 = 3;
}

pub trait DatabaseTrait: InterpreterStorage + Debug {
    fn transaction(&self) -> DatabaseTransaction;
}

#[derive(Clone, Debug)]
pub struct Database {
    data: DataSource,
}

impl Database {
    fn insert<K: Into<Vec<u8>>, V: Serialize + DeserializeOwned>(
        &mut self,
        key: K,
        column: ColumnId,
        value: V,
    ) -> Result<Option<V>, Error> {
        let result = self.data.put(
            key.into(),
            column,
            bincode::serialize(&value).map_err(|_| Error::Codec)?,
        )?;
        if let Some(previous) = result {
            Ok(Some(
                bincode::deserialize(&previous).map_err(|_| Error::Codec)?,
            ))
        } else {
            Ok(None)
        }
    }

    fn remove<V: DeserializeOwned>(
        &mut self,
        key: &[u8],
        column: ColumnId,
    ) -> Result<Option<V>, Error> {
        self.data
            .delete(key, column)?
            .map(|val| bincode::deserialize(&val).map_err(|_| Error::Codec))
            .transpose()
    }

    fn get<V: DeserializeOwned>(&self, key: &[u8], column: ColumnId) -> Result<Option<V>, Error> {
        self.data
            .get(key, column)?
            .map(|val| bincode::deserialize(&val).map_err(|_| Error::Codec))
            .transpose()
    }

    fn exists(&self, key: &[u8], column: ColumnId) -> Result<bool, Error> {
        self.data.exists(key, column)
    }
}

impl DatabaseTrait for Database {
    fn transaction(&self) -> DatabaseTransaction {
        self.into()
    }
}

impl Default for Database {
    fn default() -> Self {
        Self {
            data: Arc::new(MemoryStore::default()),
        }
    }
}

impl Storage<ContractId, Contract> for Database {
    fn insert(&mut self, key: ContractId, value: Contract) -> Result<Option<Contract>, DataError> {
        self.insert(key.as_ref(), CONTRACTS, value)
            .map_err(Into::into)
    }

    fn remove(&mut self, key: &ContractId) -> Result<Option<Contract>, DataError> {
        self.remove(key.as_ref(), CONTRACTS).map_err(Into::into)
    }

    fn get(&self, key: &ContractId) -> Result<Option<Contract>, DataError> {
        self.get(key.as_ref(), CONTRACTS).map_err(Into::into)
    }

    fn contains_key(&self, key: &ContractId) -> Result<bool, DataError> {
        self.exists(key.as_ref(), CONTRACTS).map_err(Into::into)
    }
}

impl Storage<(ContractId, Color), Word> for Database {
    fn insert(&mut self, key: (ContractId, Color), value: u64) -> Result<Option<u64>, DataError> {
        let key = MultiKey::new(key);
        self.insert(key.as_ref().to_vec(), BALANCES, value)
            .map_err(Into::into)
    }

    fn remove(&mut self, key: &(ContractId, Color)) -> Result<Option<u64>, DataError> {
        let key = MultiKey::new(*key);
        self.remove(key.as_ref(), BALANCES).map_err(Into::into)
    }

    fn get(&self, key: &(ContractId, Color)) -> Result<Option<u64>, DataError> {
        let key = MultiKey::new(*key);
        self.get(key.as_ref(), BALANCES).map_err(Into::into)
    }

    fn contains_key(&self, key: &(ContractId, Color)) -> Result<bool, DataError> {
        let key = MultiKey::new(*key);
        self.exists(key.as_ref(), BALANCES).map_err(Into::into)
    }
}

impl Storage<(ContractId, Bytes32), Bytes32> for Database {
    fn insert(
        &mut self,
        key: (ContractId, Bytes32),
        value: Bytes32,
    ) -> Result<Option<Bytes32>, DataError> {
        let key = MultiKey::new(key);
        self.insert(key.as_ref().to_vec(), STATE, value)
            .map_err(Into::into)
    }

    fn remove(&mut self, key: &(ContractId, Bytes32)) -> Result<Option<Bytes32>, DataError> {
        let key = MultiKey::new(*key);
        self.remove(key.as_ref(), STATE).map_err(Into::into)
    }

    fn get(&self, key: &(ContractId, Bytes32)) -> Result<Option<Bytes32>, DataError> {
        let key = MultiKey::new(*key);
        self.get(key.as_ref(), STATE).map_err(Into::into)
    }

    fn contains_key(&self, key: &(ContractId, Bytes32)) -> Result<bool, DataError> {
        let key = MultiKey::new(*key);
        self.exists(key.as_ref(), STATE).map_err(Into::into)
    }
}

impl InterpreterStorage for Database {}

#[derive(Clone, Debug)]
pub struct DatabaseTransaction {
    // The primary datastores
    changes: Arc<MemoryTransactionView>,
    // The inner db impl using these stores
    database: Database,
}

impl Default for DatabaseTransaction {
    fn default() -> Self {
        Database::default().transaction()
    }
}

impl DatabaseTransaction {
    pub fn commit(self) -> crate::state::Result<()> {
        // TODO: should commit be fallible if this api is meant to be atomic?
        self.changes.commit()
    }
}

impl Storage<ContractId, Contract> for DatabaseTransaction {
    fn insert(&mut self, key: ContractId, value: Contract) -> Result<Option<Contract>, DataError> {
        Storage::<ContractId, Contract>::insert(&mut self.database, key, value)
    }

    fn remove(&mut self, key: &ContractId) -> Result<Option<Contract>, DataError> {
        Storage::<ContractId, Contract>::remove(&mut self.database, key)
    }

    fn get(&self, key: &ContractId) -> Result<Option<Contract>, DataError> {
        Storage::<ContractId, Contract>::get(&self.database, key)
    }

    fn contains_key(&self, key: &ContractId) -> Result<bool, DataError> {
        Storage::<ContractId, Contract>::contains_key(&self.database, key)
    }
}

impl Storage<(ContractId, Color), Word> for DatabaseTransaction {
    fn insert(&mut self, key: (ContractId, Color), value: Word) -> Result<Option<Word>, DataError> {
        Storage::<(ContractId, Color), Word>::insert(&mut self.database, key, value)
    }

    fn remove(&mut self, key: &(ContractId, Color)) -> Result<Option<Word>, DataError> {
        Storage::<(ContractId, Color), Word>::remove(&mut self.database, key)
    }

    fn get(&self, key: &(ContractId, Color)) -> Result<Option<Word>, DataError> {
        Storage::<(ContractId, Color), Word>::get(&self.database, key)
    }

    fn contains_key(&self, key: &(ContractId, Color)) -> Result<bool, DataError> {
        Storage::<(ContractId, Color), Word>::contains_key(&self.database, key)
    }
}

impl Storage<(ContractId, Bytes32), Bytes32> for DatabaseTransaction {
    fn insert(
        &mut self,
        key: (ContractId, Bytes32),
        value: Bytes32,
    ) -> Result<Option<Bytes32>, DataError> {
        Storage::<(ContractId, Bytes32), Bytes32>::insert(&mut self.database, key, value)
    }

    fn remove(&mut self, key: &(ContractId, Bytes32)) -> Result<Option<Bytes32>, DataError> {
        Storage::<(ContractId, Bytes32), Bytes32>::remove(&mut self.database, key)
    }

    fn get(&self, key: &(ContractId, Bytes32)) -> Result<Option<Bytes32>, DataError> {
        Storage::<(ContractId, Bytes32), Bytes32>::get(&self.database, key)
    }

    fn contains_key(&self, key: &(ContractId, Bytes32)) -> Result<bool, DataError> {
        Storage::<(ContractId, Bytes32), Bytes32>::contains_key(&self.database, key)
    }
}

impl From<&Database> for DatabaseTransaction {
    fn from(source: &Database) -> Self {
        let data = Arc::new(MemoryTransactionView::new(source.data.clone()));
        Self {
            changes: data.clone(),
            database: Database { data },
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
        todo!("DataError is a ZeroVariant enum and cannot be instantiated yet")
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
                .put(contract_id, CONTRACTS, contract.clone())
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
                    .get(&contract_id, CONTRACTS)
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
                .put(contract_id, CONTRACTS, contract.clone())
                .unwrap();

            Storage::<ContractId, Contract>::remove(&mut database, &contract_id).unwrap();

            assert!(!contracts
                .lock()
                .unwrap()
                .exists(&contract_id, CONTRACTS)
                .unwrap());
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
                .put(contract_id, CONTRACTS, contract.clone())
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
                .put(MultiKey::new(balance_id), BALANCES, balance.clone())
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
                    .get(&MultiKey::new(balance_id), BALANCES)
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
                .put(MultiKey::new(balance_id), BALANCES, balance.clone())
                .unwrap();

            Storage::<(ContractId, Color), Word>::remove(&mut database, &balance_id).unwrap();

            assert!(!balances
                .lock()
                .unwrap()
                .exists(&MultiKey::new(balance_id), BALANCES)
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
                .put(MultiKey::new(balance_id), BALANCES, balance.clone())
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
                .put(MultiKey::new(storage_id), STATE, stored_value.clone())
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
                    .get(&MultiKey::new(storage_id), STATE)
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
                .put(MultiKey::new(storage_id), STATE, stored_value.clone())
                .unwrap();

            Storage::<(ContractId, Bytes32), Bytes32>::remove(&mut database, &storage_id).unwrap();

            assert!(!storage
                .lock()
                .unwrap()
                .exists(&MultiKey::new(storage_id), STATE)
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
                .put(
                    MultiKey::new(storage_id.clone()),
                    STATE,
                    stored_value.clone(),
                )
                .unwrap();

            assert!(Storage::<(ContractId, Bytes32), Bytes32>::contains_key(
                &database,
                &storage_id
            )
            .unwrap());
        }
    }
}
