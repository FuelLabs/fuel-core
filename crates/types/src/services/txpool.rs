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
        UtxoId,
    },
    fuel_types::{
        Address,
        ContractId,
        Nonce,
    },
    fuel_vm::{
        checked_transaction::Checked,
        ProgramState,
    },
    services::executor::TransactionExecutionResult,
};
use core::time::Duration;
use fuel_vm_private::{
    checked_transaction::{
        CheckError,
        CheckedTransaction,
    },
    fuel_types::BlockHeight,
};
use std::sync::Arc;
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
    Script(Checked<Script>, ConsensusParametersVersion),
    /// Create
    Create(Checked<Create>, ConsensusParametersVersion),
    /// Upgrade
    Upgrade(Checked<Upgrade>, ConsensusParametersVersion),
    /// Upload
    Upload(Checked<Upload>, ConsensusParametersVersion),
}

impl PoolTransaction {
    /// Returns the version of the consensus parameters used to create `Checked` transaction.
    pub fn used_consensus_parameters_version(&self) -> ConsensusParametersVersion {
        match self {
            PoolTransaction::Script(_, version)
            | PoolTransaction::Create(_, version)
            | PoolTransaction::Upgrade(_, version)
            | PoolTransaction::Upload(_, version) => *version,
        }
    }

    /// Used for accounting purposes when charging byte based fees.
    pub fn metered_bytes_size(&self) -> usize {
        match self {
            PoolTransaction::Script(tx, _) => tx.transaction().metered_bytes_size(),
            PoolTransaction::Create(tx, _) => tx.transaction().metered_bytes_size(),
            PoolTransaction::Upgrade(tx, _) => tx.transaction().metered_bytes_size(),
            PoolTransaction::Upload(tx, _) => tx.transaction().metered_bytes_size(),
        }
    }

    /// Returns the transaction ID
    pub fn id(&self) -> TxId {
        match self {
            PoolTransaction::Script(tx, _) => tx.id(),
            PoolTransaction::Create(tx, _) => tx.id(),
            PoolTransaction::Upgrade(tx, _) => tx.id(),
            PoolTransaction::Upload(tx, _) => tx.id(),
        }
    }

    /// Returns the maximum amount of gas that the transaction can consume.
    pub fn max_gas(&self) -> Word {
        match self {
            PoolTransaction::Script(tx, _) => tx.metadata().max_gas,
            PoolTransaction::Create(tx, _) => tx.metadata().max_gas,
            PoolTransaction::Upgrade(tx, _) => tx.metadata().max_gas,
            PoolTransaction::Upload(tx, _) => tx.metadata().max_gas,
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
        }
    }

    pub fn tip(&self) -> Word {
        match self {
            Self::Script(tx, _) => tx.transaction().tip(),
            Self::Create(tx, _) => tx.transaction().tip(),
            Self::Upload(tx, _) => tx.transaction().tip(),
            Self::Upgrade(tx, _) => tx.transaction().tip(),
        }
    }

    pub fn is_computed(&self) -> bool {
        match self {
            PoolTransaction::Script(tx, _) => tx.transaction().is_computed(),
            PoolTransaction::Create(tx, _) => tx.transaction().is_computed(),
            PoolTransaction::Upgrade(tx, _) => tx.transaction().is_computed(),
            PoolTransaction::Upload(tx, _) => tx.transaction().is_computed(),
        }
    }

    pub fn inputs(&self) -> &Vec<Input> {
        match self {
            PoolTransaction::Script(tx, _) => tx.transaction().inputs(),
            PoolTransaction::Create(tx, _) => tx.transaction().inputs(),
            PoolTransaction::Upgrade(tx, _) => tx.transaction().inputs(),
            PoolTransaction::Upload(tx, _) => tx.transaction().inputs(),
        }
    }

