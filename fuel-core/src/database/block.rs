use crate::database::{columns::BLOCKS, Database, KvStore, KvStoreError};
use crate::model::fuel_block::{BlockHeight, FuelBlock};
use crate::state::Error;
use fuel_tx::Bytes32;

const BLOCK_HEIGHT_KEY: &'static [u8] = b"BLOCK_HEIGHT";

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

impl Database {
    pub fn get_block_height(&self) -> Result<Option<BlockHeight>, Error> {
        self.get(BLOCK_HEIGHT_KEY, BLOCKS)
    }

    pub fn update_block_height(&self, height: BlockHeight) -> Result<Option<BlockHeight>, Error> {
        self.insert(BLOCK_HEIGHT_KEY, BLOCKS, height)
    }
}
