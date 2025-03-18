//! Types for interoperability with the txpool service

use crate::{
    blockchain::header::ConsensusParametersVersion,
    fuel_asm::Word,
    fuel_tx::{
        field::{
            Inputs,
            Outputs,
            ScriptGasLimit,
            Tip,
        },
        Blob,
        Cacheable,
        Chargeable,
        Create,
        Input,
        Output,
        Receipt,
        Script,
        Transaction,
        TxId,
        TxPointer,
        Upgrade,
        Upload,
    },
    fuel_vm::{
        checked_transaction::Checked,
        ProgramState,
    },
    services::preconfirmation::PreconfirmationStatus,
};
use fuel_vm_private::{
    checked_transaction::CheckedTransaction,
    fuel_types::BlockHeight,
    prelude::field::Expiration,
};
use std::sync::Arc;
use tai64::Tai64;

use super::executor::TransactionExecutionResult;

/// Pool transaction wrapped in an Arc for thread-safe sharing
pub type ArcPoolTx = Arc<PoolTransaction>;

/// Metadata for the transaction pool.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Metadata {
    version: ConsensusParametersVersion,
    size: Option<usize>,
    max_gas_price: Word,
    #[cfg(feature = "test-helpers")]
    max_gas: Option<Word>,
    #[cfg(feature = "test-helpers")]
    tx_id: Option<TxId>,
}

impl Metadata {
    /// Create a new metadata for the transaction from the pool.
    pub fn new(
        version: ConsensusParametersVersion,
        size: usize,
        max_gas_price: Word,
    ) -> Self {
        Self {
            version,
            size: Some(size),
            max_gas_price,
            #[cfg(feature = "test-helpers")]
            max_gas: None,
            #[cfg(feature = "test-helpers")]
            tx_id: None,
        }
    }

    /// Create a new test metadata for the transaction from the pool.
    #[cfg(feature = "test-helpers")]
    pub fn new_test(
        version: ConsensusParametersVersion,
        max_gas: Option<u64>,
        tx_id: Option<TxId>,
    ) -> Self {
        Self {
            version,
            size: None,
            max_gas_price: 0,
            max_gas,
            tx_id,
        }
    }

    /// Returns the max gas price for the transaction.
    pub fn max_gas_price(&self) -> Word {
        self.max_gas_price
    }
}

/// Transaction type used by the transaction pool.
/// Not all `fuel_tx::Transaction` variants are supported by the txpool.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum PoolTransaction {
    /// Script
    Script(Checked<Script>, Metadata),
    /// Create
    Create(Checked<Create>, Metadata),
    /// Upgrade
    Upgrade(Checked<Upgrade>, Metadata),
    /// Upload
    Upload(Checked<Upload>, Metadata),
    /// Blob
    Blob(Checked<Blob>, Metadata),
}

impl PoolTransaction {
    /// Returns the version of the consensus parameters used to create `Checked` transaction.
    pub fn used_consensus_parameters_version(&self) -> ConsensusParametersVersion {
        self.metadata_inner().version
    }

    /// Used for accounting purposes when charging byte based fees.
    pub fn metered_bytes_size(&self) -> usize {
        if let Some(size) = self.metadata_inner().size {
            size
        } else {
            match self {
                PoolTransaction::Script(tx, _) => tx.transaction().metered_bytes_size(),
                PoolTransaction::Create(tx, _) => tx.transaction().metered_bytes_size(),
                PoolTransaction::Upgrade(tx, _) => tx.transaction().metered_bytes_size(),
                PoolTransaction::Upload(tx, _) => tx.transaction().metered_bytes_size(),
                PoolTransaction::Blob(tx, _) => tx.transaction().metered_bytes_size(),
            }
        }
    }

    /// Returns the maximum gas price for the transaction.
    pub fn max_gas_price(&self) -> Word {
        match self {
            PoolTransaction::Script(_, metadata) => metadata.max_gas_price,
            PoolTransaction::Create(_, metadata) => metadata.max_gas_price,
            PoolTransaction::Upgrade(_, metadata) => metadata.max_gas_price,
            PoolTransaction::Upload(_, metadata) => metadata.max_gas_price,
            PoolTransaction::Blob(_, metadata) => metadata.max_gas_price,
        }
    }

