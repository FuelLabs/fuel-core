use crate::database::{
    Column,
    Database,
    KvStoreError,
};
use fuel_core_interfaces::{
    common::fuel_storage::{
        StorageInspect,
        StorageMutate,
    },
    db::ValidatorsSet,
    model::{
        ConsensusId,
        ValidatorId,
        ValidatorStake,
    },
};
use std::borrow::Cow;

impl StorageInspect<ValidatorsSet> for Database {
    type Error = KvStoreError;

    fn get(
        &self,
        key: &ValidatorId,
    ) -> Result<Option<Cow<(ValidatorStake, Option<ConsensusId>)>>, KvStoreError> {
        Database::get(self, key.as_ref(), Column::ValidatorsSet).map_err(Into::into)
    }

    fn contains_key(&self, key: &ValidatorId) -> Result<bool, KvStoreError> {
        Database::exists(self, key.as_ref(), Column::ValidatorsSet).map_err(Into::into)
    }
}

impl StorageMutate<ValidatorsSet> for Database {
    fn insert(
        &mut self,
        key: &ValidatorId,
        value: &(ValidatorStake, Option<ConsensusId>),
    ) -> Result<Option<(ValidatorStake, Option<ConsensusId>)>, KvStoreError> {
        Database::insert(self, key.as_ref(), Column::ValidatorsSet, value)
            .map_err(Into::into)
    }

    fn remove(
        &mut self,
        key: &ValidatorId,
    ) -> Result<Option<(ValidatorStake, Option<ConsensusId>)>, KvStoreError> {
        Database::remove(self, key.as_ref(), Column::ValidatorsSet).map_err(Into::into)
    }
}
