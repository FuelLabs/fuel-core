use crate::database::{columns, Database, KvStoreError};
use fuel_core_interfaces::{
    common::fuel_storage::Storage,
    model::{ConsensusPublicKey, ValidatorAddress, ValidatorStake},
};
use std::borrow::Cow;

impl Storage<ValidatorAddress, (ValidatorStake, Option<ConsensusPublicKey>)> for Database {
    type Error = KvStoreError;

    fn insert(
        &mut self,
        key: &ValidatorAddress,
        value: &(ValidatorStake, Option<ConsensusPublicKey>),
    ) -> Result<Option<(ValidatorStake, Option<ConsensusPublicKey>)>, KvStoreError> {
        Database::insert(self, key.as_ref(), columns::VALIDATOR_SET, *value).map_err(Into::into)
    }

    fn remove(
        &mut self,
        key: &ValidatorAddress,
    ) -> Result<Option<(ValidatorStake, Option<ConsensusPublicKey>)>, KvStoreError> {
        Database::remove(self, key.as_ref(), columns::VALIDATOR_SET).map_err(Into::into)
    }

    fn get(
        &self,
        key: &ValidatorAddress,
    ) -> Result<Option<Cow<(ValidatorStake, Option<ConsensusPublicKey>)>>, KvStoreError> {
        Database::get(self, key.as_ref(), columns::VALIDATOR_SET).map_err(Into::into)
    }

    fn contains_key(&self, key: &ValidatorAddress) -> Result<bool, KvStoreError> {
        Database::exists(self, key.as_ref(), columns::VALIDATOR_SET).map_err(Into::into)
    }
}
