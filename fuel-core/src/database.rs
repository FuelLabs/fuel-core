#[cfg(feature = "default")]
use crate::database::columns::COLUMN_NUM;
use crate::database::transactional::DatabaseTransaction;
use crate::model::coin::{Coin, CoinId};
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
use thiserror::Error;

pub mod balances;
pub mod code_root;
pub mod coin;
pub mod contracts;
pub mod state;
pub mod transaction;
pub mod transactional;

// Crude way to invalidate incompatible databases,
// can be used to perform migrations in the future.
pub const VERSION: u32 = 0;

pub(crate) mod columns {
    pub const DB_VERSION_COLUMN: u32 = 0;
    pub const CONTRACTS: u32 = 1;
    pub const CONTRACTS_CODE_ROOT: u32 = 2;
    pub const CONTRACTS_STATE: u32 = 3;
    pub const BALANCES: u32 = 4;
    pub const COIN: u32 = 5;

    // Number of columns
    #[cfg(feature = "default")]
    pub const COLUMN_NUM: u32 = 6;
}

pub trait DatabaseTrait: InterpreterStorage + AsRef<Database> + Debug + Send + Sync {
    fn transaction(&self) -> DatabaseTransaction;
}

#[derive(Clone, Debug)]
pub struct SharedDatabase(pub Arc<dyn DatabaseTrait>);

impl Default for SharedDatabase {
    fn default() -> Self {
        SharedDatabase(Arc::new(Database::default()))
    }
}

#[derive(Clone, Debug)]
pub struct Database {
    data: DataSource,
}

impl Database {
    #[cfg(feature = "default")]
    pub fn open(path: &Path) -> Result<Self, Error> {
        let db = RocksDb::open(path, COLUMN_NUM)?;

        Ok(Database { data: Arc::new(db) })
    }

    fn insert<K: Into<Vec<u8>>, V: Serialize + DeserializeOwned>(
        &self,
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
        &self,
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

    fn iter_all<K, V>(&self, column: ColumnId) -> impl Iterator<Item = Result<(K, V), Error>> + '_
    where
        K: From<Vec<u8>>,
        V: DeserializeOwned,
    {
        self.data.iter_all(column).map(|(key, value)| {
            let key = K::from(key);
            let value: V = bincode::deserialize(&value).map_err(|_| Error::Codec)?;
            Ok((key, value))
        })
    }
}

impl AsRef<Database> for Database {
    fn as_ref(&self) -> &Database {
        &self
    }
}

impl DatabaseTrait for Database {
    fn transaction(&self) -> DatabaseTransaction {
        self.into()
    }
}

/// Construct an in-memory database
impl Default for Database {
    fn default() -> Self {
        Self {
            data: Arc::new(MemoryStore::default()),
        }
    }
}

impl InterpreterStorage for Database {
    fn block_height(&self) -> Result<u32, DataError> {
        Ok(Default::default())
    }

    fn block_hash(&self, _block_height: u32) -> Result<Bytes32, DataError> {
        Ok(Default::default())
    }

