use crate::fuel_types::Bytes32;

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
