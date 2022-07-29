use crate::database::{columns, Database, KvStoreError};
use fuel_core_interfaces::{
    common::{fuel_storage::Storage, fuel_types::Bytes32},
    model::DaMessage,
};
use std::borrow::Cow;

impl Storage<Bytes32, DaMessage> for Database {
    type Error = KvStoreError;

    fn insert(
        &mut self,
        key: &Bytes32,
        value: &DaMessage,
    ) -> Result<Option<DaMessage>, KvStoreError> {
        Database::insert(self, key.as_ref(), columns::DA_MESSAGES, value.clone())
            .map_err(Into::into)
    }

    fn remove(&mut self, key: &Bytes32) -> Result<Option<DaMessage>, KvStoreError> {
        Database::remove(self, key.as_ref(), columns::DA_MESSAGES).map_err(Into::into)
    }

    fn get(&self, key: &Bytes32) -> Result<Option<Cow<DaMessage>>, KvStoreError> {
        Database::get(self, key.as_ref(), columns::DA_MESSAGES).map_err(Into::into)
    }

    fn contains_key(&self, key: &Bytes32) -> Result<bool, KvStoreError> {
        Database::exists(self, key.as_ref(), columns::DA_MESSAGES).map_err(Into::into)
    }
}
