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
        self._get(&key.to_be_bytes(), Column::StakingDiffs)
            .map_err(Into::into)
    }

    fn contains_key(&self, key: &DaBlockHeight) -> Result<bool, KvStoreError> {
        self._contains_key(&key.to_be_bytes(), Column::StakingDiffs)
            .map_err(Into::into)
    }
}

impl StorageMutate<StakingDiffs> for Database {
    fn insert(
        &mut self,
        key: &DaBlockHeight,
        value: &StakingDiff,
    ) -> Result<Option<StakingDiff>, KvStoreError> {
        self._insert(&key.to_be_bytes(), Column::StakingDiffs, value)
            .map_err(Into::into)
    }

    fn remove(
        &mut self,
        key: &DaBlockHeight,
    ) -> Result<Option<StakingDiff>, KvStoreError> {
        self._remove(&key.to_be_bytes(), Column::StakingDiffs)
            .map_err(Into::into)
    }
}
