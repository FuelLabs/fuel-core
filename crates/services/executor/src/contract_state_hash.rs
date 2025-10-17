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
pub fn compute_balances_hash(accessed: &BTreeMap<AssetId, Option<Word>>) -> Bytes32 {
    let mut hasher = Sha256::new();
    for (key, value) in accessed {
        hasher.update(key);
        if let Some(value) = value {
            hasher.update([1u8]);
            hasher.update(value.to_be_bytes());
        } else {
            hasher.update([0u8]);
        }
    }
    Bytes32::new(hasher.finalize().into())
}

/// Computes a hash of all contract state slots that were read or modified.
/// The hash is not dependent on the order of reads or writes.
pub fn compute_state_hash(accessed: &BTreeMap<Bytes32, Option<Vec<u8>>>) -> Bytes32 {
    let mut hasher = Sha256::new();
    for (key, value) in accessed {
        hasher.update(key);
        if let Some(value) = value {
            hasher.update([1u8]);
            hasher.update((value.len() as Word).to_be_bytes());
            hasher.update(value);
        } else {
            hasher.update([0u8]);
        }
    }
    Bytes32::new(hasher.finalize().into())
}
