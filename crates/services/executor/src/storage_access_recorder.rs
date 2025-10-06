use fuel_core_storage::{
    ContractsAssetKey,
    ContractsStateKey,
    Result as StorageResult,
    StorageAsRef,
    StorageInspect,
    column::Column,
    iter::{
        IteratorOverTableWrites,
        changes_iterator::ChangesIterator,
    },
    kv_store::{
        KeyValueInspect,
        StorageColumn,
        Value,
    },
    tables::{
        ContractsAssets,
        ContractsState,
    },
    transactional::{
        Changes,
        StorageChanges,
    },
};
use fuel_core_types::{
    fuel_tx::{
        AssetId,
        Bytes32,
        ContractId,
    },
    services::executor::{
        ContractAccessesWithValues,
        StorageAccessesWithValues,
    },
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
pub(crate) struct StorageAccesses {
    pub assets: BTreeSet<AssetId>,
    pub slots: BTreeSet<Bytes32>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ReadsPerContract {
    pub per_contract: BTreeMap<ContractId, StorageAccesses>,
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

    /// Returns slot values before and after applying the changes
    pub(crate) fn finalize<S>(
        mut self,
        storage: S,
        changes: &Changes,
    ) -> StorageResult<(ContractAccessesWithValues, ContractAccessesWithValues)>
    where
        S: StorageInspect<ContractsAssets, Error = fuel_core_storage::Error>
            + StorageInspect<ContractsState, Error = fuel_core_storage::Error>
            + StorageAsRef,
    {
        // Mark all changed keys as accessed
        for (change_column, change) in changes {
            for (key, _) in change.iter() {
                self.mark(key, *change_column);
            }
        }

        // Fetch original values from the storage
        let before: BTreeMap<ContractId, StorageAccessesWithValues> = self
            .per_contract
            .into_iter()
            .map(|(contract_id, accesses)| {
                let assets = accesses
                    .assets
                    .into_iter()
                    .map(|asset_id| {
                        let value = storage
                            .storage::<ContractsAssets>()
                            .get(&ContractsAssetKey::new(&contract_id, &asset_id))
                            .map(|v| v.map_or(0, |c| c.into_owned()))?;
                        Ok((asset_id, value))
                    })
                    .collect::<StorageResult<_>>()?;
                let slots = accesses
                    .slots
                    .into_iter()
                    .map(|slot_key| {
                        let value = storage
                            .storage::<ContractsState>()
                            .get(&ContractsStateKey::new(&contract_id, &slot_key))
                            .map(|v| v.map(|c| c.0.clone()))?;
                        Ok((slot_key, value))
                    })
                    .collect::<StorageResult<_>>()?;
                Ok((contract_id, StorageAccessesWithValues { assets, slots }))
            })
            .collect::<StorageResult<_>>()?;

        // Update final values from the changes
        let mut after: BTreeMap<ContractId, StorageAccessesWithValues> = before.clone();

        let sc = StorageChanges::Changes(changes.clone());
        let ci = ChangesIterator::new(&sc);

        for item in ci.iter_all::<ContractsAssets>(None) {
            let (key, value) = item?;
            let entry = after
                .get_mut(key.contract_id())
                .expect("Inserted by marking step above")
                .assets
                .get_mut(key.asset_id())
                .expect("Inserted by marking step above");
            *entry = value.unwrap_or(0);
        }
        for item in ci.iter_all::<ContractsState>(None) {
            let (key, value) = item?;
            let entry = after
                .get_mut(key.contract_id())
                .expect("Inserted by marking step above")
                .slots
                .get_mut(key.state_key())
                .expect("Inserted by marking step above");
            *entry = value.map(|data| data.0);
        }

        Ok((before, after))
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

    pub fn as_inner(&self) -> (&S, ReadsPerContract) {
        (&self.storage, self.record.lock().clone())
    }

    /// Get the recorded accesses so far.
    pub fn get_reads(&self) -> ReadsPerContract {
        self.record.lock().clone()
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
