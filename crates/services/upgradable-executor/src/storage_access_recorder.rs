use fuel_core_storage::{
    kv_store::{
        KeyValueInspect,
        StorageColumn,
        Value,
    },
    Result as StorageResult,
};
use fuel_core_types::services::executor::StorageReadReplayEvent;
use std::{
    cell::RefCell,
    sync::Arc,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageAccessRecorder<S>
where
    S: KeyValueInspect,
{
    pub storage: S,
    pub record: Arc<RefCell<Vec<StorageReadReplayEvent>>>,
}

impl<S> StorageAccessRecorder<S>
where
    S: KeyValueInspect,
{
    pub fn new(storage: S) -> Self {
        Self {
            storage,
            record: Default::default(),
        }
    }
}

impl<S> KeyValueInspect for StorageAccessRecorder<S>
where
    S: KeyValueInspect,
{
    type Column = S::Column;

    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        let value = self.storage.get(key, column)?;
        self.record.borrow_mut().push(StorageReadReplayEvent {
            column: column.name(),
            key: key.to_vec(),
            value: value.as_ref().map(|v| v.to_vec()),
        });
        Ok(value)
    }
}
