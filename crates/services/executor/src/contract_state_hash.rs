#[cfg(feature = "std")]
use std::collections::BTreeSet;

#[cfg(all(feature = "alloc", not(feature = "std")))]
use alloc::{
    collections::BTreeSet,
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
pub fn compute_balances_hash<E>(
    accessed: &BTreeSet<AssetId>,
    get_value: impl Fn(&AssetId) -> Result<Word, E>,
) -> Result<Bytes32, E> {
    let mut hasher = Sha256::new();
    for key in accessed {
        hasher.update(key);
        hasher.update(get_value(key)?.to_be_bytes());
    }
    let digest: [u8; 32] = hasher.finalize().into();
    Ok(Bytes32::from(digest))
}

/// Computes a hash of all contract state slots that were read or modified.
/// The hash is not dependent on the order of reads or writes.
pub fn compute_state_hash<E>(
    accessed: &BTreeSet<Bytes32>,
    get_value: impl Fn(&Bytes32) -> Result<Option<Vec<u8>>, E>,
) -> Result<Bytes32, E> {
    let mut hasher = Sha256::new();
    for key in accessed {
        hasher.update(key);
        if let Some(value) = get_value(key)? {
            hasher.update([1u8]);
            hasher.update((value.len() as Word).to_be_bytes());
            hasher.update(&value);
        } else {
            hasher.update([0u8]);
        }
    }
    let digest: [u8; 32] = hasher.finalize().into();
    Ok(Bytes32::from(digest))
}
