#[cfg(feature = "std")]
use std::collections::BTreeMap;

#[cfg(all(feature = "alloc", not(feature = "std")))]
use alloc::{
    collections::BTreeMap,
    vec::Vec,
};

use fuel_core_types::fuel_tx::{
    AssetId,
    Bytes32,
    Word,
};
use sha2::{
    Digest,
    Sha256,
};

/// Computes a hash of all contract balances that were read or modified.
/// The hash is not dependent on the order of reads or writes.
pub fn compute_balances_hash(touched_assets: BTreeMap<AssetId, Word>) -> Bytes32 {
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
pub fn compute_state_hash(touched_slots: BTreeMap<Bytes32, Option<&Vec<u8>>>) -> Bytes32 {
    let mut hasher = Sha256::new();
    for (state_key, state_value) in touched_slots {
        hasher.update(state_key);
        if let Some(value) = state_value {
            hasher.update([1u8]);
            hasher.update((value.len() as Word).to_be_bytes());
            hasher.update(value);
        } else {
            hasher.update([0u8]);
        }
    }
    let digest: [u8; 32] = hasher.finalize().into();
    Bytes32::from(digest)
}
