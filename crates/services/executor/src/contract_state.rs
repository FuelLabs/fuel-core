use alloc::{collections::BTreeMap, vec::Vec};

use fuel_core_storage::{
    column::Column, kv_store::WriteOperation, transactional::Changes, ContractsAssetKey, ContractsStateKey
};
use fuel_core_types::{
    fuel_tx::{Bytes32, AssetId, ContractId, Word},
    services::executor::StorageReadReplayEvent,
};
use sha2::{
    Digest,
    Sha256,
};

/// Leave `changes` empty when there are no changes yet, i.e. when computing state before execution
pub fn compute_balances_hash(record: &[StorageReadReplayEvent], changes: &Changes) -> Bytes32 {
    let mut touched_assets: BTreeMap<ContractId, BTreeMap<AssetId, Word>> = BTreeMap::new();
    for r in record {
        if r.column == Column::ContractsAssets as u32 {
            let key = ContractsAssetKey::from_slice(&r.key).unwrap();
            let contract_id = key.contract_id();
            let asset_id = key.asset_id();

            touched_assets
                .entry(*contract_id)
                .or_default()
                .insert(asset_id.clone(), r.value.clone().map(|v| {
                    let mut buf = [0; 8];
                    buf.copy_from_slice(v.as_slice());
                    Word::from_be_bytes(buf)
                }).unwrap_or(0));
        }
    }

    for (change_column, change) in changes {
        if *change_column == Column::ContractsAssets as u32 {
            for (key, value) in change.iter() {
                let key = ContractsStateKey::from_slice(&key).unwrap();
                let contract_id = key.contract_id();
                let state_key = key.state_key();

                touched_assets
                    .get_mut(contract_id)
                    .expect("Column cannot have been changed if it was not accessed")
                    .insert(AssetId::from(*state_key.clone()), match value {
                        WriteOperation::Insert(v) => {
                            let mut buf = [0; 8];
                            buf.copy_from_slice(v);
                            Word::from_be_bytes(buf)
                        },
                        WriteOperation::Remove => 0,
                    });
            }
        }
    }

    let mut hasher = Sha256::new();
    for (contract_id, values) in touched_assets {
        hasher.update(&*contract_id);
        hasher.update(&(values.len() as u64).to_be_bytes());
        for (state_key, state_value) in values {
            hasher.update(&state_key);
            hasher.update(&state_value.to_be_bytes());
        }
    }
    let digest: [u8; 32] = hasher.finalize().into();
    Bytes32::from(digest)
}

/// Leave `changes` empty when there are no changes yet, i.e. when computing state before execution
pub fn compute_state_hash(record: &[StorageReadReplayEvent], changes: &Changes) -> Bytes32 {
    let mut touched_slots: BTreeMap<ContractId, BTreeMap<Bytes32, Option<Vec<u8>>>> = BTreeMap::new();
    for r in record {
        if r.column == Column::ContractsState as u32 {
            let key = ContractsStateKey::from_slice(&r.key).unwrap();
            let contract_id = key.contract_id();
            let state_key = key.state_key();

            touched_slots
                .entry(*contract_id)
                .or_default()
                .insert(state_key.clone(), r.value.clone());
        }
    }

    for (change_column, change) in changes {
        if *change_column == Column::ContractsState as u32 {
            for (key, value) in change.iter() {
                let key = ContractsStateKey::from_slice(&key).unwrap();
                let contract_id = key.contract_id();
                let state_key = key.state_key();
    
                touched_slots
                    .get_mut(contract_id)
                    .expect("Column cannot have been changed if it was not accessed")
                    .insert(state_key.clone(), match value {
                        WriteOperation::Insert(v) => Some(v.to_vec()),
                        WriteOperation::Remove => None,
                    });
            }
        }
    }

    let mut hasher = Sha256::new();
    for (contract_id, values) in touched_slots {
        hasher.update(&*contract_id);
        hasher.update(&(values.len() as u64).to_be_bytes());
        for (state_key, state_value) in values {
            hasher.update(&state_key);
            if let Some(value) = state_value {
                hasher.update(&[1u8]);
                hasher.update(&(value.len() as Word).to_be_bytes());
                hasher.update(&value);
            } else {
                hasher.update(&[0u8]);
            }
        }
    }
    let digest: [u8; 32] = hasher.finalize().into();
    Bytes32::from(digest)
}
