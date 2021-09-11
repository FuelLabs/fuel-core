use crate::database::{columns::BLOCKS, Database, KvStore, KvStoreError};
use crate::model::fuel_block::FuelBlock;
use fuel_tx::Bytes32;

impl KvStore<Bytes32, FuelBlock> for Database {
    fn insert(&self, key: &Bytes32, value: &FuelBlock) -> Result<Option<FuelBlock>, KvStoreError> {
        Database::insert(&self, key.as_ref(), BLOCKS, value.clone()).map_err(Into::into)
    }

    fn remove(&self, key: &Bytes32) -> Result<Option<FuelBlock>, KvStoreError> {
        Database::remove(&self, key.as_ref(), BLOCKS).map_err(Into::into)
    }

    fn get(&self, key: &Bytes32) -> Result<Option<FuelBlock>, KvStoreError> {
        Database::get(&self, key.as_ref(), BLOCKS).map_err(Into::into)
    }

    fn contains_key(&self, key: &Bytes32) -> Result<bool, KvStoreError> {
        Database::exists(&self, key.as_ref(), BLOCKS).map_err(Into::into)
    }
}
