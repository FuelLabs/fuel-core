//! Coin

use crate::{
    blockchain::primitives::BlockHeight,
    fuel_asm::Word,
    fuel_types::{
        Address,
        AssetId,
    },
};

/// Represents the user's coin for some asset with `asset_id`.
/// The `Coin` is either `CoinStatus::Spent` or `CoinStatus::Unspent`. If the coin is unspent,
/// it can be used as an input to the transaction and can be spent up to the `amount`.
/// After usage as an input of a transaction, the `Coin` becomes `CoinStatus::Spent`.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone)]
pub struct Coin {
    /// The address with permission to spend this coin
    pub owner: Address,
    /// Amount of coins
    pub amount: Word,
    /// Different incompatible coins can coexist with different asset ids.
    /// This is the "color" of the coin.
    pub asset_id: AssetId,
    /// This coin cannot be spent until the given height
    pub maturity: BlockHeight,
    /// Whether a coin has been spent or not
    pub status: CoinStatus,
    /// Which block this coin was created in
    pub block_created: BlockHeight,
}

/// Whether a coin has been spent or not
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Copy, Clone, Eq, PartialOrd, PartialEq)]
#[repr(u8)]
pub enum CoinStatus {
    /// Coin has not been spent
    Unspent,
    /// Coin has been spent
    Spent,
}
