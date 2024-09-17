use fuel_core_storage::{
    iter::{
        BoxedIter,
        IterDirection,
        IterableStore,
    },
    kv_store::{
        KVItem,
        KeyItem,
        KeyValueInspect,
        StorageColumn,
        Value,
    },
    Result as StorageResult,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct IterableKeyValueViewWrapper<Column>(
    Arc<dyn IterableStore<Column = Column> + Sync + Send>,
);

impl<Column> IterableKeyValueViewWrapper<Column> {
    pub fn into_inner(self) -> Arc<dyn IterableStore<Column = Column> + Sync + Send> {
        self.0
    }
}

impl<Column> std::fmt::Debug for IterableKeyValueViewWrapper<Column> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("IterableKeyValueViewWrapper").finish()
    }
}

impl<Column> IterableKeyValueViewWrapper<Column> {
    pub fn new<S>(storage: S) -> Self
    where
        S: IterableStore<Column = Column> + Send + Sync + 'static,
    {
        Self(Arc::new(storage))
    }
}

impl<Column> KeyValueInspect for IterableKeyValueViewWrapper<Column>
where
    Column: StorageColumn,
{
    type Column = Column;

    fn exists(&self, key: &[u8], column: Self::Column) -> StorageResult<bool> {
        self.0.exists(key, column)
    }

    fn size_of_value(
        &self,
        key: &[u8],
        column: Self::Column,
    ) -> StorageResult<Option<usize>> {
        self.0.size_of_value(key, column)
    }

    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        self.0.get(key, column)
    }

    fn read(
        &self,
        key: &[u8],
        column: Self::Column,
        buf: &mut [u8],
    ) -> StorageResult<Option<usize>> {
        self.0.read(key, column, buf)
    }
}

impl<Column> IterableStore for IterableKeyValueViewWrapper<Column>
where
    Column: StorageColumn,
{
    fn iter_store(
        &self,
        column: Self::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<KVItem> {
        self.0.iter_store(column, prefix, start, direction)
    }

    fn iter_store_keys(
        &self,
        column: Self::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<KeyItem> {
        self.0.iter_store_keys(column, prefix, start, direction)
    }
}
