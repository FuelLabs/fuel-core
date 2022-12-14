use crate::{
    blockchain::primitives::BlockId,
    fuel_tx::Input,
    fuel_types::Address,
};

mod genesis;
mod poa;
mod sealed;
mod vote;

pub use genesis::Genesis;
pub use poa::PoAConsensus;
pub use sealed::Sealed;
pub use vote::ConsensusVote;

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// The consensus related data that doesn't live on the
/// header.
pub enum Consensus {
    /// The genesis block defines the consensus rules for future blocks.
    Genesis(Genesis),
    PoA(PoAConsensus),
}

impl Consensus {
    /// Retrieve the block producer address from the consensus data
    pub fn block_producer(&self, block_id: &BlockId) -> anyhow::Result<Address> {
        match &self {
            Consensus::Genesis(_) => Ok(Address::zeroed()),
            Consensus::PoA(poa_data) => {
                let public_key = poa_data.signature.recover(block_id.as_message())?;
                let address = Input::owner(&public_key);
                Ok(address)
            }
        }
    }
}

#[cfg(any(test, feature = "test-helpers"))]
impl Default for Consensus {
    fn default() -> Self {
        Consensus::PoA(Default::default())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsensusType {
    PoA,
}
