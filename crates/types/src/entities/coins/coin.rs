//! Coin

use crate::{
    fuel_asm::Word,
    fuel_tx::{
        input::coin::{
            CoinPredicate,
            CoinSigned,
            DataCoinPredicate,
            DataCoinSigned,
        },
        Input,
        TxPointer,
        UtxoId,
    },
    fuel_types::{
        Address,
        AssetId,
    },
};
use alloc::vec::Vec;

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
            tx_pointer: self.tx_pointer,
        })
    }
}

/// Represents the user's coin for some asset with `asset_id`.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Eq, PartialOrd, PartialEq)]
pub struct DataCoin {
    /// The coin utxo id.
    pub utxo_id: UtxoId,
    /// The address with permission to spend this coin
    pub owner: Address,
    /// Amount of coins
    pub amount: Word,
    /// Different incompatible coins can coexist with different asset ids.
    /// This is the "color" of the coin.
    pub asset_id: AssetId,
    /// Indexes the block and transaction this coin originated from
    pub tx_pointer: TxPointer,
    /// A data field that can be used to store arbitrary data.
    pub data: Vec<u8>,
}

impl DataCoin {
    /// Compress the coin to minimize the serialized size.
    pub fn compress(self) -> CompressedCoin {
        CompressedCoin::V2(CompressedCoinV2 {
            owner: self.owner,
            amount: self.amount,
            asset_id: self.asset_id,
            tx_pointer: self.tx_pointer,
            data: self.data,
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
    /// CompressedCoin Version 2
    V2(CompressedCoinV2),
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
    /// Indexes the block and transaction this coin originated from
    pub tx_pointer: TxPointer,
}

/// CompressedCoin Version 2
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct CompressedCoinV2 {
    /// The address with permission to spend this coin
    pub owner: Address,
    /// Amount of coins
    pub amount: Word,
    /// Different incompatible coins can coexist with different asset ids.
    /// This is the "color" of the coin.
    pub asset_id: AssetId,
    /// Indexes the block and transaction this coin originated from
    pub tx_pointer: TxPointer,
    /// Data
    pub data: Vec<u8>,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Eq, PartialEq)]
/// The uncompressed version of `CompressedCoin`
pub enum UncompressedCoin {
    /// Coin derived from `V1`
    Coin(Coin),
    /// Coin with data field derived from `V2`
    DataCoin(DataCoin),
}

impl UncompressedCoin {
    /// Get the UTXO ID of the coin
    pub fn utxo_id(&self) -> &UtxoId {
        match self {
            UncompressedCoin::Coin(coin) => &coin.utxo_id,
            UncompressedCoin::DataCoin(coin) => &coin.utxo_id,
        }
    }

    /// Get the owner of the coin
    pub fn owner(&self) -> &Address {
        match self {
            UncompressedCoin::Coin(coin) => &coin.owner,
            UncompressedCoin::DataCoin(coin) => &coin.owner,
        }
    }

    /// Get the amount of the coin
    pub fn amount(&self) -> &Word {
        match self {
            UncompressedCoin::Coin(coin) => &coin.amount,
            UncompressedCoin::DataCoin(coin) => &coin.amount,
        }
    }

    /// Get the asset ID of the coin
    pub fn asset_id(&self) -> &AssetId {
        match self {
            UncompressedCoin::Coin(coin) => &coin.asset_id,
            UncompressedCoin::DataCoin(coin) => &coin.asset_id,
        }
    }

    /// Get the TX Pointer of the coin
    pub fn tx_pointer(&self) -> &TxPointer {
        match self {
            UncompressedCoin::Coin(coin) => &coin.tx_pointer,
            UncompressedCoin::DataCoin(coin) => &coin.tx_pointer,
        }
    }

    /// Returns the data attached to coin if it exists
    pub fn data(&self) -> Option<&Vec<u8>> {
        match self {
            UncompressedCoin::Coin(_) => None,
            UncompressedCoin::DataCoin(coin) => Some(&coin.data),
        }
    }
}

impl From<CompressedCoinV1> for CompressedCoin {
    fn from(value: CompressedCoinV1) -> Self {
        Self::V1(value)
    }
}

impl CompressedCoin {
    /// Uncompress the coin.
    pub fn uncompress_coin(self, utxo_id: UtxoId) -> Option<Coin> {
        match self {
            CompressedCoin::V1(coin) => Some(Coin {
                utxo_id,
                owner: coin.owner,
                amount: coin.amount,
                asset_id: coin.asset_id,
                tx_pointer: coin.tx_pointer,
            }),
            CompressedCoin::V2(_coin) => None,
        }
    }

    /// Uncompress the coin to `UncompressedCoin` enum
    pub fn uncompress(self, utxo_id: UtxoId) -> UncompressedCoin {
        match self {
            CompressedCoin::V1(coin) => UncompressedCoin::Coin(Coin {
                utxo_id,
                owner: coin.owner,
                amount: coin.amount,
                asset_id: coin.asset_id,
                tx_pointer: coin.tx_pointer,
            }),
            CompressedCoin::V2(coin) => UncompressedCoin::DataCoin(DataCoin {
                utxo_id,
                owner: coin.owner,
                amount: coin.amount,
                asset_id: coin.asset_id,
                tx_pointer: coin.tx_pointer,
                data: coin.data,
            }),
        }
    }

    /// Get the owner of the coin
    pub fn owner(&self) -> &Address {
        match self {
            CompressedCoin::V1(coin) => &coin.owner,
            CompressedCoin::V2(coin) => &coin.owner,
        }
    }

    /// Set the owner of the coin
    pub fn set_owner(&mut self, owner: Address) {
        match self {
            CompressedCoin::V1(coin) => coin.owner = owner,
            CompressedCoin::V2(coin) => coin.owner = owner,
        }
    }

    /// Get the amount of the coin
    pub fn amount(&self) -> &Word {
        match self {
            CompressedCoin::V1(coin) => &coin.amount,
            CompressedCoin::V2(coin) => &coin.amount,
        }
    }

    /// Set the amount of the coin
    pub fn set_amount(&mut self, amount: Word) {
        match self {
            CompressedCoin::V1(coin) => coin.amount = amount,
            CompressedCoin::V2(coin) => coin.amount = amount,
        }
    }

    /// Get the asset ID of the coin
    pub fn asset_id(&self) -> &AssetId {
        match self {
            CompressedCoin::V1(coin) => &coin.asset_id,
            CompressedCoin::V2(coin) => &coin.asset_id,
        }
    }

    /// Set the asset ID of the coin
    pub fn set_asset_id(&mut self, asset_id: AssetId) {
        match self {
            CompressedCoin::V1(coin) => coin.asset_id = asset_id,
            CompressedCoin::V2(coin) => coin.asset_id = asset_id,
        }
    }

    /// Get the TX Pointer of the coin
    pub fn tx_pointer(&self) -> &TxPointer {
        match self {
            CompressedCoin::V1(coin) => &coin.tx_pointer,
            CompressedCoin::V2(coin) => &coin.tx_pointer,
        }
    }

    /// Set the TX Pointer of the coin
    pub fn set_tx_pointer(&mut self, tx_pointer: TxPointer) {
        match self {
            CompressedCoin::V1(coin) => coin.tx_pointer = tx_pointer,
            CompressedCoin::V2(coin) => coin.tx_pointer = tx_pointer,
        }
    }

    /// Verifies the integrity of the coin.
    ///
    /// Returns `None`, if the `input` is a different type of input.
    /// Otherwise, returns the result of the field comparison.
    pub fn matches_input(&self, input: &Input) -> Option<bool> {
        tracing::debug!("Checking if CompressedCoin matches Input: {:?}", input);
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
                _ => None,
            },
            Input::DataCoinSigned(DataCoinSigned {
                owner,
                amount,
                asset_id,
                data,
                ..
            })
            | Input::DataCoinPredicate(DataCoinPredicate {
                owner,
                amount,
                asset_id,
                data,
                ..
            }) => match self {
                CompressedCoin::V2(coin) => Some(
                    owner == &coin.owner
                        && amount == &coin.amount
                        && asset_id == &coin.asset_id
                        && data == &coin.data,
                ),
                _ => {
                    tracing::debug!(
                        "Invalid type for CompressedCoin: expected V2 for DataCoin but found V1. \
                  ");
                    None
                }
            },
            _ => None,
        }
    }
}
