use crate::database::{Database, KvStore, KvStoreError};
use crate::model::fuel_block::FuelBlock;
use fuel_tx::{Bytes32, Transaction};

impl KvStore<Bytes32, FuelBlock> for Database {
    fn insert(&self, key: &Bytes32, value: &FuelBlock) -> Result<Option<FuelBlock>, KvStoreError> {
        todo!()
    }

    fn remove(&self, key: &Bytes32) -> Result<Option<FuelBlock>, KvStoreError> {
        todo!()
    }

    fn get(&self, key: &Bytes32) -> Result<Option<FuelBlock>, KvStoreError> {
        todo!()
    }

    fn contains_key(&self, key: &Bytes32) -> Result<bool, KvStoreError> {
        todo!()
    }
}