    fn coinbase(&self) -> Result<Address, DataError> {
        Ok(Default::default())
    }
}

pub trait KvStore<K, V> {
    fn insert(&self, key: &K, value: &V) -> Result<Option<V>, KvStoreError>;
    fn remove(&self, key: &K) -> Result<Option<V>, KvStoreError>;
    fn get(&self, key: &K) -> Result<Option<V>, KvStoreError>;
    fn contains_key(&self, key: &K) -> Result<bool, KvStoreError>;
}

#[derive(Debug, Error)]
pub enum KvStoreError {
    #[error("generic error occurred")]
    Error(Box<dyn std::error::Error + Send>),
}

impl From<bincode::Error> for KvStoreError {
    fn from(e: bincode::Error) -> Self {
        KvStoreError::Error(Box::new(e))
    }
}

impl From<crate::state::Error> for KvStoreError {
    fn from(e: Error) -> Self {
        KvStoreError::Error(Box::new(e))
    }
}

impl From<crate::state::Error> for DataError {
    fn from(e: Error) -> Self {
        panic!("No valid DataError variants to construct {:?}", e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::columns::{BALANCES, CONTRACTS, CONTRACTS_CODE_ROOT, CONTRACTS_STATE};

    mod contracts {
        use super::*;

        #[test]
        fn get() {
            let contract_id: ContractId = ContractId::from([1u8; 32]);
            let contract: Contract = Contract::from(vec![32u8]);

            let database = Database::default();

            database
                .insert(contract_id.as_ref().to_vec(), CONTRACTS, contract.clone())
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

            let mut database = Database::default();
            Storage::<ContractId, Contract>::insert(&mut database, &contract_id, &contract.clone())
                .unwrap();

            let returned: Contract = database
                .get(contract_id.as_ref(), CONTRACTS)
                .unwrap()
                .unwrap();
            assert_eq!(returned, contract);
        }

        #[test]
        fn remove() {
            let contract_id: ContractId = ContractId::from([1u8; 32]);
            let contract: Contract = Contract::from(vec![32u8]);

            let mut database = Database::default();
            database
                .insert(contract_id.as_ref().to_vec(), CONTRACTS, contract.clone())
                .unwrap();

            Storage::<ContractId, Contract>::remove(&mut database, &contract_id).unwrap();

            assert!(!database.exists(contract_id.as_ref(), CONTRACTS).unwrap());
        }

        #[test]
        fn exists() {
            let contract_id: ContractId = ContractId::from([1u8; 32]);
            let contract: Contract = Contract::from(vec![32u8]);

            let database = Database::default();
            database
                .insert(contract_id.as_ref().to_vec(), CONTRACTS, contract.clone())
                .unwrap();

            assert!(
                Storage::<ContractId, Contract>::contains_key(&database, &contract_id).unwrap()
            );
        }
    }

    mod contract_code_root {
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
            Storage::<ContractId, (Salt, Bytes32)>::insert(
                &mut database,
                &contract_id,
                &(salt, root),
            )
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
                Storage::<ContractId, (Salt, Bytes32)>::contains_key(&database, &contract_id)
                    .unwrap()
            );
        }
    }

    mod balances {
        use super::*;

        #[test]
        fn get() {
            let balance_id: (ContractId, Color) =
                (ContractId::from([1u8; 32]), Color::new([1u8; 32]));
            let balance: Word = 100;

            let database = Database::default();
            let key: Vec<u8> = MultiKey::new(balance_id).into();
            let _: Option<Word> = database.insert(key, BALANCES, balance.clone()).unwrap();

            assert_eq!(
                MerkleStorage::<ContractId, Color, Word>::get(
                    &database,
                    &balance_id.0,
                    &balance_id.1
                )
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

            let mut database = Database::default();
            MerkleStorage::<ContractId, Color, Word>::insert(
                &mut database,
                &balance_id.0,
                &balance_id.1,
                &balance,
            )
            .unwrap();

            let returned: Word = database
                .get(MultiKey::new(balance_id).as_ref(), BALANCES)
                .unwrap()
                .unwrap();
            assert_eq!(returned, balance);
        }

        #[test]
        fn remove() {
            let balance_id: (ContractId, Color) =
                (ContractId::from([1u8; 32]), Color::new([1u8; 32]));
            let balance: Word = 100;

            let mut database = Database::default();
            database
                .insert(MultiKey::new(balance_id), BALANCES, balance.clone())
                .unwrap();

            MerkleStorage::<ContractId, Color, Word>::remove(
                &mut database,
                &balance_id.0,
                &balance_id.1,
            )
            .unwrap();

            assert!(!database
                .exists(MultiKey::new(balance_id).as_ref(), BALANCES)
                .unwrap());
        }

        #[test]
        fn exists() {
            let balance_id: (ContractId, Color) =
                (ContractId::from([1u8; 32]), Color::new([1u8; 32]));
            let balance: Word = 100;

            let database = Database::default();
            database
                .insert(
                    MultiKey::new(balance_id).as_ref().to_vec(),
                    BALANCES,
                    balance.clone(),
                )
                .unwrap();

            assert!(MerkleStorage::<ContractId, Color, Word>::contains_key(
                &database,
                &balance_id.0,
                &balance_id.1
            )
            .unwrap());
        }

        #[test]
        fn root() {
            let balance_id: (ContractId, Color) =
                (ContractId::from([1u8; 32]), Color::new([1u8; 32]));
            let balance: Word = 100;

            let mut database = Database::default();

            MerkleStorage::<ContractId, Color, Word>::insert(
                &mut database,
                &balance_id.0,
                &balance_id.1,
                &balance,
            )
            .unwrap();

            let root = MerkleStorage::<ContractId, Color, Word>::root(&mut database, &balance_id.0);
            assert!(root.is_ok())
        }
    }

