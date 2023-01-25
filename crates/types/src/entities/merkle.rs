//! Types for Merkle trees

use crate::{
    fuel_merkle::binary::in_memory,
    fuel_tx::Bytes32,
};

/// Metadata for dense Merkle trees
#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DenseMerkleMetadata {
    /// The root hash of the dense Merkle tree structure
    pub root: Bytes32,
    /// The version of the dense Merkle tree structure is equal to the number of
    /// leaves. Every time we append a new leaf to the Merkle tree data set, we
    /// increment the version number.
    pub version: u64,
}

impl Default for DenseMerkleMetadata {
    fn default() -> Self {
        let mut empty_merkle_tree = in_memory::MerkleTree::new();
        Self {
            root: empty_merkle_tree.root().into(),
            version: 0,
        }
    }
}
