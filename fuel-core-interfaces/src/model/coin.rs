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

/// Represents the user's coin for some asset with `asset_id`.
/// The `Coin` is either `CoinStatus::Spent` or `CoinStatus::Unspent`. If the coin is unspent,
/// it can be used as an input to the transaction and can be spent up to the `amount`.
/// After usage as an input of a transaction, the `Coin` becomes `CoinStatus::Spent`.
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
#[repr(u8)]
pub enum CoinStatus {
    Unspent,
    Spent,
}
