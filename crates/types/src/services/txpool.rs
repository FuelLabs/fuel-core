//! Types for interoperability with the txpool service

use crate::{
    blockchain::block::Block,
    fuel_asm::Word,
    fuel_tx::{
        field::{
            Inputs,
            Outputs,
            ScriptGasLimit,
        },
        Cacheable,
        Chargeable,
        Create,
        Input,
        Output,
        Receipt,
        Script,
        Transaction,
        TxId,
        UtxoId,
    },
    fuel_types::{
        ContractId,
        Nonce,
    },
    fuel_vm::{
        checked_transaction::Checked,
        ProgramState,
    },
    services::executor::TransactionExecutionResult,
};
use fuel_vm_private::{
    checked_transaction::{
        CheckError,
        CheckedTransaction,
    },
    fuel_types::BlockHeight,
};
use std::{
    sync::Arc,
    time::Duration,
};
use tai64::Tai64;

/// The alias for transaction pool result.
pub type Result<T> = core::result::Result<T, Error>;
/// Pool transaction wrapped in an Arc for thread-safe sharing
pub type ArcPoolTx = Arc<PoolTransaction>;

/// Transaction type used by the transaction pool. Transaction pool supports not
/// all `fuel_tx::Transaction` variants.
#[derive(Debug, Eq, PartialEq)]
pub enum PoolTransaction {
    /// Script
    Script(Checked<Script>),
    /// Create
    Create(Checked<Create>),
}

impl PoolTransaction {
    /// Returns the gas price.
    pub fn price(&self) -> Word {
        match self {
            PoolTransaction::Script(script) => script.transaction().price(),
            PoolTransaction::Create(create) => create.transaction().price(),
        }
    }

    /// Returns the maximum amount of gas that the transaction can consume.
    pub fn max_gas(&self) -> Word {
        match self {
            PoolTransaction::Script(script) => script.metadata().fee.max_gas(),
            PoolTransaction::Create(create) => create.metadata().fee.max_gas(),
        }
    }

    /// Used for accounting purposes when charging byte based fees.
    pub fn metered_bytes_size(&self) -> usize {
        match self {
            PoolTransaction::Script(script) => script.transaction().metered_bytes_size(),
            PoolTransaction::Create(create) => create.transaction().metered_bytes_size(),
        }
    }

    /// Returns the transaction ID
    pub fn id(&self) -> TxId {
        match self {
            PoolTransaction::Script(script) => script.id(),
            PoolTransaction::Create(create) => create.id(),
        }
    }
}

#[allow(missing_docs)]
impl PoolTransaction {
    pub fn script_gas_limit(&self) -> Option<Word> {
        match self {
            PoolTransaction::Script(script) => {
                Some(*script.transaction().script_gas_limit())
            }
            PoolTransaction::Create(_) => None,
        }
    }

    pub fn is_computed(&self) -> bool {
        match self {
            PoolTransaction::Script(script) => script.transaction().is_computed(),
            PoolTransaction::Create(create) => create.transaction().is_computed(),
        }
    }

    pub fn inputs(&self) -> &Vec<Input> {
        match self {
            PoolTransaction::Script(script) => script.transaction().inputs(),
            PoolTransaction::Create(create) => create.transaction().inputs(),
        }
    }

    pub fn outputs(&self) -> &Vec<Output> {
        match self {
            PoolTransaction::Script(script) => script.transaction().outputs(),
            PoolTransaction::Create(create) => create.transaction().outputs(),
        }
    }
}

impl From<&PoolTransaction> for Transaction {
    fn from(tx: &PoolTransaction) -> Self {
        match tx {
            PoolTransaction::Script(script) => {
                Transaction::Script(script.transaction().clone())
            }
            PoolTransaction::Create(create) => {
                Transaction::Create(create.transaction().clone())
            }
        }
    }
}

impl From<&PoolTransaction> for CheckedTransaction {
    fn from(tx: &PoolTransaction) -> Self {
        match tx {
            PoolTransaction::Script(script) => CheckedTransaction::Script(script.clone()),
            PoolTransaction::Create(create) => CheckedTransaction::Create(create.clone()),
        }
    }
}

impl From<Checked<Script>> for PoolTransaction {
    fn from(checked: Checked<Script>) -> Self {
        Self::Script(checked)
    }
}

impl From<Checked<Create>> for PoolTransaction {
    fn from(checked: Checked<Create>) -> Self {
        Self::Create(checked)
    }
}

