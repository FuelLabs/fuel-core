use crate::database::{columns, Database, KvStoreError};
use fuel_storage::Storage;
use fuel_types::Address;
use std::borrow::Cow;

impl Storage<Address, u64> for Database {
    type Error = KvStoreError;

    fn insert(&mut self, key: &Address, value: &u64) -> Result<Option<u64>, KvStoreError> {
        Database::insert(self, key.as_ref(), columns::VALIDATOR_SET, *value).map_err(Into::into)
    }

    fn remove(&mut self, key: &Address) -> Result<Option<u64>, KvStoreError> {
        Database::remove(self, key.as_ref(), columns::VALIDATOR_SET).map_err(Into::into)
    }

    fn get(&self, key: &Address) -> Result<Option<Cow<u64>>, KvStoreError> {
        Database::get(self, key.as_ref(), columns::VALIDATOR_SET).map_err(Into::into)
    }

    fn contains_key(&self, key: &Address) -> Result<bool, KvStoreError> {
        Database::exists(self, key.as_ref(), columns::VALIDATOR_SET).map_err(Into::into)
    }
}
