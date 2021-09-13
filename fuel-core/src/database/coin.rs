use crate::database::{columns, Database, KvStore, KvStoreError};
use crate::model::coin::Coin;
use crate::state::KeyValueStore;
use fuel_tx::Bytes32;

impl KvStore<Bytes32, Coin> for Database {
    fn insert(&self, key: &Bytes32, value: &Coin) -> Result<Option<Coin>, KvStoreError> {
        Database::insert(&self, key.as_ref().to_vec(), columns::COIN, value.clone())
            .map_err(Into::into)
    }

    fn remove(&self, key: &Bytes32) -> Result<Option<Coin>, KvStoreError> {
        Database::remove(&self, key.as_ref(), columns::COIN).map_err(Into::into)
    }

    fn get(&self, key: &Bytes32) -> Result<Option<Coin>, KvStoreError> {
        Database::get(&self, key.as_ref(), columns::COIN).map_err(Into::into)
    }

    fn contains_key(&self, key: &Bytes32) -> Result<bool, KvStoreError> {
        Database::exists(&self, key.as_ref(), columns::COIN).map_err(Into::into)
    }
}
