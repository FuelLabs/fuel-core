use fuel_core_storage::{
    ContractsAssetKey,
    ContractsStateKey,
    Result as StorageResult,
    column::Column,
    kv_store::{
        KeyValueInspect,
        StorageColumn,
        Value,
    },
    transactional::Changes,
};
use fuel_core_types::fuel_tx::{
    AssetId,
    Bytes32,
    ContractId,
};
use parking_lot::Mutex;

#[cfg(feature = "std")]
use std::{
    collections::{
        BTreeMap,
        BTreeSet,
    },
    sync::Arc,
};

#[cfg(not(feature = "std"))]
use alloc::{
    collections::{
        BTreeMap,
        BTreeSet,
    },
    sync::Arc,
};

#[derive(Debug, Clone, Default)]
pub(crate) struct ContractAccesses {
    pub assets: BTreeSet<AssetId>,
    pub slots: BTreeSet<Bytes32>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ReadsPerContract {
    pub per_contract: BTreeMap<ContractId, ContractAccesses>,
}

impl ReadsPerContract {
    /// Mark some key as accessed
    fn mark(&mut self, key: &[u8], column_id: u32) {
        if column_id == Column::ContractsAssets.as_u32() {
            let key = ContractsAssetKey::from_slice(key).unwrap();
            self.per_contract
                .entry(*key.contract_id())
                .or_default()
                .assets
                .insert(*key.asset_id());
        } else if column_id == Column::ContractsState.as_u32() {
            let key = ContractsStateKey::from_slice(key).unwrap();
            self.per_contract
                .entry(*key.contract_id())
                .or_default()
                .slots
                .insert(*key.state_key());
        }
    }

    /// Go through the changes of a transaction and mark all changed keys as accessed.
    pub(crate) fn finalize(
        mut self,
        changes: &Changes,
    ) -> BTreeMap<ContractId, ContractAccesses> {
        for (change_column, change) in changes {
            for (key, _) in change.iter() {
                self.mark(key, *change_column);
            }
        }
        self.per_contract
    }
}

pub struct StorageAccessRecorder<S>
where
    S: KeyValueInspect,
{
    pub storage: S,
    pub record: Arc<Mutex<ReadsPerContract>>,
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

    /// Mark some key as accessed without actually reading it.
    fn mark(&self, key: &[u8], column_id: u32) {
        self.record.lock().mark(key, column_id);
    }
}

impl<S> KeyValueInspect for StorageAccessRecorder<S>
where
    S: KeyValueInspect,
{
    type Column = S::Column;

    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        self.mark(key, column.id());
        self.storage.get(key, column)
    }

    fn exists(&self, key: &[u8], column: Self::Column) -> StorageResult<bool> {
        self.mark(key, column.id());
        self.storage.exists(key, column)
    }

    fn size_of_value(
        &self,
        key: &[u8],
        column: Self::Column,
    ) -> StorageResult<Option<usize>> {
        self.mark(key, column.id());
        self.storage.size_of_value(key, column)
    }

    fn read(
        &self,
        key: &[u8],
        column: Self::Column,
        offset: usize,
        buf: &mut [u8],
    ) -> StorageResult<bool> {
        self.mark(key, column.id());
        self.storage.read(key, column, offset, buf)
    }
}
