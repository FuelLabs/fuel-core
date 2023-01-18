//! Types for Merkle metadata

use crate::{
    fuel_merkle::binary::in_memory,
    fuel_tx::Bytes32,
};

/// Metadata for dense Merkle trees
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct DenseMerkleMetadata {
    /// The root hash of the dense Merkle tree structure
    pub root: Bytes32,
    /// The number of leaves in the dense Merkle tree structure
    pub leaves_count: u64,
}

impl Default for DenseMerkleMetadata {
    fn default() -> Self {
        let mut empty_merkle_tree = in_memory::MerkleTree::new();
        Self {
            root: empty_merkle_tree.root().into(),
            leaves_count: 0,
        }
    }
}
