//! The module containing all types related to the preconfirmation service.

#[cfg(feature = "std")]
use std::sync::Arc;

use crate::fuel_tx::TxId;
use tai64::Tai64;

#[cfg(not(feature = "std"))]
use alloc::{
    string::String,
    vec::Vec,
};

use super::txpool;
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

#[cfg(feature = "std")]
/// Status of a transaction that has been pre-confirmed by block producer
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PreconfirmationStatus {
    /// Transaction was squeezed out by the tx pool
    SqueezedOut(Arc<txpool::statuses::PreConfirmationSqueezedOut>),
    /// Transaction has been confirmed and will be included in block_height
    Success(Arc<txpool::statuses::PreConfirmationSuccess>),
    /// Transaction will not be included in a block, rejected at `block_height`
    Failure(Arc<txpool::statuses::PreConfirmationFailure>),
}

#[cfg(not(feature = "std"))]
/// Status of a transaction that has been pre-confirmed by block producer
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PreconfirmationStatus {
    /// Transaction was squeezed out by the tx pool
    SqueezedOut(txpool::statuses::PreConfirmationSqueezedOut),
    /// Transaction has been confirmed and will be included in block_height
    Success(txpool::statuses::PreConfirmationSuccess),
    /// Transaction will not be included in a block, rejected at `block_height`
    Failure(txpool::statuses::PreConfirmationFailure),
}