    mod storage {
        use super::*;

        #[test]
        fn get() {
            let storage_id: (ContractId, Bytes32) =
                (ContractId::from([1u8; 32]), Bytes32::from([1u8; 32]));
            let stored_value: Bytes32 = Bytes32::from([2u8; 32]);

            let database = Database::default();
            database
                .insert(
                    MultiKey::new(storage_id),
                    CONTRACTS_STATE,
                    stored_value.clone(),
                )
                .unwrap();

            assert_eq!(
                MerkleStorage::<ContractId, Bytes32, Bytes32>::get(
                    &database,
                    &storage_id.0,
                    &storage_id.1
                )
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

            let mut database = Database::default();
            MerkleStorage::<ContractId, Bytes32, Bytes32>::insert(
                &mut database,
                &storage_id.0,
                &storage_id.1,
                &stored_value,
            )
            .unwrap();

            let returned: Bytes32 = database
                .get(MultiKey::new(storage_id).as_ref(), CONTRACTS_STATE)
                .unwrap()
                .unwrap();
            assert_eq!(returned, stored_value);
        }

        #[test]
        fn remove() {
            let storage_id: (ContractId, Bytes32) =
                (ContractId::from([1u8; 32]), Bytes32::from([1u8; 32]));
            let stored_value: Bytes32 = Bytes32::from([2u8; 32]);

            let mut database = Database::default();
            database
                .insert(
                    MultiKey::new(storage_id),
                    CONTRACTS_STATE,
                    stored_value.clone(),
                )
                .unwrap();

            MerkleStorage::<ContractId, Bytes32, Bytes32>::remove(
                &mut database,
                &storage_id.0,
                &storage_id.1,
            )
            .unwrap();

            assert!(!database
                .exists(MultiKey::new(storage_id).as_ref(), CONTRACTS_STATE)
                .unwrap());
        }

        #[test]
        fn exists() {
            let storage_id: (ContractId, Bytes32) =
                (ContractId::from([1u8; 32]), Bytes32::from([1u8; 32]));
            let stored_value: Bytes32 = Bytes32::from([2u8; 32]);

            let database = Database::default();
            database
                .insert(
                    MultiKey::new(storage_id.clone()),
                    CONTRACTS_STATE,
                    stored_value.clone(),
                )
                .unwrap();

            assert!(MerkleStorage::<ContractId, Bytes32, Bytes32>::contains_key(
                &database,
                &storage_id.0,
                &storage_id.1
            )
            .unwrap());
        }

        #[test]
        fn root() {
            let storage_id: (ContractId, Bytes32) =
                (ContractId::from([1u8; 32]), Bytes32::from([1u8; 32]));
            let stored_value: Bytes32 = Bytes32::from([2u8; 32]);

            let mut database = Database::default();

            MerkleStorage::<ContractId, Bytes32, Bytes32>::insert(
                &mut database,
                &storage_id.0,
                &storage_id.1,
                &stored_value,
            )
            .unwrap();

            let root =
                MerkleStorage::<ContractId, Bytes32, Bytes32>::root(&mut database, &storage_id.0);
            assert!(root.is_ok())
        }
    }
}
