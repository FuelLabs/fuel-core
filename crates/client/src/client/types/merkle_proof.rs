use crate::client::types::scalars::MerkleRoot;

#[derive(Debug)]
pub struct MerkleProof {
    /// The proof set of the message proof.
    pub proof_set: Vec<MerkleRoot>,
    /// The index that was used to produce this proof.
    pub proof_index: u64,
}
