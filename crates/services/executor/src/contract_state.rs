#[cfg(feature = "std")]
use std::collections::BTreeMap;

#[cfg(all(feature = "alloc", not(feature = "std")))]
use alloc::{
    collections::BTreeMap,
    vec::Vec,
};

use fuel_core_storage::{
    ContractsAssetKey,
    ContractsStateKey,
    column::Column,
    kv_store::WriteOperation,
    transactional::Changes,
};
use fuel_core_types::{
    fuel_tx::{
        AssetId,
        Bytes32,
        ContractId,
        Word,
    },
    services::executor::StorageReadReplayEvent,
};
use sha2::{
    Digest,
    Sha256,
};

/// Computes a hash of all contract balances that were read or modified.
/// The hash is not dependent on the order of reads or writes.
/// Leave `changes` empty when there are no changes yet, i.e. when computing state before execution
pub fn compute_balances_hash(
    contract_id: &ContractId,
    record: &[StorageReadReplayEvent],
    changes: &Changes,
) -> Bytes32 {
    let mut touched_assets: BTreeMap<AssetId, Word> = BTreeMap::new();
    for r in record {
        if r.column == Column::ContractsAssets as u32 {
            let key = ContractsAssetKey::from_slice(&r.key).unwrap();
            let r_contract_id = key.contract_id();
            let r_asset_id = key.asset_id();

            if r_contract_id != contract_id {
                continue;
            }

            touched_assets.insert(
                *r_asset_id,
                r.value
                    .clone()
                    .map(|v| {
                        let mut buf = [0; 8];
                        buf.copy_from_slice(v.as_slice());
                        Word::from_be_bytes(buf)
                    })
                    .unwrap_or(0),
            );
        }
    }

    for (change_column, change) in changes {
        if *change_column == Column::ContractsAssets as u32 {
            for (key, value) in change.iter() {
                let key = ContractsStateKey::from_slice(key).unwrap();
                let c_contract_id = key.contract_id();
                let c_state_key = key.state_key();

                if c_contract_id != contract_id {
                    continue;
                }

                touched_assets.insert(
                    AssetId::from(**c_state_key),
                    match value {
                        WriteOperation::Insert(v) => {
                            let mut buf = [0; 8];
                            buf.copy_from_slice(v);
                            Word::from_be_bytes(buf)
                        }
                        WriteOperation::Remove => 0,
                    },
                );
            }
        }
    }

    let mut hasher = Sha256::new();
    for (state_key, state_value) in touched_assets {
        hasher.update(state_key);
        hasher.update(state_value.to_be_bytes());
    }
    let digest: [u8; 32] = hasher.finalize().into();
    Bytes32::from(digest)
}

/// Computes a hash of all contract state slots that were read or modified.
/// The hash is not dependent on the order of reads or writes.
/// Leave `changes` empty when there are no changes yet, i.e. when computing state before execution
pub fn compute_state_hash(
    contract_id: &ContractId,
    record: &[StorageReadReplayEvent],
    changes: &Changes,
) -> Bytes32 {
    let mut touched_slots: BTreeMap<Bytes32, Option<Vec<u8>>> = BTreeMap::new();
    for r in record {
        if r.column == Column::ContractsState as u32 {
            let key = ContractsStateKey::from_slice(&r.key).unwrap();
            let r_contract_id = key.contract_id();
            let r_state_key = key.state_key();

            if r_contract_id != contract_id {
                continue;
            }

            touched_slots.insert(*r_state_key, r.value.clone());
        }
    }

    for (change_column, change) in changes {
        if *change_column == Column::ContractsState as u32 {
            for (key, value) in change.iter() {
                let key = ContractsStateKey::from_slice(key).unwrap();
                let c_contract_id = key.contract_id();
                let c_state_key = key.state_key();

                if c_contract_id != contract_id {
                    continue;
                }

                touched_slots.insert(
                    *c_state_key,
                    match value {
                        WriteOperation::Insert(v) => Some(v.to_vec()),
                        WriteOperation::Remove => None,
                    },
                );
            }
        }
    }

    let mut hasher = Sha256::new();
    for (state_key, state_value) in touched_slots {
        hasher.update(state_key);
        if let Some(value) = state_value {
            hasher.update([1u8]);
            hasher.update((value.len() as Word).to_be_bytes());
            hasher.update(&value);
        } else {
            hasher.update([0u8]);
        }
    }
    let digest: [u8; 32] = hasher.finalize().into();
    Bytes32::from(digest)
}