/// The `removed` field contains the list of removed transactions during the insertion
/// of the `inserted` transaction.
#[derive(Debug)]
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
    },
    /// Transaction was squeezed of the txpool
    SqueezedOut {
        /// Why this happened
        reason: String,
    },
    /// Transaction was included in a block, but the exection was reverted
    Failed {
        /// Included in this block
        block_height: BlockHeight,
        /// Time when the block was generated
        time: Tai64,
        /// Result of executing the transaction for scripts
        result: Option<ProgramState>,
        /// The receipts generated during execution of the transaction.
        receipts: Vec<Receipt>,
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
        TransactionExecutionResult::Success { result, receipts } => {
            TransactionStatus::Success {
                block_height,
                time,
                result,
                receipts,
            }
        }
        TransactionExecutionResult::Failed { result, receipts } => {
            TransactionStatus::Failed {
                block_height,
                time,
                result,
                receipts,
            }
        }
    }
}

#[allow(missing_docs)]
#[derive(thiserror::Error, Debug, Clone)]
#[non_exhaustive]
pub enum Error {
    #[error("TxPool required that transaction contains metadata")]
    NoMetadata,
    #[error("TxPool doesn't support this type of transaction.")]
    NotSupportedTransactionType,
    #[error("Transaction is not inserted. Hash is already known")]
    NotInsertedTxKnown,
    #[error("Transaction is not inserted. Pool limit is hit, try to increase gas_price")]
    NotInsertedLimitHit,
    #[error("Transaction is not inserted. The gas price is too low.")]
    NotInsertedGasPriceTooLow,
    #[error(
        "Transaction is not inserted. More priced tx {0:#x} already spend this UTXO output: {1:#x}"
    )]
    NotInsertedCollision(TxId, UtxoId),
    #[error(
        "Transaction is not inserted. More priced tx has created contract with ContractId {0:#x}"
    )]
    NotInsertedCollisionContractId(ContractId),
    #[error(
        "Transaction is not inserted. A higher priced tx {0:#x} is already spending this message: {1:#x}"
    )]
    NotInsertedCollisionMessageId(TxId, Nonce),
    #[error("Transaction is not inserted. UTXO input does not exist: {0:#x}")]
    NotInsertedOutputDoesNotExist(UtxoId),
    #[error("Transaction is not inserted. UTXO input contract does not exist or was already spent: {0:#x}")]
    NotInsertedInputContractDoesNotExist(ContractId),
    #[error("Transaction is not inserted. ContractId is already taken {0:#x}")]
    NotInsertedContractIdAlreadyTaken(ContractId),
    #[error("Transaction is not inserted. UTXO does not exist: {0:#x}")]
    NotInsertedInputUtxoIdNotDoesNotExist(UtxoId),
    #[error("Transaction is not inserted. UTXO is spent: {0:#x}")]
    NotInsertedInputUtxoIdSpent(UtxoId),
    #[error("Transaction is not inserted. Message is spent: {0:#x}")]
    NotInsertedInputMessageSpent(Nonce),
    #[error("Transaction is not inserted. Message id {0:#x} does not match any received message from the DA layer.")]
    NotInsertedInputMessageUnknown(Nonce),
    #[error(
        "Transaction is not inserted. UTXO requires Contract input {0:#x} that is priced lower"
    )]
    NotInsertedContractPricedLower(ContractId),
    #[error("Transaction is not inserted. Input coin mismatch the values from database")]
    NotInsertedIoCoinMismatch,
    #[error("Transaction is not inserted. Input output mismatch. Coin owner is different from expected input")]
    NotInsertedIoWrongOwner,
    #[error("Transaction is not inserted. Input output mismatch. Coin output does not match expected input")]
    NotInsertedIoWrongAmount,
    #[error("Transaction is not inserted. Input output mismatch. Coin output asset_id does not match expected inputs")]
    NotInsertedIoWrongAssetId,
    #[error(
        "Transaction is not inserted. Input message mismatch the values from database"
    )]
    NotInsertedIoMessageMismatch,
    #[error(
        "Transaction is not inserted. Input output mismatch. Expected coin but output is contract"
    )]
    NotInsertedIoContractOutput,
    #[error("Transaction is not inserted. Maximum depth of dependent transaction chain reached")]
    NotInsertedMaxDepth,
    #[error("Transaction exceeds the max gas per block limit. Tx gas: {tx_gas}, block limit {block_limit}")]
    NotInsertedMaxGasLimit { tx_gas: Word, block_limit: Word },
    // small todo for now it can pass but in future we should include better messages
    #[error("Transaction removed.")]
    Removed,
    #[error("Transaction expired because it exceeded the configured time to live `tx-pool-ttl`.")]
    TTLReason,
    #[error("Transaction squeezed out because {0}")]
    SqueezedOut(String),
    #[error("Invalid transaction data: {0:?}")]
    ConsensusValidity(CheckError),
    #[error("Mint transactions are disallowed from the txpool")]
    MintIsDisallowed,
    #[error("Database error: {0}")]
    Database(String),
    // TODO: We need it for now until channels are removed from TxPool.
    #[error("Got some unexpected error: {0}")]
    Other(String),
}

impl From<CheckError> for Error {
    fn from(e: CheckError) -> Self {
        Error::ConsensusValidity(e)
    }
}
