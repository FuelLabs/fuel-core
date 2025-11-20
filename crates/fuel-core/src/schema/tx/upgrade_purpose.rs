use crate::schema::scalars::{Bytes32, U16};
use async_graphql::{Object, Union};

#[derive(Union)]
pub enum UpgradePurpose {
    /// The upgrade is performed to change the consensus parameters.
    ConsensusParameters(ConsensusParametersPurpose),
    /// The upgrade is performed to change the state transition function.
    StateTransition(StateTransitionPurpose),
}

pub struct ConsensusParametersPurpose {
    /// The index of the witness in the [`Witnesses`] field that contains
    /// the serialized consensus parameters.
    witness_index: U16,
    /// The hash of the serialized consensus parameters.
    /// Since the serialized consensus parameters live inside witnesses(malleable
    /// data), any party can override them. The `checksum` is used to verify that the
    /// data was not modified.
    checksum: Bytes32,
}

#[Object]
impl ConsensusParametersPurpose {
    async fn witness_index(&self) -> U16 {
        self.witness_index
    }

    async fn checksum(&self) -> Bytes32 {
        self.checksum
    }
}

pub struct StateTransitionPurpose {
    /// The Merkle root of the new bytecode of the state transition function.
    /// The bytecode must be present on the blockchain(should be known by the
    /// network) at the moment of inclusion of this transaction.
    root: Bytes32,
}

#[Object]
impl StateTransitionPurpose {
    async fn root(&self) -> Bytes32 {
        self.root
    }
}

impl From<fuel_core_types::fuel_tx::UpgradePurpose> for UpgradePurpose {
    fn from(value: fuel_core_types::fuel_tx::UpgradePurpose) -> Self {
        match value {
            fuel_core_types::fuel_tx::UpgradePurpose::ConsensusParameters {
                witness_index,
                checksum,
            } => UpgradePurpose::ConsensusParameters(ConsensusParametersPurpose {
                witness_index: witness_index.into(),
                checksum: checksum.into(),
            }),
            fuel_core_types::fuel_tx::UpgradePurpose::StateTransition { root } => {
                UpgradePurpose::StateTransition(StateTransitionPurpose {
                    root: root.into(),
                })
            }
        }
    }
}
