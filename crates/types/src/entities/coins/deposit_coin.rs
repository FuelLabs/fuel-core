//! DepositCoin

use super::CoinStatus;
use crate::{
    blockchain::primitives::DaBlockHeight,
    entities::Nonce,
    fuel_types::{
        Address,
        Word,
    },
};

/// Message send from Da layer to fuel by bridge
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CompressedDepositCoin {
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

impl CompressedDepositCoin {
    /// Returns the id of the message
    pub fn id(&self) -> &Word {
        &self.nonce
    }

    /// Decompress the deposit coin
    pub fn decompress(self, status: CoinStatus) -> DepositCoin {
        DepositCoin {
            sender: self.sender,
            recipient: self.recipient,
            nonce: self.nonce,
            amount: self.amount,
            da_height: self.da_height,
            status,
        }
    }
}

/// Message send from Da layer to fuel by bridge
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub struct DepositCoin {
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
    /// Whether a message has been spent or not
    pub status: CoinStatus,
}

impl DepositCoin {
    /// Compress the message
    pub fn compress(self) -> CompressedDepositCoin {
        CompressedDepositCoin {
            sender: self.sender,
            recipient: self.recipient,
            nonce: self.nonce,
            amount: self.amount,
            da_height: self.da_height,
        }
    }
}
