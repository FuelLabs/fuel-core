use crate::{database::{columns, Database, KvStoreError}, model::coin::Coin};
use fuel_storage::Storage;
use fuel_types::Bytes32;
use std::borrow::Cow;

impl Storage<Bytes32, Coin> for Database {
    type Error = KvStoreError;

    fn insert(
        &mut self,
        key: &Bytes32,
        value: &Coin,
    ) -> Result<Option<Coin>, KvStoreError> {
        Database::insert(self, key.as_ref(), columns::TOKEN_DEPOSITS, value.clone()).map_err(Into::into)
    }

    fn remove(&mut self, key: &Bytes32) -> Result<Option<Coin>, KvStoreError> {
        Database::remove(self, key.as_ref(), columns::TOKEN_DEPOSITS).map_err(Into::into)
    }

    fn get(&self, key: &Bytes32) -> Result<Option<Cow<Coin>>, KvStoreError> {
        Database::get(self, key.as_ref(), columns::TOKEN_DEPOSITS).map_err(Into::into)
    }

    fn contains_key(&self, key: &Bytes32) -> Result<bool, KvStoreError> {
        Database::exists(self, key.as_ref(), columns::TOKEN_DEPOSITS).map_err(Into::into)
    }
}