    /// Returns the expiration block for a transaction.
    pub fn expiration(&self) -> BlockHeight {
        match self {
            PoolTransaction::Script(tx, _) => tx.transaction().expiration(),
            PoolTransaction::Create(tx, _) => tx.transaction().expiration(),
            PoolTransaction::Upgrade(tx, _) => tx.transaction().expiration(),
            PoolTransaction::Upload(tx, _) => tx.transaction().expiration(),
            PoolTransaction::Blob(tx, _) => tx.transaction().expiration(),
        }
    }

    #[cfg(feature = "test-helpers")]
    fn id_inner(&self) -> TxId {
        match self {
            PoolTransaction::Script(tx, _) => tx.id(),
            PoolTransaction::Create(tx, _) => tx.id(),
            PoolTransaction::Upgrade(tx, _) => tx.id(),
            PoolTransaction::Upload(tx, _) => tx.id(),
            PoolTransaction::Blob(tx, _) => tx.id(),
        }
    }

    #[cfg(feature = "test-helpers")]
    fn max_gas_inner(&self) -> Word {
        match self {
            PoolTransaction::Script(tx, _) => tx.metadata().max_gas,
            PoolTransaction::Create(tx, _) => tx.metadata().max_gas,
            PoolTransaction::Upgrade(tx, _) => tx.metadata().max_gas,
            PoolTransaction::Upload(tx, _) => tx.metadata().max_gas,
            PoolTransaction::Blob(tx, _) => tx.metadata().max_gas,
        }
    }

    fn metadata_inner(&self) -> &Metadata {
        match self {
            PoolTransaction::Script(_, metadata) => metadata,
            PoolTransaction::Create(_, metadata) => metadata,
            PoolTransaction::Upgrade(_, metadata) => metadata,
            PoolTransaction::Upload(_, metadata) => metadata,
            PoolTransaction::Blob(_, metadata) => metadata,
        }
    }

    /// Returns the transaction ID
    #[cfg(not(feature = "test-helpers"))]
    pub fn id(&self) -> TxId {
        match self {
            PoolTransaction::Script(tx, _) => tx.id(),
            PoolTransaction::Create(tx, _) => tx.id(),
            PoolTransaction::Upgrade(tx, _) => tx.id(),
            PoolTransaction::Upload(tx, _) => tx.id(),
            PoolTransaction::Blob(tx, _) => tx.id(),
        }
    }

    /// Returns the transaction ID
    #[cfg(feature = "test-helpers")]
    pub fn id(&self) -> TxId {
        if let Some(tx_id) = self.metadata_inner().tx_id {
            tx_id
        } else {
            self.id_inner()
        }
    }

    /// Returns the maximum amount of gas that the transaction can consume.
    #[cfg(not(feature = "test-helpers"))]
    pub fn max_gas(&self) -> Word {
        match self {
            PoolTransaction::Script(tx, _) => tx.metadata().max_gas,
            PoolTransaction::Create(tx, _) => tx.metadata().max_gas,
            PoolTransaction::Upgrade(tx, _) => tx.metadata().max_gas,
            PoolTransaction::Upload(tx, _) => tx.metadata().max_gas,
            PoolTransaction::Blob(tx, _) => tx.metadata().max_gas,
        }
    }

    /// Returns the maximum amount of gas that the transaction can consume.
    #[cfg(feature = "test-helpers")]
    pub fn max_gas(&self) -> Word {
        if let Some(max_gas) = self.metadata_inner().max_gas {
            max_gas
        } else {
            self.max_gas_inner()
        }
    }
}

#[allow(missing_docs)]
impl PoolTransaction {
    pub fn script_gas_limit(&self) -> Option<Word> {
        match self {
            PoolTransaction::Script(script, _) => {
                Some(*script.transaction().script_gas_limit())
            }
            PoolTransaction::Create(_, _) => None,
            PoolTransaction::Upgrade(_, _) => None,
            PoolTransaction::Upload(_, _) => None,
            PoolTransaction::Blob(_, _) => None,
        }
    }

    pub fn tip(&self) -> Word {
        match self {
            Self::Script(tx, _) => tx.transaction().tip(),
            Self::Create(tx, _) => tx.transaction().tip(),
            Self::Upload(tx, _) => tx.transaction().tip(),
            Self::Upgrade(tx, _) => tx.transaction().tip(),
            Self::Blob(tx, _) => tx.transaction().tip(),
        }
    }

