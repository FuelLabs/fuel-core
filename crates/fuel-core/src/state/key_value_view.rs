use fuel_core_storage::{
    kv_store::{
        KeyValueInspect,
        StorageColumn,
        Value,
    },
    Result as StorageResult,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct KeyValueViewWrapper<Column>(
    Arc<dyn KeyValueInspect<Column = Column> + Sync + Send>,
);

impl<Column> std::fmt::Debug for KeyValueViewWrapper<Column> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("KeyValueViewWrapper").finish()
    }
}

impl<Column> KeyValueViewWrapper<Column> {
    pub fn new<S>(storage: S) -> Self
    where
        S: KeyValueInspect<Column = Column> + Send + Sync + 'static,
    {
        Self(Arc::new(storage))
    }
}

impl<Column> KeyValueInspect for KeyValueViewWrapper<Column>
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
