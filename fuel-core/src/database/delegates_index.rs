use crate::database::{
    columns,
    Database,
    KvStoreError,
};
use fuel_core_interfaces::{
    common::{
        fuel_storage::Storage,
        fuel_types::Address,
    },
    model::DaBlockHeight,
};
use std::borrow::Cow;

/// Delegate Index maps delegateAddress with list of da block where delegation happened. so that we
/// can access those changes
impl Storage<Address, Vec<DaBlockHeight>> for Database {
    type Error = KvStoreError;

    fn insert(
        &mut self,
        key: &Address,
        value: &Vec<DaBlockHeight>,
    ) -> Result<Option<Vec<DaBlockHeight>>, KvStoreError> {
        Database::insert(self, key.as_ref(), columns::VALIDATOR_SET, value.clone())
            .map_err(Into::into)
    }

    fn remove(
        &mut self,
        key: &Address,
    ) -> Result<Option<Vec<DaBlockHeight>>, KvStoreError> {
        Database::remove(self, key.as_ref(), columns::VALIDATOR_SET).map_err(Into::into)
    }

    fn get(
        &self,
        key: &Address,
    ) -> Result<Option<Cow<Vec<DaBlockHeight>>>, KvStoreError> {
        Database::get(self, key.as_ref(), columns::VALIDATOR_SET).map_err(Into::into)
    }

    fn contains_key(&self, key: &Address) -> Result<bool, KvStoreError> {
        Database::exists(self, key.as_ref(), columns::VALIDATOR_SET).map_err(Into::into)
    }
}
