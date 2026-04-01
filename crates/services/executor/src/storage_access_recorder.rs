use core::cell::RefCell;
use fuel_core_storage::{
    ContractsAssetKey,
    ContractsStateKey,
    Result as StorageResult,
    StorageAsRef,
    StorageInspect,
    StorageReadError,
    column::Column,
    iter::changes_iterator::ChangesIterator,
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
use fuel_core_types::fuel_tx::{
    AssetId,
    Bytes32,
    ContractId,
};

#[cfg(feature = "std")]
use std::collections::{
    BTreeMap,
    BTreeSet,
};

#[cfg(not(feature = "std"))]
use alloc::{
    collections::{
        BTreeMap,
        BTreeSet,
    },
    vec::Vec,
};

#[derive(Debug, Clone, Default)]
pub(crate) struct ContractAccesses {
    pub assets: BTreeSet<AssetId>,
    pub slots: BTreeSet<Bytes32>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ContractAccessesWithValues {
    pub assets: BTreeMap<AssetId, Option<u64>>,
    pub slots: BTreeMap<Bytes32, Option<Vec<u8>>>,
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

    /// Returns slot values before and after applying the changes
    pub(crate) fn finalize<S>(
        self,
        storage: S,
        changes: &Changes,
    ) -> StorageResult<(
        BTreeMap<ContractId, ContractAccessesWithValues>,
        BTreeMap<ContractId, ContractAccessesWithValues>,
    )>
    where
        S: StorageInspect<ContractsAssets, Error = fuel_core_storage::Error>
            + StorageInspect<ContractsState, Error = fuel_core_storage::Error>,
    {
        // Fetch original values from the storage
        let before: BTreeMap<ContractId, ContractAccessesWithValues> = self
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
                            .map(|v| v.map(|c| c.into_owned()))?;
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
                            .map(|v| {
                                v.map(|c| {
                                    let bytes: Vec<u8> = c.into_owned().into();
                                    bytes
                                })
                            })?;
                        Ok((slot_key, value))
                    })
                    .collect::<StorageResult<_>>()?;
                Ok((contract_id, ContractAccessesWithValues { assets, slots }))
            })
            .collect::<StorageResult<_>>()?;

        // Update final values from the changes
        let mut after: BTreeMap<ContractId, ContractAccessesWithValues> =
            Default::default();

        let sc = StorageChanges::Changes(changes.clone());
        let ci = ChangesIterator::new(&sc);

        for item in ci.iter_all_writes::<ContractsAssets>(None) {
            let (key, value) = item?;
            let entry = after
                .entry(*key.contract_id())
                .or_default()
                .assets
                .entry(*key.asset_id())
                .or_default();

            *entry = value;
        }
        for item in ci.iter_all_writes::<ContractsState>(None) {
            let (key, value) = item?;
            let entry = after
                .entry(*key.contract_id())
                .or_default()
                .slots
                .entry(*key.state_key())
                .or_default();
            *entry = value.map(|data| data.into());
        }

        Ok((before, after))
    }
}

pub struct StorageAccessRecorder<S>
where
    S: KeyValueInspect,
{
    pub storage: S,
    pub record: RefCell<ReadsPerContract>,
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

    pub fn into_inner(self) -> (S, ReadsPerContract) {
        (self.storage, self.record.take())
    }

    /// Mark some key as accessed without actually reading it.
    fn mark(&self, key: &[u8], column_id: u32) {
        self.record.borrow_mut().mark(key, column_id);
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

    fn read_exact(
        &self,
        key: &[u8],
        column: Self::Column,
        offset: usize,
        buf: &mut [u8],
    ) -> StorageResult<Result<usize, StorageReadError>> {
        self.mark(key, column.id());
        self.storage.read_exact(key, column, offset, buf)
    }

    fn read_zerofill(
        &self,
        key: &[u8],
        column: Self::Column,
        offset: usize,
        buf: &mut [u8],
    ) -> StorageResult<Result<usize, StorageReadError>> {
        self.mark(key, column.id());
        self.storage.read_zerofill(key, column, offset, buf)
    }
}
