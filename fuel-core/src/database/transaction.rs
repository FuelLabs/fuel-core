use crate::database::{Database, KvStore, KvStoreError};
use fuel_tx::{Bytes32, Transaction};

impl KvStore<Bytes32, Transaction> for Database {
    fn insert(
        &self,
        key: &Bytes32,
        value: &Transaction,
    ) -> Result<Option<Transaction>, KvStoreError> {
        todo!()
    }

    fn remove(&self, key: &Bytes32) -> Result<Option<Transaction>, KvStoreError> {
        todo!()
    }

    fn get(&self, key: &Bytes32) -> Result<Option<Transaction>, KvStoreError> {
        todo!()
    }

    fn contains_key(&self, key: &Bytes32) -> Result<bool, KvStoreError> {
        todo!()
    }
}
