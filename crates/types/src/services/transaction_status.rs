//! The status of the transaction during its lifecycle.

use crate::{
    fuel_tx::{
        Output,
        Receipt,
        TxPointer,
    },
    fuel_vm::ProgramState,
    services::{
        executor::TransactionExecutionResult,
        preconfirmation::PreconfirmationStatus,
    },
};
use fuel_vm_private::fuel_types::BlockHeight;
use tai64::Tai64;

#[cfg(feature = "std")]
use std::sync::Arc;

#[cfg(not(feature = "std"))]
use alloc::sync::Arc;

#[cfg(feature = "alloc")]
use alloc::{
    string::{
        String,
        ToString,
    },
    vec::Vec,
};

/// The status of the transaction during its life from the tx pool until the block.
// TODO: This type needs to be updated: https://github.com/FuelLabs/fuel-core/issues/2794
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransactionExecutionStatus {
    /// Transaction was submitted into the txpool
    Submitted {
        /// Timestamp of submission into the txpool
        time: Tai64,
    },
    /// Transaction was successfully included in a block
    Success {
        /// Included in this block
        block_height: BlockHeight,
        /// Time when the block was generated
        time: Tai64,
        /// Result of executing the transaction for scripts
        result: Option<ProgramState>,
        /// The receipts generated during execution of the transaction.
        receipts: Vec<Receipt>,
        /// The total gas used by the transaction.
        total_gas: u64,
        /// The total fee paid by the transaction.
        total_fee: u64,
    },
    /// Transaction was squeezed of the txpool
    SqueezedOut {
        /// Why this happened
        reason: String,
    },
    /// Transaction was included in a block, but the execution was reverted
    Failed {
        /// Included in this block
        block_height: BlockHeight,
        /// Time when the block was generated
        time: Tai64,
        /// Result of executing the transaction for scripts
        result: Option<ProgramState>,
        /// The receipts generated during execution of the transaction.
        receipts: Vec<Receipt>,
        /// The total gas used by the transaction.
        total_gas: u64,
        /// The total fee paid by the transaction.
        total_fee: u64,
    },
}

impl From<TransactionExecutionStatus> for TransactionStatus {
    fn from(value: TransactionExecutionStatus) -> Self {
        match value {
            TransactionExecutionStatus::Submitted { time } => {
                TransactionStatus::Submitted(
                    statuses::Submitted { timestamp: time }.into(),
                )
            }
            TransactionExecutionStatus::Success {
                block_height,
                time,
                result,
                receipts,
                total_gas,
                total_fee,
            } => TransactionStatus::Success(
                statuses::Success {
                    block_height,
                    block_timestamp: time,
                    program_state: result,
                    receipts,
                    total_gas,
                    total_fee,
                }
                .into(),
            ),
            // TODO: Removed this variant as part of the
            //  https://github.com/FuelLabs/fuel-core/issues/2794
            TransactionExecutionStatus::SqueezedOut { reason } => {
                TransactionStatus::SqueezedOut(statuses::SqueezedOut { reason }.into())
            }
            TransactionExecutionStatus::Failed {
                block_height,
                time,
                result,
                receipts,
                total_gas,
                total_fee,
            } => {
                #[cfg(feature = "std")]
                let reason = TransactionExecutionResult::reason(&receipts, &result);
                #[cfg(not(feature = "std"))]
                let reason = "Unknown reason".to_string();
                TransactionStatus::Failure(
                    statuses::Failure {
                        reason,
                        block_height,
                        block_timestamp: time,
                        program_state: result,
                        receipts,
                        total_gas,
                        total_fee,
                    }
                    .into(),
                )
            }
        }
    }
}

/// The status of the transaction during its life from the TxPool until the inclusion in the block.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransactionStatus {
    /// Transaction was submitted into the TxPool
    Submitted(Arc<statuses::Submitted>),
    /// Transaction was successfully included in a block
    Success(Arc<statuses::Success>),
    /// Transaction was successfully executed by block producer
    PreConfirmationSuccess(Arc<statuses::PreConfirmationSuccess>),
    /// Transaction was squeezed out of the TxPool
    SqueezedOut(Arc<statuses::SqueezedOut>),
    /// Transaction was squeezed out
    PreConfirmationSqueezedOut(Arc<statuses::PreConfirmationSqueezedOut>),
    /// Transaction was included in a block, but the execution has failed
    Failure(Arc<statuses::Failure>),
    /// Transaction was included in a block by block producer, but the execution has failed
    PreConfirmationFailure(Arc<statuses::PreConfirmationFailure>),
}

