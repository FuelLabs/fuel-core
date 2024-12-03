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

#[cfg(test)]
mod tests {
    use super::IndexationError;

    impl PartialEq for IndexationError {
        fn eq(&self, other: &Self) -> bool {
            match (self, other) {
                (
                    Self::CoinBalanceWouldUnderflow {
                        owner: l_owner,
                        asset_id: l_asset_id,
                        current_amount: l_current_amount,
                        requested_deduction: l_requested_deduction,
                    },
                    Self::CoinBalanceWouldUnderflow {
                        owner: r_owner,
                        asset_id: r_asset_id,
                        current_amount: r_current_amount,
                        requested_deduction: r_requested_deduction,
                    },
                ) => {
                    l_owner == r_owner
                        && l_asset_id == r_asset_id
                        && l_current_amount == r_current_amount
                        && l_requested_deduction == r_requested_deduction
                }
                (
                    Self::MessageBalanceWouldUnderflow {
                        owner: l_owner,
                        current_amount: l_current_amount,
                        requested_deduction: l_requested_deduction,
                        retryable: l_retryable,
                    },
                    Self::MessageBalanceWouldUnderflow {
                        owner: r_owner,
                        current_amount: r_current_amount,
                        requested_deduction: r_requested_deduction,
                        retryable: r_retryable,
                    },
                ) => {
                    l_owner == r_owner
                        && l_current_amount == r_current_amount
                        && l_requested_deduction == r_requested_deduction
                        && l_retryable == r_retryable
                }
                (
                    Self::CoinToSpendAlreadyIndexed {
                        owner: l_owner,
                        asset_id: l_asset_id,
                        amount: l_amount,
                        utxo_id: l_utxo_id,
                    },
                    Self::CoinToSpendAlreadyIndexed {
                        owner: r_owner,
                        asset_id: r_asset_id,
                        amount: r_amount,
                        utxo_id: r_utxo_id,
                    },
                ) => {
                    l_owner == r_owner
                        && l_asset_id == r_asset_id
                        && l_amount == r_amount
                        && l_utxo_id == r_utxo_id
                }
                (
                    Self::MessageToSpendAlreadyIndexed {
                        owner: l_owner,
                        amount: l_amount,
                        nonce: l_nonce,
                    },
                    Self::MessageToSpendAlreadyIndexed {
                        owner: r_owner,
                        amount: r_amount,
                        nonce: r_nonce,
                    },
                ) => l_owner == r_owner && l_amount == r_amount && l_nonce == r_nonce,
                (
                    Self::CoinToSpendNotFound {
                        owner: l_owner,
                        asset_id: l_asset_id,
                        amount: l_amount,
                        utxo_id: l_utxo_id,
                    },
                    Self::CoinToSpendNotFound {
                        owner: r_owner,
                        asset_id: r_asset_id,
                        amount: r_amount,
                        utxo_id: r_utxo_id,
                    },
                ) => {
                    l_owner == r_owner
                        && l_asset_id == r_asset_id
                        && l_amount == r_amount
                        && l_utxo_id == r_utxo_id
                }
                (
                    Self::MessageToSpendNotFound {
                        owner: l_owner,
                        amount: l_amount,
                        nonce: l_nonce,
                    },
                    Self::MessageToSpendNotFound {
                        owner: r_owner,
                        amount: r_amount,
                        nonce: r_nonce,
                    },
                ) => l_owner == r_owner && l_amount == r_amount && l_nonce == r_nonce,
                (Self::StorageError(l0), Self::StorageError(r0)) => l0 == r0,
                _ => panic!("comparison not expected"),
            }
        }
    }
}
