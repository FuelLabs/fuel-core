use crate::database::{columns, Database, KvStoreError};
use fuel_storage::Storage;
use fuel_types::Address;
use std::{borrow::Cow, collections::HashMap};

impl Storage<u64, HashMap<Address, u64>> for Database {
    type Error = KvStoreError;

    fn insert(
        &mut self,
        key: &u64,
        value: &HashMap<Address, u64>,
    ) -> Result<Option<HashMap<Address, u64>>, KvStoreError> {
        Database::insert(
            self,
            key.to_be_bytes(),
            columns::VALIDATOR_SET_DIFFS,
            value.clone(),
        )
        .map_err(Into::into)
    }

    fn remove(&mut self, key: &u64) -> Result<Option<HashMap<Address, u64>>, KvStoreError> {
        Database::remove(self, &key.to_be_bytes(), columns::VALIDATOR_SET_DIFFS).map_err(Into::into)
    }

    fn get(&self, key: &u64) -> Result<Option<Cow<HashMap<Address, u64>>>, KvStoreError> {
        Database::get(self, &key.to_be_bytes(), columns::VALIDATOR_SET_DIFFS).map_err(Into::into)
    }

    fn contains_key(&self, key: &u64) -> Result<bool, KvStoreError> {
        Database::exists(self, &key.to_be_bytes(), columns::VALIDATOR_SET_DIFFS).map_err(Into::into)
    }
}
