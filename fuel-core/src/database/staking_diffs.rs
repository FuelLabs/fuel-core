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
    db::StakingDiffs,
    model::DaBlockHeight,
    relayer::StakingDiff,
};
use std::borrow::Cow;

impl StorageInspect<StakingDiffs> for Database {
    type Error = KvStoreError;

    fn get(&self, key: &DaBlockHeight) -> Result<Option<Cow<StakingDiff>>, KvStoreError> {
        Database::get(self, &key.to_be_bytes(), Column::StakingDiffs).map_err(Into::into)
    }

    fn contains_key(&self, key: &DaBlockHeight) -> Result<bool, KvStoreError> {
        Database::exists(self, &key.to_be_bytes(), Column::StakingDiffs)
            .map_err(Into::into)
    }
}

impl StorageMutate<StakingDiffs> for Database {
    fn insert(
        &mut self,
        key: &DaBlockHeight,
        value: &StakingDiff,
    ) -> Result<Option<StakingDiff>, KvStoreError> {
        Database::insert(self, key.to_be_bytes(), Column::StakingDiffs, value.clone())
            .map_err(Into::into)
    }

    fn remove(
        &mut self,
        key: &DaBlockHeight,
    ) -> Result<Option<StakingDiff>, KvStoreError> {
        Database::remove(self, &key.to_be_bytes(), Column::StakingDiffs)
            .map_err(Into::into)
    }
}
