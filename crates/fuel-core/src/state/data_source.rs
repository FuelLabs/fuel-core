use crate::{
    database::database_description::DatabaseDescription,
    state::TransactableStorage,
};
use fuel_core_storage::{
    iter::{
        BoxedIter,
        IterDirection,
        IterableStore,
    },
    kv_store::{
        KVItem,
        KeyValueInspect,
        Value,
    },
    Result as StorageResult,
};
use std::sync::Arc;

#[allow(type_alias_bounds)]
pub type DataSourceType<Description>
where
    Description: DatabaseDescription,
= Arc<dyn TransactableStorage<Description::Height, Column = Description::Column>>;

#[derive(Debug, Clone)]
pub struct DataSource<Description, Stage>
where
    Description: DatabaseDescription,
{
    pub(crate) data: DataSourceType<Description>,
    pub(crate) stage: Stage,
}

impl<Description, Stage> DataSource<Description, Stage>
where
    Description: DatabaseDescription,
{
    pub fn new(data: DataSourceType<Description>, stage: Stage) -> Self {
        Self { data, stage }
    }
}

impl<Description, Stage> KeyValueInspect for DataSource<Description, Stage>
where
    Description: DatabaseDescription,
{
    type Column = Description::Column;

    fn exists(&self, key: &[u8], column: Self::Column) -> StorageResult<bool> {
        self.data.exists(key, column)
    }

    fn size_of_value(
        &self,
        key: &[u8],
        column: Self::Column,
    ) -> StorageResult<Option<usize>> {
        self.data.size_of_value(key, column)
    }

    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        self.data.get(key, column)
    }

    fn read(
        &self,
        key: &[u8],
        column: Self::Column,
        buf: &mut [u8],
    ) -> StorageResult<Option<usize>> {
        self.data.read(key, column, buf)
    }
}

impl<Description, Stage> IterableStore for DataSource<Description, Stage>
where
    Description: DatabaseDescription,
{
    fn iter_store(
        &self,
        column: Self::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<KVItem> {
        self.data.iter_store(column, prefix, start, direction)
    }

    fn iter_store_keys(
        &self,
        column: Self::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<fuel_core_storage::kv_store::KeyItem> {
        self.data.iter_store_keys(column, prefix, start, direction)
    }
}
