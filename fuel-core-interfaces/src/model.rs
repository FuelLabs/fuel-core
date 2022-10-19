mod block;
mod block_height;
mod coin;
mod da_block_height;
mod messages;
mod txpool;
mod vote;

use crate::common::fuel_types::{
    Address,
    Bytes32,
};
pub use block::{
    ConsensusType,
    FuelApplicationHeader,
    FuelBlock,
    FuelBlockConsensus,
    FuelBlockDb,
    FuelBlockHeader,
    FuelBlockPoAConsensus,
    FuelConsensusHeader,
    PartialFuelBlock,
    PartialFuelBlockHeader,
    SealedFuelBlock,
};
pub use block_height::BlockHeight;
pub use coin::{
    Coin,
    CoinStatus,
};
pub use da_block_height::DaBlockHeight;
pub use messages::*;
pub use txpool::{
    ArcTx,
    TxInfo,
};
pub use vote::ConsensusVote;

/// Validator address used for registration of validator on DA layer
pub type ValidatorId = Address;
/// Consensus public key used for Fuel network consensus protocol to
/// check signatures. ConsensusId is assigned by validator.
pub type ConsensusId = Bytes32;
