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
        fuel_tx::{
            Bytes32,
            Receipt,
        },
    },
    db::Receipts,
};
use std::borrow::Cow;

impl StorageInspect<Receipts> for Database {
    type Error = KvStoreError;

    fn get(&self, key: &Bytes32) -> Result<Option<Cow<Vec<Receipt>>>, KvStoreError> {
        Database::get(self, key.as_ref(), Column::Receipts).map_err(Into::into)
    }

    fn contains_key(&self, key: &Bytes32) -> Result<bool, KvStoreError> {
        Database::exists(self, key.as_ref(), Column::Receipts).map_err(Into::into)
    }
}

impl StorageMutate<Receipts> for Database {
    fn insert(
        &mut self,
        key: &Bytes32,
        value: &[Receipt],
    ) -> Result<Option<Vec<Receipt>>, KvStoreError> {
        Database::insert(self, key.as_ref(), Column::Receipts, value).map_err(Into::into)
    }

    fn remove(&mut self, key: &Bytes32) -> Result<Option<Vec<Receipt>>, KvStoreError> {
        Database::remove(self, key.as_ref(), Column::Receipts).map_err(Into::into)
    }
}
