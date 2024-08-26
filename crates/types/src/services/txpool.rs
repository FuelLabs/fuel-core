//! Types for interoperability with the txpool service

use crate::{
    blockchain::{
        block::Block,
        header::ConsensusParametersVersion,
    },
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
        Upgrade,
        Upload,
    },
    fuel_vm::{
        checked_transaction::Checked,
        ProgramState,
    },
    services::executor::TransactionExecutionResult,
};
use core::time::Duration;
use fuel_vm_private::{
    checked_transaction::CheckedTransaction,
    fuel_types::BlockHeight,
};
use std::sync::Arc;
use tai64::Tai64;

/// Pool transaction wrapped in an Arc for thread-safe sharing
pub type ArcPoolTx = Arc<PoolTransaction>;

/// Transaction type used by the transaction pool.
/// Not all `fuel_tx::Transaction` variants are supported by the txpool.
#[derive(Debug, Eq, PartialEq)]
pub enum PoolTransaction {
    /// Script
    Script(Checked<Script>, ConsensusParametersVersion),
    /// Create
    Create(Checked<Create>, ConsensusParametersVersion),
    /// Upgrade
    Upgrade(Checked<Upgrade>, ConsensusParametersVersion),
    /// Upload
    Upload(Checked<Upload>, ConsensusParametersVersion),
    /// Blob
    Blob(Checked<Blob>, ConsensusParametersVersion),
}

impl PoolTransaction {
    /// Returns the version of the consensus parameters used to create `Checked` transaction.
    pub fn used_consensus_parameters_version(&self) -> ConsensusParametersVersion {
        match self {
            PoolTransaction::Script(_, version)
            | PoolTransaction::Create(_, version)
            | PoolTransaction::Upgrade(_, version)
            | PoolTransaction::Upload(_, version)
            | PoolTransaction::Blob(_, version) => *version,
        }
    }

    /// Used for accounting purposes when charging byte based fees.
    pub fn metered_bytes_size(&self) -> usize {
        match self {
            PoolTransaction::Script(tx, _) => tx.transaction().metered_bytes_size(),
            PoolTransaction::Create(tx, _) => tx.transaction().metered_bytes_size(),
            PoolTransaction::Upgrade(tx, _) => tx.transaction().metered_bytes_size(),
            PoolTransaction::Upload(tx, _) => tx.transaction().metered_bytes_size(),
            PoolTransaction::Blob(tx, _) => tx.transaction().metered_bytes_size(),
        }
    }

    /// Returns the transaction ID
    pub fn id(&self) -> TxId {
        match self {
            PoolTransaction::Script(tx, _) => tx.id(),
            PoolTransaction::Create(tx, _) => tx.id(),
            PoolTransaction::Upgrade(tx, _) => tx.id(),
            PoolTransaction::Upload(tx, _) => tx.id(),
            PoolTransaction::Blob(tx, _) => tx.id(),
        }
    }

    /// Returns the maximum amount of gas that the transaction can consume.
    pub fn max_gas(&self) -> Word {
        match self {
            PoolTransaction::Script(tx, _) => tx.metadata().max_gas,
            PoolTransaction::Create(tx, _) => tx.metadata().max_gas,
            PoolTransaction::Upgrade(tx, _) => tx.metadata().max_gas,
            PoolTransaction::Upload(tx, _) => tx.metadata().max_gas,
            PoolTransaction::Blob(tx, _) => tx.metadata().max_gas,
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

/// The `removed` field contains the list of removed transactions during the insertion
/// of the `inserted` transaction.
#[derive(Debug, PartialEq, Eq)]
pub struct InsertionResult {
    /// This was inserted
    pub inserted: ArcPoolTx,
    /// The time the transaction was inserted.
    pub submitted_time: Duration,
    /// These were removed during the insertion
    pub removed: Vec<ArcPoolTx>,
}

/// The status of the transaction during its life from the tx pool until the block.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransactionStatus {
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

/// Converts the transaction execution result to the transaction status.
pub fn from_executor_to_status(
    block: &Block,
    result: TransactionExecutionResult,
) -> TransactionStatus {
    let time = block.header().time();
    let block_height = *block.header().height();
    match result {
        TransactionExecutionResult::Success {
            result,
            receipts,
            total_gas,
            total_fee,
        } => TransactionStatus::Success {
            block_height,
            time,
            result,
            receipts,
            total_gas,
            total_fee,
        },
        TransactionExecutionResult::Failed {
            result,
            receipts,
            total_gas,
            total_fee,
        } => TransactionStatus::Failed {
            block_height,
            time,
            result,
            receipts,
            total_gas,
            total_fee,
        },
    }
}
