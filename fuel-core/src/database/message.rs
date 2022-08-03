use crate::database::{columns, Database, KvStoreError};
use fuel_core_interfaces::{
    common::{fuel_storage::Storage, fuel_types::MessageId},
    model::DaMessage,
};
use std::borrow::Cow;

impl Storage<MessageId, DaMessage> for Database {
    type Error = KvStoreError;

    fn insert(
        &mut self,
        key: &MessageId,
        value: &DaMessage,
    ) -> Result<Option<DaMessage>, KvStoreError> {
        Database::insert(self, key.as_ref(), columns::DA_MESSAGES, value.clone())
            .map_err(Into::into)
    }

    fn remove(&mut self, key: &MessageId) -> Result<Option<DaMessage>, KvStoreError> {
        Database::remove(self, key.as_ref(), columns::DA_MESSAGES).map_err(Into::into)
    }

    fn get(&self, key: &MessageId) -> Result<Option<Cow<DaMessage>>, KvStoreError> {
        Database::get(self, key.as_ref(), columns::DA_MESSAGES).map_err(Into::into)
    }

    fn contains_key(&self, key: &MessageId) -> Result<bool, KvStoreError> {
        Database::exists(self, key.as_ref(), columns::DA_MESSAGES).map_err(Into::into)
    }
}
