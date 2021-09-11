use crate::database::{columns::RECEIPTS, Database, KvStore, KvStoreError};
use fuel_tx::{Bytes32, Receipt};

impl KvStore<Bytes32, Vec<Receipt>> for Database {
    fn insert(
        &self,
        key: &Bytes32,
        value: &Vec<Receipt>,
    ) -> Result<Option<Vec<Receipt>>, KvStoreError> {
        Database::insert(&self, key.as_ref(), RECEIPTS, value.clone()).map_err(Into::into)
    }

    fn remove(&self, key: &Bytes32) -> Result<Option<Vec<Receipt>>, KvStoreError> {
        Database::remove(&self, key.as_ref(), RECEIPTS).map_err(Into::into)
    }

    fn get(&self, key: &Bytes32) -> Result<Option<Vec<Receipt>>, KvStoreError> {
        Database::get(&self, key.as_ref(), RECEIPTS).map_err(Into::into)
    }

    fn contains_key(&self, key: &Bytes32) -> Result<bool, KvStoreError> {
        Database::exists(&self, key.as_ref(), RECEIPTS).map_err(Into::into)
    }
}
