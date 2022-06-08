mod block;
mod block_height;
mod coin;
mod deposit_coin;
mod txpool;

pub use block::{FuelBlock, FuelBlockConsensus, FuelBlockDb, FuelBlockHeader, SealedFuelBlock};
pub use block_height::BlockHeight;
pub use coin::{Coin, CoinStatus};
pub use deposit_coin::DepositCoin;
pub use txpool::{ArcTx, TxInfo};

pub type DaBlockHeight = u32;
pub type ValidatorStake = u64;

/// TODO temporary structure
#[derive(Clone, Debug)]
pub struct ConsensusVote {}
