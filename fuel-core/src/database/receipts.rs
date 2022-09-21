use crate::database::{
    storage::Receipts,
    Column,
    Database,
    KvStoreError,
};
use fuel_core_interfaces::common::{
    fuel_storage::{
        StorageInspect,
        StorageMutate,
    },
    fuel_tx::{
        Bytes32,
        Receipt,
    },
};
use std::borrow::Cow;

impl StorageInspect<Receipts> for Database {
    type Error = KvStoreError;

    fn get(&self, key: &Bytes32) -> Result<Option<Cow<Vec<Receipt>>>, KvStoreError> {
        self._get(key.as_ref(), Column::Receipts)
            .map_err(Into::into)
    }

    fn contains_key(&self, key: &Bytes32) -> Result<bool, KvStoreError> {
        self._contains_key(key.as_ref(), Column::Receipts)
            .map_err(Into::into)
    }
}

impl StorageMutate<Receipts> for Database {
    fn insert(
        &mut self,
        key: &Bytes32,
        value: &[Receipt],
    ) -> Result<Option<Vec<Receipt>>, KvStoreError> {
        self._insert(key.as_ref(), Column::Receipts, value)
            .map_err(Into::into)
    }

    fn remove(&mut self, key: &Bytes32) -> Result<Option<Vec<Receipt>>, KvStoreError> {
        self._remove(key.as_ref(), Column::Receipts)
            .map_err(Into::into)
    }
}
