//! MessageCoin

use crate::{
    blockchain::primitives::DaBlockHeight,
    fuel_types::{
        Address,
        Nonce,
        Word,
    },
};

/// Message send from Da layer to fuel by bridge
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub struct MessageCoin {
    /// Account that sent the message from the da layer
    pub sender: Address,
    /// Fuel account receiving the message
    pub recipient: Address,
    /// Nonce must be unique. It's used to prevent replay attacks
    pub nonce: Nonce,
    /// The amount of the base asset of Fuel chain sent along this message
    pub amount: Word,
    /// The block height from the parent da layer that originated this message
    pub da_height: DaBlockHeight,
}
