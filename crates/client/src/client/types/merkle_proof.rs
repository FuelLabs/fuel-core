use crate::client::{schema, types::primitives::MerkleRoot};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MerkleProof {
    /// The proof set of the message proof.
    pub proof_set: Vec<MerkleRoot>,
    /// The index that was used to produce this proof.
    pub proof_index: u64,
}

// GraphQL Translation

impl From<schema::message::MerkleProof> for MerkleProof {
    fn from(value: schema::message::MerkleProof) -> Self {
        let proof_set = value
            .proof_set
            .iter()
            .cloned()
            .map(Into::into)
            .collect::<Vec<_>>();
        let proof_index = value.proof_index.into();
        Self {
            proof_set,
            proof_index,
        }
    }
}
