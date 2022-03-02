use crate::model::fuel_block::BlockHeight;
use async_graphql::Enum;
use fuel_asm::Word;
use fuel_tx::{Address, AssetId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Coin {
    pub owner: Address,
    pub amount: Word,
    pub asset_id: AssetId,
    pub maturity: BlockHeight,
    pub status: CoinStatus,
    pub block_created: BlockHeight,
}

#[derive(Debug, Copy, Clone, Eq, PartialOrd, PartialEq, Serialize, Deserialize, Enum)]
pub enum CoinStatus {
    Unspent,
    Spent,
}
