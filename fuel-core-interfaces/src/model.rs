mod block;
mod block_height;
mod coin;
mod txpool;
mod deposit_coin;

pub use block::{FuelBlock, FuelBlockConsensus, FuelBlockDb, FuelBlockHeader, SealedFuelBlock};
pub use block_height::BlockHeight;
pub use coin::{Coin, CoinStatus};
pub use txpool::{ArcTx, TxInfo};
pub use deposit_coin::DepositCoin;

pub type DaBlockHeight = u64;
pub type ValidatorStake = u64;
