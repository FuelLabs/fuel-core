use fuel_core_storage::{
    Result as StorageResult,
    kv_store::{
        KeyValueInspect,
        StorageColumn,
        Value,
    },
};
use fuel_core_types::services::executor::StorageReadReplayEvent;
use parking_lot::Mutex;

#[cfg(feature = "std")]
use std::sync::Arc;

#[cfg(not(feature = "std"))]
use alloc::{
    sync::Arc,
    vec::Vec,
};

pub struct StorageAccessRecorder<S>
where
    S: KeyValueInspect,
{
    pub storage: S,
    pub record: Arc<Mutex<Vec<StorageReadReplayEvent>>>,
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
        self.record.lock().push(StorageReadReplayEvent {
            column: column.id(),
            key: key.to_vec(),
            value: value.as_ref().map(|v| v.to_vec()),
        });
        Ok(value)
    }
}
