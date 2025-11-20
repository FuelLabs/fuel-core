//! Consensus configuration, including specific consensus types like PoA

use crate::{
    blockchain::primitives::BlockId,
    fuel_tx::Input,
    fuel_types::{
        Address,
        Bytes32,
    },
};

// Different types of consensus are represented as separate modules
pub mod poa;

use poa::PoAConsensus;

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
/// The consensus related data that doesn't live on the
/// header.
pub enum Consensus {
    /// The genesis block defines the consensus rules for future blocks.
    Genesis(Genesis),
    /// Proof of authority consensus
    PoA(PoAConsensus),
}

impl Consensus {
    /// Retrieve the block producer address from the consensus data
    pub fn block_producer(&self, block_id: &BlockId) -> anyhow::Result<Address> {
        match &self {
            Consensus::Genesis(_) => Ok(Address::zeroed()),
            Consensus::PoA(poa_data) => {
                let public_key = poa_data
                    .signature
                    .recover(block_id.as_message())
                    .map_err(|e| anyhow::anyhow!("Can't recover public key: {:?}", e))?;
                let address = Input::owner(&public_key);
                Ok(address)
            }
        }
    }
}

impl Default for Consensus {
    fn default() -> Self {
        Consensus::PoA(Default::default())
    }
}

/// Consensus type that a block is using
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsensusType {
    /// Proof of authority
    PoA,
}

/// A sealed entity with consensus info.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Sealed<Entity> {
    /// The actual value
    pub entity: Entity,
    // TODO: Rename to `seal`
    /// Consensus info
    pub consensus: Consensus,
}

/// The first block of the blockchain is a genesis block. It determines the initial state of the
/// network - contracts states, contracts balances, unspent coins, and messages. It also contains
/// the hash on the initial config of the network that defines the consensus rules for following
/// blocks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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
    /// The Binary Merkle Tree root of all processed transaction ids.
    pub transactions_root: Bytes32,
}
