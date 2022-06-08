use crate::database::{columns, Database, KvStoreError};
use fuel_core_interfaces::model::ValidatorStake;
use fuel_storage::Storage;
use fuel_types::Address;
use std::borrow::Cow;

impl Storage<Address, (ValidatorStake, Option<Address>)> for Database {
    type Error = KvStoreError;

    fn insert(
        &mut self,
        key: &Address,
        value: &(ValidatorStake, Option<Address>),
    ) -> Result<Option<(ValidatorStake, Option<Address>)>, KvStoreError> {
        Database::insert(self, key.as_ref(), columns::VALIDATOR_SET, *value).map_err(Into::into)
    }

    fn remove(
        &mut self,
        key: &Address,
    ) -> Result<Option<(ValidatorStake, Option<Address>)>, KvStoreError> {
        Database::remove(self, key.as_ref(), columns::VALIDATOR_SET).map_err(Into::into)
    }

    fn get(
        &self,
        key: &Address,
    ) -> Result<Option<Cow<(ValidatorStake, Option<Address>)>>, KvStoreError> {
        Database::get(self, key.as_ref(), columns::VALIDATOR_SET).map_err(Into::into)
    }

    fn contains_key(&self, key: &Address) -> Result<bool, KvStoreError> {
        Database::exists(self, key.as_ref(), columns::VALIDATOR_SET).map_err(Into::into)
    }
}
