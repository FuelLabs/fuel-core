use crate::database::{
    Column,
    Database,
    KvStoreError,
};
use fuel_core_interfaces::{
    common::{
        fuel_storage::{
            StorageInspect,
            StorageMutate,
        },
        fuel_types::Address,
    },
    db::DelegatesIndexes,
    model::DaBlockHeight,
};
use std::borrow::Cow;

impl StorageInspect<DelegatesIndexes> for Database {
    type Error = KvStoreError;

    fn get(
        &self,
        key: &Address,
    ) -> Result<Option<Cow<Vec<DaBlockHeight>>>, KvStoreError> {
        self._get(key.as_ref(), Column::DelegatesIndexes)
            .map_err(Into::into)
    }

    fn contains_key(&self, key: &Address) -> Result<bool, KvStoreError> {
        self._contains_key(key.as_ref(), Column::DelegatesIndexes)
            .map_err(Into::into)
    }
}

impl StorageMutate<DelegatesIndexes> for Database {
    fn insert(
        &mut self,
        key: &Address,
        value: &[DaBlockHeight],
    ) -> Result<Option<Vec<DaBlockHeight>>, KvStoreError> {
        self._insert(key.as_ref(), Column::DelegatesIndexes, value)
            .map_err(Into::into)
    }

    fn remove(
        &mut self,
        key: &Address,
    ) -> Result<Option<Vec<DaBlockHeight>>, KvStoreError> {
        self._remove(key.as_ref(), Column::DelegatesIndexes)
            .map_err(Into::into)
    }
}
