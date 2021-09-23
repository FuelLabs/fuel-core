use crate::database::{columns::TRANSACTIONS, Database, KvStore, KvStoreError};
use crate::state::{Error, IterDirection};
use fuel_tx::{Bytes32, Transaction};

impl KvStore<Bytes32, Transaction> for Database {
    fn insert(
        &self,
        key: &Bytes32,
        value: &Transaction,
    ) -> Result<Option<Transaction>, KvStoreError> {
        Database::insert(&self, key.as_ref(), TRANSACTIONS, value.clone()).map_err(Into::into)
    }

    fn remove(&self, key: &Bytes32) -> Result<Option<Transaction>, KvStoreError> {
        Database::remove(&self, key.as_ref(), TRANSACTIONS).map_err(Into::into)
    }

    fn get(&self, key: &Bytes32) -> Result<Option<Transaction>, KvStoreError> {
        Database::get(&self, key.as_ref(), TRANSACTIONS).map_err(Into::into)
    }

    fn contains_key(&self, key: &Bytes32) -> Result<bool, KvStoreError> {
        Database::exists(&self, key.as_ref(), TRANSACTIONS).map_err(Into::into)
    }
}

impl Database {
    pub fn all_transactions(
        &self,
        start: Option<&Bytes32>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = Result<Transaction, Error>> + '_ {
        let start = start.map(|b| b.as_ref());
        self.iter_all::<Vec<u8>, Transaction>(TRANSACTIONS, None, start, direction)
            .map(|res| res.map(|(_, tx)| tx))
    }
}