    pub fn is_computed(&self) -> bool {
        match self {
            PoolTransaction::Script(tx, _) => tx.transaction().is_computed(),
            PoolTransaction::Create(tx, _) => tx.transaction().is_computed(),
            PoolTransaction::Upgrade(tx, _) => tx.transaction().is_computed(),
            PoolTransaction::Upload(tx, _) => tx.transaction().is_computed(),
            PoolTransaction::Blob(tx, _) => tx.transaction().is_computed(),
        }
    }

    pub fn inputs(&self) -> &Vec<Input> {
        match self {
            PoolTransaction::Script(tx, _) => tx.transaction().inputs(),
            PoolTransaction::Create(tx, _) => tx.transaction().inputs(),
            PoolTransaction::Upgrade(tx, _) => tx.transaction().inputs(),
            PoolTransaction::Upload(tx, _) => tx.transaction().inputs(),
            PoolTransaction::Blob(tx, _) => tx.transaction().inputs(),
        }
    }

    pub fn outputs(&self) -> &Vec<Output> {
        match self {
            PoolTransaction::Script(tx, _) => tx.transaction().outputs(),
            PoolTransaction::Create(tx, _) => tx.transaction().outputs(),
            PoolTransaction::Upgrade(tx, _) => tx.transaction().outputs(),
            PoolTransaction::Upload(tx, _) => tx.transaction().outputs(),
            PoolTransaction::Blob(tx, _) => tx.transaction().outputs(),
        }
    }
}

impl From<PoolTransaction> for CheckedTransaction {
    fn from(tx: PoolTransaction) -> Self {
        match tx {
            PoolTransaction::Script(tx, _) => CheckedTransaction::Script(tx),
            PoolTransaction::Create(tx, _) => CheckedTransaction::Create(tx),
            PoolTransaction::Upgrade(tx, _) => CheckedTransaction::Upgrade(tx),
            PoolTransaction::Upload(tx, _) => CheckedTransaction::Upload(tx),
            PoolTransaction::Blob(tx, _) => CheckedTransaction::Blob(tx),
        }
    }
}

impl From<&PoolTransaction> for Transaction {
    fn from(tx: &PoolTransaction) -> Self {
        match tx {
            PoolTransaction::Script(tx, _) => {
                Transaction::Script(tx.transaction().clone())
            }
            PoolTransaction::Create(tx, _) => {
                Transaction::Create(tx.transaction().clone())
            }
            PoolTransaction::Upgrade(tx, _) => {
                Transaction::Upgrade(tx.transaction().clone())
            }
            PoolTransaction::Upload(tx, _) => {
                Transaction::Upload(tx.transaction().clone())
            }
            PoolTransaction::Blob(tx, _) => Transaction::Blob(tx.transaction().clone()),
        }
    }
}

impl From<&PoolTransaction> for CheckedTransaction {
    fn from(tx: &PoolTransaction) -> Self {
        match tx {
            PoolTransaction::Script(tx, _) => CheckedTransaction::Script(tx.clone()),
            PoolTransaction::Create(tx, _) => CheckedTransaction::Create(tx.clone()),
            PoolTransaction::Upgrade(tx, _) => CheckedTransaction::Upgrade(tx.clone()),
            PoolTransaction::Upload(tx, _) => CheckedTransaction::Upload(tx.clone()),
            PoolTransaction::Blob(tx, _) => CheckedTransaction::Blob(tx.clone()),
        }
    }
}

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
            } => TransactionStatus::Failure(
                statuses::Failure {
                    reason: TransactionExecutionResult::reason(&receipts, &result),
                    block_height,
                    block_timestamp: time,
                    program_state: result,
                    receipts,
                    total_gas,
                    total_fee,
                }
                .into(),
            ),
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
                    outputs: Some(outputs),
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
                        outputs: Some(outputs),
                        reason,
                    }
                    .into(),
                )
            }
        }
    }
}

/// The status of the transaction during its lifecycle.
pub mod statuses {
    use super::*;

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
                receipts: vec![],
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
        pub outputs: Option<Vec<Output>>,
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
                receipts: vec![],
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
        pub outputs: Option<Vec<Output>>,
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
                outputs: None,
                reason: "Dummy reason".to_string(),
            }
        }
    }
}
