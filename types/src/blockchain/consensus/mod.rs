use super::primitives::BlockId;
use crate::{
    fuel_crypto::{
        PublicKey,
        Signature,
    },
    fuel_tx::Input,
    fuel_types::{
        Address,
        Bytes32,
    },
};

pub mod poa;

/// The first block of the blockchain is a genesis block. It determines the initial state of the
/// network - contracts states, contracts balances, unspent coins, and messages. It also contains
/// the hash on the initial config of the network that defines the consensus rules for following
/// blocks.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Genesis {
    /// The chain config define what consensus type to use, what settlement layer to use,
    /// rules of block validity, etc.
    pub chain_config_hash: Bytes32,
    /// The Binary Merkle Tree root of all genesis coins.
    pub coins_root: Bytes32,
    /// The Binary Merkle Tree root of state, balances, contracts code hash of each contract.
    pub contracts_root: Bytes32,
    /// The Binary Merkle Tree root of all genesis messages.
    pub messages_root: Bytes32,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// The consensus related data that doesn't live on the
/// header.
pub enum Consensus {
    /// The genesis block defines the consensus rules for future blocks.
    Genesis(Genesis),
    PoA(poa::PoAConsensus),
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

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(any(test, feature = "test-helpers"), derive(Default))]
/// A sealed entity with consensus info.
pub struct Sealed<Entity> {
    pub entity: Entity,
    pub consensus: Consensus,
}

/// A vote from a validator.
///
/// This is a dummy placeholder for the Vote Struct in fuel-bft
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ConsensusVote {
    block_id: Bytes32,
    height: u64,
    round: u64,
    signature: Signature,
    // step: Step,
    validator: PublicKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsensusType {
    PoA,
}
