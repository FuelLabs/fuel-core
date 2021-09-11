use crate::database::{Database, KvStore, KvStoreError};
use fuel_tx::{Bytes32, Receipt};

impl KvStore<Bytes32, Vec<Receipt>> for Database {
    fn insert(
        &self,
        key: &Bytes32,
        value: &Vec<Receipt>,
    ) -> Result<Option<Vec<Receipt>>, KvStoreError> {
        todo!()
    }

    fn remove(&self, key: &Bytes32) -> Result<Option<Vec<Receipt>>, KvStoreError> {
        todo!()
    }

    fn get(&self, key: &Bytes32) -> Result<Option<Vec<Receipt>>, KvStoreError> {
        todo!()
    }

    fn contains_key(&self, key: &Bytes32) -> Result<bool, KvStoreError> {
        todo!()
    }
}
