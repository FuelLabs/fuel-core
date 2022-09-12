use crate::{
    common::{
        fuel_asm::Word,
        fuel_tx::{
            Address,
            AssetId,
        },
    },
    model::BlockHeight,
};
#[cfg(graphql)]
use async_graphql::Enum;

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone)]
pub struct Coin {
    pub owner: Address,
    pub amount: Word,
    pub asset_id: AssetId,
    pub maturity: BlockHeight,
    pub status: CoinStatus,
    pub block_created: BlockHeight,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Copy, Clone, Eq, PartialOrd, PartialEq)]
pub enum CoinStatus {
    Unspent,
    Spent,
}
