//! The module containing all types related to the preconfirmation service.

use crate::fuel_tx::{
    Receipt,
    TxId,
    TxPointer,
    UtxoId,
};
use fuel_vm_private::fuel_tx::Output;
use tai64::Tai64;

#[cfg(not(feature = "std"))]
use alloc::{
    format,
    string::String,
    sync::Arc,
    vec::Vec,
};
#[cfg(feature = "std")]
use std::sync::Arc;

/// A collection of pre-confirmations that have been signed by a delegate
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Preconfirmations {
    /// The expiration time of the key used to sign
    pub expiration: Tai64,
    /// The transactions which have been pre-confirmed
    pub preconfirmations: Vec<Preconfirmation>,
}

/// A pre-confirmation is a message that is sent by the block producer to give the _final_
/// status of a transaction
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Preconfirmation {
    /// The ID of the transaction that is being pre-confirmed
    pub tx_id: TxId,
    /// The status of the transaction that is being pre-confirmed
    pub status: PreconfirmationStatus,
}

/// Transaction was squeezed out by the tx pool
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SqueezedOut {
    /// Reason the transaction was squeezed out
    reason: String,
}

impl SqueezedOut {
    /// Creates a new SqueezedOut
    pub fn new(reason: String, tx_id: TxId) -> Self {
        Self {
            reason: format!("{reason} TxId: {tx_id}"),
        }
    }

    /// Returns why the transaction was squeezed out.
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

/// Status of a transaction that has been pre-confirmed by block producer
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PreconfirmationStatus {
    /// Transaction was squeezed out by the tx pool
    SqueezedOut(SqueezedOut),
    /// Transaction has been confirmed and will be included in block_height
    Success {
        /// Transaction pointer within the block.
        tx_pointer: TxPointer,
        /// The total gas used by the transaction.
        total_gas: u64,
        /// The total fee paid by the transaction.
        total_fee: u64,
        /// Receipts produced by the transaction during execution.
        receipts: Arc<Vec<Receipt>>,
        /// Dynamic outputs produced by the transaction during execution.
        outputs: Vec<(UtxoId, Output)>,
    },
    /// Transaction will not be included in a block, rejected at `block_height`
    Failure {
        /// Transaction pointer within the block.
        tx_pointer: TxPointer,
        /// The total gas used by the transaction.
        total_gas: u64,
        /// The total fee paid by the transaction.
        total_fee: u64,
        /// Receipts produced by the transaction during execution.
        receipts: Arc<Vec<Receipt>>,
        /// Dynamic outputs produced by the transaction during execution.
        outputs: Vec<(UtxoId, Output)>,
    },
}
