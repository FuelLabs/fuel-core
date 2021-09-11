use crate::database::{columns, Database, KvStore, KvStoreError};
use crate::model::txo::TransactionOutput;
use fuel_tx::Bytes32;

impl KvStore<Bytes32, TransactionOutput> for Database {
    fn insert(
        &self,
        key: &Bytes32,
        value: &TransactionOutput,
    ) -> Result<Option<TransactionOutput>, KvStoreError> {
        Database::insert(&self, key.as_ref().to_vec(), columns::TXO, value.clone())
            .map_err(Into::into)
    }

    fn remove(&self, key: &Bytes32) -> Result<Option<TransactionOutput>, KvStoreError> {
        Database::remove(&self, key.as_ref(), columns::TXO).map_err(Into::into)
    }

    fn get(&self, key: &Bytes32) -> Result<Option<TransactionOutput>, KvStoreError> {
        Database::get(&self, key.as_ref(), columns::TXO).map_err(Into::into)
    }

    fn contains_key(&self, key: &Bytes32) -> Result<bool, KvStoreError> {
        Database::exists(&self, key.as_ref(), columns::TXO).map_err(Into::into)
    }
}
