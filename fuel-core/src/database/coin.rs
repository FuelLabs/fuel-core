use crate::database::{columns, Database, KvStore, KvStoreError};
use crate::model::coin::{Coin, CoinId};
use crate::state::{ColumnId, KeyValueStore};
use fuel_tx::Bytes32;

impl KvStore<CoinId, Coin> for Database {
    fn insert(&self, key: &CoinId, value: &Coin) -> Result<Option<Coin>, KvStoreError> {
        self.insert(key.clone(), columns::COIN, value.clone())
            .map_err(Into::into)
    }

    fn remove(&self, key: &CoinId) -> Result<Option<Coin>, KvStoreError> {
        self.remove(&key.0, columns::COIN).map_err(Into::into)
    }

    fn get(&self, key: &CoinId) -> Result<Option<Coin>, KvStoreError> {
        self.get(&key.0, columns::COIN).map_err(Into::into)
    }

    fn contains_key(&self, key: &CoinId) -> Result<bool, KvStoreError> {
        self.exists(&key.0, columns::COIN).map_err(Into::into)
    }
}
