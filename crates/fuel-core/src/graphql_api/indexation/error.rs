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
    #[from]
    StorageError(StorageError),
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
                (Self::StorageError(l0), Self::StorageError(r0)) => l0 == r0,
                _ => false,
            }
        }
    }
}
