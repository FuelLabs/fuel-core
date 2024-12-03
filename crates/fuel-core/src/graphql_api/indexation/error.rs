use fuel_core_storage::Error as StorageError;

use fuel_core_types::{
    fuel_tx::{
        Address,
        AssetId,
        UtxoId,
    },
    fuel_types::Nonce,
};

#[derive(derive_more::From, derive_more::Display, Debug)]
pub enum IndexationError {
    #[display(
        fmt = "Coin balance would underflow for owner: {}, asset_id: {}, current_amount: {}, requested_deduction: {}",
        owner,
        asset_id,
        current_amount,
        requested_deduction
    )]
    CoinBalanceWouldUnderflow {
        owner: Address,
        asset_id: AssetId,
        current_amount: u128,
        requested_deduction: u128,
    },
    #[display(
        fmt = "Message balance would underflow for owner: {}, current_amount: {}, requested_deduction: {}, retryable: {}",
        owner,
        current_amount,
        requested_deduction,
        retryable
    )]
    MessageBalanceWouldUnderflow {
        owner: Address,
        current_amount: u128,
        requested_deduction: u128,
        retryable: bool,
    },
    #[display(
        fmt = "Coin not found in coins to spend index for owner: {}, asset_id: {}, amount: {}, utxo_id: {}",
        owner,
        asset_id,
        amount,
        utxo_id
    )]
    CoinToSpendNotFound {
        owner: Address,
        asset_id: AssetId,
        amount: u64,
        utxo_id: UtxoId,
    },
    #[display(
        fmt = "Coin already in the coins to spend index for owner: {}, asset_id: {}, amount: {}, utxo_id: {}",
        owner,
        asset_id,
        amount,
        utxo_id
    )]
    CoinToSpendAlreadyIndexed {
        owner: Address,
        asset_id: AssetId,
        amount: u64,
        utxo_id: UtxoId,
    },
    #[display(
        fmt = "Message not found in coins to spend index for owner: {}, amount: {}, nonce: {}",
        owner,
        amount,
        nonce
    )]
    MessageToSpendNotFound {
        owner: Address,
        amount: u64,
        nonce: Nonce,
    },
    #[display(
        fmt = "Message already in the coins to spend index for owner: {}, amount: {}, nonce: {}",
        owner,
        amount,
        nonce
    )]
    MessageToSpendAlreadyIndexed {
        owner: Address,
        amount: u64,
        nonce: Nonce,
    },
    #[display(fmt = "Invalid coin type encountered in the index: {}", coin_type)]
    InvalidIndexedCoinType { coin_type: u8 },
    #[from]
    StorageError(StorageError),
}

impl std::error::Error for IndexationError {}

impl From<IndexationError> for StorageError {
    fn from(error: IndexationError) -> Self {
        match error {
            IndexationError::StorageError(e) => e,
            e => StorageError::Other(anyhow::anyhow!(e)),
        }
    }
}
