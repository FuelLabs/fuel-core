//! Coin

use crate::{
    fuel_asm::Word,
    fuel_tx::{
        input::coin::{
            CoinPredicate,
            CoinSigned,
        },
        Input,
        TxPointer,
        UtxoId,
    },
    fuel_types::{
        Address,
        AssetId,
        BlockHeight,
    },
};

/// Represents the user's coin for some asset with `asset_id`.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Copy, Clone, Eq, PartialOrd, PartialEq)]
pub struct Coin {
    /// The coin utxo id.
    pub utxo_id: UtxoId,
    /// The address with permission to spend this coin
    pub owner: Address,
    /// Amount of coins
    pub amount: Word,
    /// Different incompatible coins can coexist with different asset ids.
    /// This is the "color" of the coin.
    pub asset_id: AssetId,
    /// This coin cannot be spent until the given height
    pub maturity: BlockHeight,
    /// Indexes the block and transaction this coin originated from
    pub tx_pointer: TxPointer,
}

impl Coin {
    /// Compress the coin to minimize the serialized size.
    pub fn compress(self) -> CompressedCoin {
        CompressedCoin::V1(CompressedCoinV1 {
            owner: self.owner,
            amount: self.amount,
            asset_id: self.asset_id,
            maturity: self.maturity,
            tx_pointer: self.tx_pointer,
        })
    }
}

/// The compressed version of the `Coin` with minimum fields required for
/// the proper work of the blockchain.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CompressedCoin {
    /// CompressedCoin Version 1
    V1(CompressedCoinV1),
}

#[cfg(any(test, feature = "test-helpers"))]
impl Default for CompressedCoin {
    fn default() -> Self {
        Self::V1(Default::default())
    }
}

/// CompressedCoin Version 1
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct CompressedCoinV1 {
    /// The address with permission to spend this coin
    pub owner: Address,
    /// Amount of coins
    pub amount: Word,
    /// Different incompatible coins can coexist with different asset ids.
    /// This is the "color" of the coin.
    pub asset_id: AssetId,
    /// This coin cannot be spent until the given height
    pub maturity: BlockHeight,
    /// Indexes the block and transaction this coin originated from
    pub tx_pointer: TxPointer,
}

impl From<CompressedCoinV1> for CompressedCoin {
    fn from(value: CompressedCoinV1) -> Self {
        Self::V1(value)
    }
}

impl CompressedCoin {
    /// Uncompress the coin.
    pub fn uncompress(self, utxo_id: UtxoId) -> Coin {
        match self {
            CompressedCoin::V1(coin) => Coin {
                utxo_id,
                owner: coin.owner,
                amount: coin.amount,
                asset_id: coin.asset_id,
                maturity: coin.maturity,
                tx_pointer: coin.tx_pointer,
            },
        }
    }

    /// Get the owner of the coin
    pub fn owner(&self) -> &Address {
        match self {
            CompressedCoin::V1(coin) => &coin.owner,
        }
    }

    /// Set the owner of the coin
    pub fn set_owner(&mut self, owner: Address) {
        match self {
            CompressedCoin::V1(coin) => coin.owner = owner,
        }
    }

    /// Get the amount of the coin
    pub fn amount(&self) -> &Word {
        match self {
            CompressedCoin::V1(coin) => &coin.amount,
        }
    }

    /// Set the amount of the coin
    pub fn set_amount(&mut self, amount: Word) {
        match self {
            CompressedCoin::V1(coin) => coin.amount = amount,
        }
    }

    /// Get the asset ID of the coin
    pub fn asset_id(&self) -> &AssetId {
        match self {
            CompressedCoin::V1(coin) => &coin.asset_id,
        }
    }

    /// Set the asset ID of the coin
    pub fn set_asset_id(&mut self, asset_id: AssetId) {
        match self {
            CompressedCoin::V1(coin) => coin.asset_id = asset_id,
        }
    }

    /// Get the maturity of the coin
    pub fn maturity(&self) -> &BlockHeight {
        match self {
            CompressedCoin::V1(coin) => &coin.maturity,
        }
    }

    /// Set the maturity of the coin
    pub fn set_maturity(&mut self, maturity: BlockHeight) {
        match self {
            CompressedCoin::V1(coin) => coin.maturity = maturity,
        }
    }

    /// Get the TX Pointer of the coin
    pub fn tx_pointer(&self) -> &TxPointer {
        match self {
            CompressedCoin::V1(coin) => &coin.tx_pointer,
        }
    }

    /// Set the TX Pointer of the coin
    pub fn set_tx_pointer(&mut self, tx_pointer: TxPointer) {
        match self {
            CompressedCoin::V1(coin) => coin.tx_pointer = tx_pointer,
        }
    }

    /// Verifies the integrity of the coin.
    ///
    /// Returns `None`, if the `input` is not a coin.
    /// Otherwise, returns the result of the field comparison.
    pub fn matches_input(&self, input: &Input) -> Option<bool> {
        match input {
            Input::CoinSigned(CoinSigned {
                owner,
                amount,
                asset_id,
                ..
            })
            | Input::CoinPredicate(CoinPredicate {
                owner,
                amount,
                asset_id,
                ..
            }) => match self {
                CompressedCoin::V1(coin) => Some(
                    owner == &coin.owner
                        && amount == &coin.amount
                        && asset_id == &coin.asset_id,
                ),
            },
            _ => None,
        }
    }
}
