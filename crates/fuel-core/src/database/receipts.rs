use crate::database::{
    Column,
    Database,
    StorageError,
};
use fuel_core_storage::{
    tables::Receipts,
    StorageInspect,
    StorageMutate,
};
use fuel_core_types::{
    fuel_tx::Receipt,
    fuel_types::Bytes32,
};
use std::borrow::Cow;

impl StorageInspect<Receipts> for Database {
    type Error = StorageError;

    fn get(&self, key: &Bytes32) -> Result<Option<Cow<Vec<Receipt>>>, Self::Error> {
        Database::get(self, key.as_ref(), Column::Receipts).map_err(Into::into)
    }

    fn contains_key(&self, key: &Bytes32) -> Result<bool, Self::Error> {
        Database::exists(self, key.as_ref(), Column::Receipts).map_err(Into::into)
    }
}

impl StorageMutate<Receipts> for Database {
    fn insert(
        &mut self,
        key: &Bytes32,
        value: &[Receipt],
    ) -> Result<Option<Vec<Receipt>>, Self::Error> {
        Database::insert(self, key.as_ref(), Column::Receipts, value).map_err(Into::into)
    }

    fn remove(&mut self, key: &Bytes32) -> Result<Option<Vec<Receipt>>, Self::Error> {
        Database::remove(self, key.as_ref(), Column::Receipts).map_err(Into::into)
    }
}