impl TransactionStatus {
    /// Returns `true` if the status is considered "final".
    pub fn is_final(&self) -> bool {
        match self {
            TransactionStatus::Success(_)
            | TransactionStatus::Failure(_)
            | TransactionStatus::SqueezedOut(_)
            | TransactionStatus::PreConfirmationSqueezedOut(_) => true,
            TransactionStatus::Submitted(_)
            | TransactionStatus::PreConfirmationSuccess(_)
            | TransactionStatus::PreConfirmationFailure(_) => false,
        }
    }

    /// Returns `true` if the status is pre confirmation.
    pub fn is_preconfirmation(&self) -> bool {
        match self {
            TransactionStatus::Submitted(_)
            | TransactionStatus::Success(_)
            | TransactionStatus::Failure(_)
            | TransactionStatus::SqueezedOut(_) => false,
            TransactionStatus::PreConfirmationSuccess(_)
            | TransactionStatus::PreConfirmationFailure(_)
            | TransactionStatus::PreConfirmationSqueezedOut(_) => true,
        }
    }

    /// Returns `true` if the status is `Submitted`.
    pub fn is_submitted(&self) -> bool {
        matches!(self, Self::Submitted { .. })
    }

    /// Creates a new `TransactionStatus::Submitted` variant.
    pub fn submitted(timestamp: Tai64) -> Self {
        Self::Submitted(statuses::Submitted { timestamp }.into())
    }

    /// Creates a new `TransactionStatus::SqueezedOut` variant.
    pub fn squeezed_out(reason: String) -> Self {
        Self::SqueezedOut(statuses::SqueezedOut { reason }.into())
    }

    /// Creates a new `TransactionStatus::PreConfirmationSqueezedOut` variant.
    pub fn preconfirmation_squeezed_out(reason: String) -> Self {
        Self::PreConfirmationSqueezedOut(
            statuses::PreConfirmationSqueezedOut { reason }.into(),
        )
    }
}

impl From<PreconfirmationStatus> for TransactionStatus {
    fn from(value: PreconfirmationStatus) -> Self {
        match value {
            PreconfirmationStatus::SqueezedOut { reason } => {
                TransactionStatus::PreConfirmationSqueezedOut(
                    statuses::PreConfirmationSqueezedOut { reason }.into(),
                )
            }
            PreconfirmationStatus::Success {
                tx_pointer,
                total_gas,
                total_fee,
                receipts,
                outputs,
            } => TransactionStatus::PreConfirmationSuccess(
                statuses::PreConfirmationSuccess {
                    tx_pointer,
                    total_gas,
                    total_fee,
                    receipts: Some(receipts),
                    resolved_outputs: Some(outputs),
                }
                .into(),
            ),
            PreconfirmationStatus::Failure {
                tx_pointer,
                total_gas,
                total_fee,
                receipts,
                outputs,
            } => {
                let reason = TransactionExecutionResult::reason(&receipts, &None);
                TransactionStatus::PreConfirmationFailure(
                    statuses::PreConfirmationFailure {
                        tx_pointer,
                        total_gas,
                        total_fee,
                        receipts: Some(receipts),
                        resolved_outputs: Some(outputs),
                        reason,
                    }
                    .into(),
                )
            }
        }
    }
}

/// The status of the transaction possible during the preconfirmation phase.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreConfirmationStatus {
    /// Transaction was successfully executed by block producer.
    Success(Arc<statuses::PreConfirmationSuccess>),
    /// Transaction was squeezed out by the block producer.
    SqueezedOut(Arc<statuses::PreConfirmationSqueezedOut>),
    /// Transaction was included in a block by block producer, but the execution has failed.
    Failure(Arc<statuses::PreConfirmationFailure>),
}

/// The status of the transaction during its lifecycle.
pub mod statuses {
    use super::*;
    use crate::fuel_tx::UtxoId;

    /// Transaction was submitted into the TxPool
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Submitted {
        /// Timestamp of submission into the TxPool
        pub timestamp: Tai64,
    }

    impl Default for Submitted {
        fn default() -> Self {
            Self {
                timestamp: Tai64::UNIX_EPOCH,
            }
        }
    }

