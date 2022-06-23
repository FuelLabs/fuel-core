mod block;
mod block_height;
mod coin;
mod deposit_coin;
mod txpool;
mod vote;

pub use block::{FuelBlock, FuelBlockConsensus, FuelBlockDb, FuelBlockHeader, SealedFuelBlock};
pub use block_height::BlockHeight;
pub use coin::{Coin, CoinStatus};
pub use deposit_coin::DepositCoin;
use fuel_types::{Address, Bytes32};
pub use txpool::{ArcTx, TxInfo};
pub use vote::Vote;

pub type DaBlockHeight = u32;
pub type ValidatorStake = u64;

/// Validator address used for registration of validator on DA layer
pub type ValidatorId = Address;
/// Consensus public key used for Fuel network consensus protocol to
/// check signatures. ConsensusId is assigned by validator.
pub type ConsensusId = Bytes32;

/// TODO temporary structure
#[derive(Clone, Debug)]
pub struct ConsensusVote {}
