use crate::{
    database::database_description::DatabaseDescription,
    state::{
        generic_database::GenericDatabase,
        iterable_key_value_view::IterableKeyValueViewWrapper,
        key_value_view::KeyValueViewWrapper,
    },
};
use fuel_core_storage::{
    iter::{
        IterDirection,
        IterableStore,
    },
    kv_store::StorageColumn,
    transactional::StorageChanges,
    Result as StorageResult,
};
use std::fmt::Debug;

pub mod data_source;
pub mod generic_database;
#[cfg(feature = "rocksdb")]
pub mod historical_rocksdb;
pub mod in_memory;
pub mod iterable_key_value_view;
pub mod key_value_view;
#[cfg(feature = "rocksdb")]
pub mod rocks_db;
#[cfg(feature = "rocksdb")]
pub mod rocks_db_key_iterator;

pub type ColumnType<Description> = <Description as DatabaseDescription>::Column;
pub type HeightType<Description> = <Description as DatabaseDescription>::Height;

/// A type extends the `KeyValueView`, allowing iteration over the storage.
pub type IterableKeyValueView<Column, BlockHeight> =
    GenericDatabase<IterableKeyValueViewWrapper<Column>, BlockHeight>;

/// The basic view available for the key value storage.
pub type KeyValueView<Column, BlockHeight> =
    GenericDatabase<KeyValueViewWrapper<Column>, BlockHeight>;

impl<Column, Height> IterableKeyValueView<Column, Height>
where
    Column: StorageColumn + 'static,
{
    /// Downgrades the `IterableKeyValueView` into the `KeyValueView`.
    pub fn into_key_value_view(self) -> KeyValueView<Column, Height> {
        let (iterable, metadata) = self.into_inner();
        let storage = KeyValueViewWrapper::new(iterable);
        KeyValueView::from_storage_and_metadata(storage, metadata)
    }
}

pub trait TransactableStorage<Height>: IterableStore + Debug + Send + Sync {
    /// Commits the changes into the storage.
    fn commit_changes(
        &self,
        height: Option<Height>,
        changes: StorageChanges,
    ) -> StorageResult<()>;

    fn view_at_height(
        &self,
        height: &Height,
    ) -> StorageResult<KeyValueView<Self::Column, Height>>;

    fn latest_view(&self) -> StorageResult<IterableKeyValueView<Self::Column, Height>>;

    fn rollback_block_to(&self, height: &Height) -> StorageResult<()>;
}

// It is used only to allow conversion of the `StorageTransaction` into the `DataSource`.
#[cfg(feature = "test-helpers")]
impl<Height, S> TransactableStorage<Height>
    for fuel_core_storage::transactional::StorageTransaction<S>
where
    S: IterableStore + Debug + Send + Sync,
{
    fn commit_changes(&self, _: Option<Height>, _: StorageChanges) -> StorageResult<()> {
        unimplemented!()
    }

    fn view_at_height(
        &self,
        _: &Height,
    ) -> StorageResult<KeyValueView<Self::Column, Height>> {
        unimplemented!()
    }

    fn latest_view(&self) -> StorageResult<IterableKeyValueView<Self::Column, Height>> {
        unimplemented!()
    }
    fn rollback_block_to(&self, _: &Height) -> StorageResult<()> {
        unimplemented!()
    }
}