    pub fn outputs(&self) -> &Vec<Output> {
        match self {
            PoolTransaction::Script(tx, _) => tx.transaction().outputs(),
            PoolTransaction::Create(tx, _) => tx.transaction().outputs(),
            PoolTransaction::Upgrade(tx, _) => tx.transaction().outputs(),
            PoolTransaction::Upload(tx, _) => tx.transaction().outputs(),
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
        }
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

#[allow(missing_docs)]
#[derive(derive_more::Display, Debug, Clone)]
#[non_exhaustive]
pub enum Error {
    #[display(fmt = "Gas price not found for block height {_0}")]
    GasPriceNotFound(String),
    #[display(fmt = "Failed to calculate max block bytes: {_0}")]
    MaxBlockBytes(String),
    #[display(fmt = "TxPool required that transaction contains metadata")]
    NoMetadata,
    #[display(fmt = "TxPool doesn't support this type of transaction.")]
    NotSupportedTransactionType,
    #[display(fmt = "Transaction is not inserted. Hash is already known")]
    NotInsertedTxKnown,
    #[display(
        fmt = "Transaction is not inserted. Pool limit is hit, try to increase gas_price"
    )]
    NotInsertedLimitHit,
    #[display(fmt = "Transaction is not inserted. The gas price is too low.")]
    NotInsertedGasPriceTooLow,
    #[display(
        fmt = "Transaction is not inserted. More priced tx {_0:#x} already spend this UTXO output: {_1:#x}"
    )]
    NotInsertedCollision(TxId, UtxoId),
    #[display(
        fmt = "Transaction is not inserted. More priced tx has created contract with ContractId {_0:#x}"
    )]
    NotInsertedCollisionContractId(ContractId),
    #[display(
        fmt = "Transaction is not inserted. A higher priced tx {_0:#x} is already spending this message: {_1:#x}"
    )]
    NotInsertedCollisionMessageId(TxId, Nonce),
    #[display(fmt = "Transaction is not inserted. UTXO input does not exist: {_0:#x}")]
    NotInsertedOutputDoesNotExist(UtxoId),
    #[display(
        fmt = "Transaction is not inserted. UTXO input contract does not exist or was already spent: {_0:#x}"
    )]
    NotInsertedInputContractDoesNotExist(ContractId),
    #[display(fmt = "Transaction is not inserted. ContractId is already taken {_0:#x}")]
    NotInsertedContractIdAlreadyTaken(ContractId),
    #[display(fmt = "Transaction is not inserted. UTXO does not exist: {_0:#x}")]
    NotInsertedInputUtxoIdNotDoesNotExist(UtxoId),
    #[display(fmt = "Transaction is not inserted. UTXO is spent: {_0:#x}")]
    NotInsertedInputUtxoIdSpent(UtxoId),
    #[display(
        fmt = "Transaction is not inserted. Message id {_0:#x} does not match any received message from the DA layer."
    )]
    NotInsertedInputMessageUnknown(Nonce),
    #[display(
        fmt = "Transaction is not inserted. UTXO requires Contract input {_0:#x} that is priced lower"
    )]
    NotInsertedContractPricedLower(ContractId),
    #[display(
        fmt = "Transaction is not inserted. Input coin mismatch the values from database"
    )]
    NotInsertedIoCoinMismatch,
    #[display(
        fmt = "Transaction is not inserted. Input output mismatch. Coin owner is different from expected input"
    )]
    NotInsertedIoWrongOwner,
    #[display(
        fmt = "Transaction is not inserted. Input output mismatch. Coin output does not match expected input"
    )]
    NotInsertedIoWrongAmount,
    #[display(
        fmt = "Transaction is not inserted. Input output mismatch. Coin output asset_id does not match expected inputs"
    )]
    NotInsertedIoWrongAssetId,
    #[display(
        fmt = "Transaction is not inserted. Input message mismatch the values from database"
    )]
    NotInsertedIoMessageMismatch,
    #[display(
        fmt = "Transaction is not inserted. Input output mismatch. Expected coin but output is contract"
    )]
    NotInsertedIoContractOutput,
    #[display(
        fmt = "Transaction is not inserted. Maximum depth of dependent transaction chain reached"
    )]
    NotInsertedMaxDepth,
    // small todo for now it can pass but in future we should include better messages
    #[display(fmt = "Transaction removed.")]
    Removed,
    #[display(
        fmt = "Transaction expired because it exceeded the configured time to live `tx-pool-ttl`."
    )]
    TTLReason,
    #[display(fmt = "Transaction squeezed out because {_0}")]
    SqueezedOut(String),
    #[display(fmt = "Invalid transaction data: {_0:?}")]
    ConsensusValidity(CheckError),
    #[display(fmt = "Mint transactions are disallowed from the txpool")]
    MintIsDisallowed,
    #[display(fmt = "The UTXO `{_0}` is blacklisted")]
    BlacklistedUTXO(UtxoId),
    #[display(fmt = "The owner `{_0}` is blacklisted")]
    BlacklistedOwner(Address),
    #[display(fmt = "The contract `{_0}` is blacklisted")]
    BlacklistedContract(ContractId),
    #[display(fmt = "The message `{_0}` is blacklisted")]
    BlacklistedMessage(Nonce),
    #[display(fmt = "Database error: {_0}")]
    Database(String),
    // TODO: We need it for now until channels are removed from TxPool.
    #[display(fmt = "Got some unexpected error: {_0}")]
    Other(String),
}

impl From<CheckError> for Error {
    fn from(e: CheckError) -> Self {
        Error::ConsensusValidity(e)
    }
}