    /// Transaction was successfully included in a block
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Success {
        /// Included in this block
        pub block_height: BlockHeight,
        /// Timestamp of the block
        pub block_timestamp: Tai64,
        /// Result of executing the transaction for scripts
        pub program_state: Option<ProgramState>,
        /// The receipts generated during execution of the transaction
        pub receipts: Vec<Receipt>,
        /// The total gas used by the transaction
        pub total_gas: u64,
        /// The total fee paid by the transaction
        pub total_fee: u64,
    }

    impl Default for Success {
        fn default() -> Self {
            Self {
                block_height: Default::default(),
                block_timestamp: Tai64::UNIX_EPOCH,
                program_state: None,
                receipts: Vec::new(),
                total_gas: 0,
                total_fee: 0,
            }
        }
    }

    /// Transaction was successfully executed by block producer
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    #[derive(Default, Clone, Debug, PartialEq, Eq)]
    pub struct PreConfirmationSuccess {
        /// Transaction pointer within the block.
        pub tx_pointer: TxPointer,
        /// The total gas used by the transaction.
        pub total_gas: u64,
        /// The total fee paid by the transaction.
        pub total_fee: u64,
        /// Receipts produced by the transaction during execution.
        pub receipts: Option<Vec<Receipt>>,
        /// Dynamic outputs produced by the transaction during execution.
        pub resolved_outputs: Option<Vec<(UtxoId, Output)>>,
    }

    /// Transaction was squeezed out of the TxPool
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct SqueezedOut {
        /// The reason why the transaction was squeezed out
        pub reason: String,
    }

    impl Default for SqueezedOut {
        fn default() -> Self {
            Self {
                reason: "Default reason".to_string(),
            }
        }
    }

    impl From<&PreConfirmationSqueezedOut> for SqueezedOut {
        fn from(value: &PreConfirmationSqueezedOut) -> Self {
            Self {
                reason: value.reason.clone(),
            }
        }
    }

    impl From<SqueezedOut> for TransactionStatus {
        fn from(value: SqueezedOut) -> Self {
            TransactionStatus::SqueezedOut(value.into())
        }
    }

    /// Transaction was squeezed out from block producer
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct PreConfirmationSqueezedOut {
        /// The reason why the transaction was squeezed out
        pub reason: String,
    }

    impl Default for PreConfirmationSqueezedOut {
        fn default() -> Self {
            Self {
                reason: "Default reason".to_string(),
            }
        }
    }

    /// Transaction was included in a block, but the execution has failed
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Failure {
        /// Included in this block
        pub block_height: BlockHeight,
        /// Timestamp of the block
        pub block_timestamp: Tai64,
        /// The reason why the transaction has failed
        pub reason: String,
        /// Result of executing the transaction for scripts
        pub program_state: Option<ProgramState>,
        /// The receipts generated during execution of the transaction
        pub receipts: Vec<Receipt>,
        /// The total gas used by the transaction
        pub total_gas: u64,
        /// The total fee paid by the transaction
        pub total_fee: u64,
    }

    impl Default for Failure {
        fn default() -> Self {
            Self {
                block_height: Default::default(),
                block_timestamp: Tai64::UNIX_EPOCH,
                reason: "Dummy reason".to_string(),
                program_state: None,
                receipts: Vec::new(),
                total_gas: 0,
                total_fee: 0,
            }
        }
    }

    /// Transaction was included in a block by block producer, but the execution has failed
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct PreConfirmationFailure {
        /// Transaction pointer within the block.
        pub tx_pointer: TxPointer,
        /// The total gas used by the transaction.
        pub total_gas: u64,
        /// The total fee paid by the transaction.
        pub total_fee: u64,
        /// Receipts produced by the transaction during execution.
        pub receipts: Option<Vec<Receipt>>,
        /// Dynamic outputs produced by the transaction during execution.
        pub resolved_outputs: Option<Vec<(UtxoId, Output)>>,
        /// The reason why the transaction has failed
        pub reason: String,
    }

    impl Default for PreConfirmationFailure {
        fn default() -> Self {
            Self {
                tx_pointer: Default::default(),
                total_gas: 0,
                total_fee: 0,
                receipts: None,
                resolved_outputs: None,
                reason: "Dummy reason".to_string(),
            }
        }
    }
}
