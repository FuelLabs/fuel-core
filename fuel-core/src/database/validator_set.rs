use crate::database::{columns, Database, KvStoreError};
use fuel_core_interfaces::{
    common::fuel_storage::Storage,
    model::{ConsensusId, ValidatorId, ValidatorStake},
};
use std::borrow::Cow;

impl Storage<ValidatorId, (ValidatorStake, Option<ConsensusId>)> for Database {
    type Error = KvStoreError;

    fn insert(
        &mut self,
        key: &ValidatorId,
        value: &(ValidatorStake, Option<ConsensusId>),
    ) -> Result<Option<(ValidatorStake, Option<ConsensusId>)>, KvStoreError> {
        Database::insert(self, key.as_ref(), columns::VALIDATOR_SET, *value).map_err(Into::into)
    }

    fn remove(
        &mut self,
        key: &ValidatorId,
    ) -> Result<Option<(ValidatorStake, Option<ConsensusId>)>, KvStoreError> {
        Database::remove(self, key.as_ref(), columns::VALIDATOR_SET).map_err(Into::into)
    }

    fn get(
        &self,
        key: &ValidatorId,
    ) -> Result<Option<Cow<(ValidatorStake, Option<ConsensusId>)>>, KvStoreError> {
        Database::get(self, key.as_ref(), columns::VALIDATOR_SET).map_err(Into::into)
    }

    fn contains_key(&self, key: &ValidatorId) -> Result<bool, KvStoreError> {
        Database::exists(self, key.as_ref(), columns::VALIDATOR_SET).map_err(Into::into)
    }
}
