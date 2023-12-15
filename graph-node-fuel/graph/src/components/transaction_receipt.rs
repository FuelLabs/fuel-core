//! Code for retrieving transaction receipts from the database.
//!
//! This module exposes the [`LightTransactionReceipt`] type, which holds basic information about
//! the retrieved transaction receipts.

use web3::types::{TransactionReceipt, H256, U256, U64};

/// Like web3::types::Receipt, but with fewer fields.
#[derive(Debug, PartialEq, Eq)]
pub struct LightTransactionReceipt {
    pub transaction_hash: H256,
    pub transaction_index: U64,
    pub block_hash: Option<H256>,
    pub block_number: Option<U64>,
    pub gas_used: Option<U256>,
    pub status: Option<U64>,
}

impl From<TransactionReceipt> for LightTransactionReceipt {
    fn from(receipt: TransactionReceipt) -> Self {
        let TransactionReceipt {
            transaction_hash,
            transaction_index,
            block_hash,
            block_number,
            gas_used,
            status,
            ..
        } = receipt;
        LightTransactionReceipt {
            transaction_hash,
            transaction_index,
            block_hash,
            block_number,
            gas_used,
            status,
        }
    }
}
