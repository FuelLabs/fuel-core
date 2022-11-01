use fuel_core_interfaces::common::{
    fuel_merkle::binary::in_memory,
    fuel_tx::Bytes32,
};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct DenseMerkleMetadata {
    pub root: Bytes32,
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
