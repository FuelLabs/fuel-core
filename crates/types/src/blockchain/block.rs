//! Types related to blocks

use super::{
    consensus::ConsensusType,
    header::{
        ApplicationHeader,
        BlockHeader,
        ConsensusHeader,
        PartialBlockHeader,
    },
    primitives::{
        BlockId,
        Empty,
    },
    transaction::TransactionValidityError,
};
use crate::{
    fuel_tx::{
        Transaction,
        UniqueIdentifier,
    },
    fuel_types::{
        bytes::SerializableVec,
        Bytes32,
        MessageId,
    },
    services::txpool::TransactionStatus,
};
use fuel_vm_private::prelude::{
    Backtrace,
    CheckError,
    ContractId,
    InterpreterError,
};
use std::error::Error as StdError;
use thiserror::Error;

/// Fuel block with all transaction data included
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(any(test, feature = "test-helpers"), derive(Default))]
pub struct Block<TransactionRepresentation = Transaction> {
    /// Generated complete header.
    header: BlockHeader,
    /// Executed transactions.
    transactions: Vec<TransactionRepresentation>,
}

/// Compressed version of the fuel `Block`.
pub type CompressedBlock = Block<Bytes32>;

/// Fuel block with all transaction data included
/// but without any data generated.
/// This type can be created with unexecuted
/// transactions to produce a [`Block`] or
/// it can be created with pre-executed transactions in
/// order to validate they were constructed correctly.
#[derive(Clone, Debug)]
pub struct PartialFuelBlock {
    /// The partial header.
    pub header: PartialBlockHeader,
    /// Transactions that can either be pre-executed
    /// or not.
    pub transactions: Vec<Transaction>,
}

impl Block<Transaction> {
    /// Create a new full fuel block from a [`PartialBlockHeader`],
    /// executed transactions and the [`MessageId`]s.
    ///
    /// The order of the transactions must be the same order they were
    /// executed in.
    /// The order of the messages must be the same as they were
    /// produced in.
    ///
    /// Message ids are produced by executed the transactions and collecting
    /// the ids from the receipts of messages outputs.
    pub fn new(
        header: PartialBlockHeader,
        mut transactions: Vec<Transaction>,
        message_ids: &[MessageId],
    ) -> Self {
        // I think this is safe as it doesn't appear that any of the reads actually mutate the data.
        // Alternatively we can clone to be safe.
        let transaction_ids: Vec<_> =
            transactions.iter_mut().map(|tx| tx.to_bytes()).collect();
        Self {
            header: header.generate(&transaction_ids[..], message_ids),
            transactions,
        }
    }

    /// Compresses the fuel block and replaces transactions with hashes.
    pub fn compress(&self) -> CompressedBlock {
        Block {
            header: self.header.clone(),
            transactions: self.transactions.iter().map(|tx| tx.id()).collect(),
        }
    }
}

impl<T> Block<T> {
    /// Destructure into the inner types.
    pub fn into_inner(self) -> (BlockHeader, Vec<T>) {
        (self.header, self.transactions)
    }
}

impl CompressedBlock {
    /// Convert from a compressed block back to a the full block.
    pub fn uncompress(self, transactions: Vec<Transaction>) -> Block<Transaction> {
        // TODO: should we perform an extra validation step to ensure the provided
        //  txs match the expected ones in the block?
        Block {
            header: self.header,
            transactions,
        }
    }
}

/// Starting point for executing a block.
/// Production starts with a [`PartialFuelBlock`].
/// Validation starts with a full [`FuelBlock`].
pub type ExecutionBlock = ExecutionTypes<PartialFuelBlock, Block>;

/// Execution wrapper with only a single type.
pub type ExecutionType<T> = ExecutionTypes<T, T>;

#[derive(Debug, Clone, Copy)]
/// Execution wrapper where the types
/// depend on the type of execution.
pub enum ExecutionTypes<P, V> {
    /// Production mode where P is being produced.
    Production(P),
    /// Validation mode where V is being checked.
    Validation(V),
}

/// The result of transactions execution.
#[derive(Debug)]
pub struct ExecutionResult {
    /// Created block during the execution of transactions. It contains only valid transactions.
    pub block: Block,
    /// The list of skipped transactions with corresponding errors. Those transactions were
    /// not included in the block and didn't affect the state of the blockchain.
    pub skipped_transactions: Vec<(Transaction, Error)>,
    /// The status of the transactions execution included into the block.
    pub tx_status: Vec<TransactionExecutionStatus>,
}

#[derive(Debug, Clone)]
/// The status of a transaction after it is executed.
pub struct TransactionExecutionStatus {
    /// The id of the transaction.
    pub id: Bytes32,
    /// The status of the executed transaction.
    pub status: TransactionStatus,
}

#[derive(Debug, Clone, Copy)]
/// The kind of execution.
pub enum ExecutionKind {
    /// Producing a block.
    Production,
    /// Validating a block.
    Validation,
}

impl<TransactionRepresentation> Block<TransactionRepresentation> {
    /// Get the hash of the header.
    pub fn id(&self) -> BlockId {
        self.header.id()
    }

    /// Get the executed transactions.
    pub fn transactions(&self) -> &[TransactionRepresentation] {
        &self.transactions[..]
    }

    /// Get the complete header.
    pub fn header(&self) -> &BlockHeader {
        &self.header
    }

    /// The type of consensus this header is using.
    pub fn consensus_type(&self) -> ConsensusType {
        self.header.consensus_type()
    }

    /// Get mutable access to transactions for testing purposes
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn transactions_mut(&mut self) -> &mut Vec<TransactionRepresentation> {
        &mut self.transactions
    }

    /// Get mutable access to header for testing purposes
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn header_mut(&mut self) -> &mut BlockHeader {
        &mut self.header
    }
}

impl PartialFuelBlock {
    /// Create a new block
    pub fn new(header: PartialBlockHeader, transactions: Vec<Transaction>) -> Self {
        Self {
            header,
            transactions,
        }
    }

    /// Generate a [`Block`] after running this partial block.
    ///
    /// The order of the messages must be the same as they were
    /// produced in.
    ///
    /// Message ids are produced by executed the transactions and collecting
    /// the ids from the receipts of messages outputs.
    pub fn generate(self, message_ids: &[MessageId]) -> Block {
        Block::new(self.header, self.transactions, message_ids)
    }
}

impl From<Block> for PartialFuelBlock {
    fn from(block: Block) -> Self {
        let Block {
            header:
                BlockHeader {
                    application: ApplicationHeader { da_height, .. },
                    consensus:
                        ConsensusHeader {
                            prev_root,
                            height,
                            time,
                            ..
                        },
                    ..
                },
            transactions,
        } = block;
        Self {
            header: PartialBlockHeader {
                application: ApplicationHeader {
                    da_height,
                    generated: Empty {},
                },
                consensus: ConsensusHeader {
                    prev_root,
                    height,
                    time,
                    generated: Empty {},
                },
                metadata: None,
            },
            transactions,
        }
    }
}

impl ExecutionBlock {
    /// Get the hash of the full [`FuelBlock`] if validating.
    pub fn id(&self) -> Option<BlockId> {
        match self {
            ExecutionTypes::Production(_) => None,
            ExecutionTypes::Validation(v) => Some(v.id()),
        }
    }

    /// Get the transaction root from the full [`FuelBlock`] if validating.
    pub fn txs_root(&self) -> Option<Bytes32> {
        match self {
            ExecutionTypes::Production(_) => None,
            ExecutionTypes::Validation(v) => Some(v.header().transactions_root),
        }
    }
}

impl<P, V> ExecutionTypes<P, V> {
    /// Map the production type if producing.
    pub fn map_p<Q, F>(self, f: F) -> ExecutionTypes<Q, V>
    where
        F: FnOnce(P) -> Q,
    {
        match self {
            ExecutionTypes::Production(p) => ExecutionTypes::Production(f(p)),
            ExecutionTypes::Validation(v) => ExecutionTypes::Validation(v),
        }
    }

    /// Map the validation type if validating.
    pub fn map_v<W, F>(self, f: F) -> ExecutionTypes<P, W>
    where
        F: FnOnce(V) -> W,
    {
        match self {
            ExecutionTypes::Production(p) => ExecutionTypes::Production(p),
            ExecutionTypes::Validation(v) => ExecutionTypes::Validation(f(v)),
        }
    }

    /// Get a reference version of the inner type.
    pub fn as_ref(&self) -> ExecutionTypes<&P, &V> {
        match *self {
            ExecutionTypes::Production(ref p) => ExecutionTypes::Production(p),
            ExecutionTypes::Validation(ref v) => ExecutionTypes::Validation(v),
        }
    }

    /// Get a mutable reference version of the inner type.
    pub fn as_mut(&mut self) -> ExecutionTypes<&mut P, &mut V> {
        match *self {
            ExecutionTypes::Production(ref mut p) => ExecutionTypes::Production(p),
            ExecutionTypes::Validation(ref mut v) => ExecutionTypes::Validation(v),
        }
    }

    /// Get the kind of execution.
    pub fn to_kind(&self) -> ExecutionKind {
        match self {
            ExecutionTypes::Production(_) => ExecutionKind::Production,
            ExecutionTypes::Validation(_) => ExecutionKind::Validation,
        }
    }
}

impl<T> ExecutionType<T> {
    /// Map the wrapped type.
    pub fn map<U, F>(self, f: F) -> ExecutionType<U>
    where
        F: FnOnce(T) -> U,
    {
        match self {
            ExecutionTypes::Production(p) => ExecutionTypes::Production(f(p)),
            ExecutionTypes::Validation(v) => ExecutionTypes::Validation(f(v)),
        }
    }

    /// Filter and map the inner type.
    pub fn filter_map<U, F>(self, f: F) -> Option<ExecutionType<U>>
    where
        F: FnOnce(T) -> Option<U>,
    {
        match self {
            ExecutionTypes::Production(p) => f(p).map(ExecutionTypes::Production),
            ExecutionTypes::Validation(v) => f(v).map(ExecutionTypes::Validation),
        }
    }

    /// Get the inner type.
    pub fn into_inner(self) -> T {
        match self {
            ExecutionTypes::Production(t) | ExecutionTypes::Validation(t) => t,
        }
    }

    /// Split into the execution kind and the inner type.
    pub fn split(self) -> (ExecutionKind, T) {
        let kind = self.to_kind();
        (kind, self.into_inner())
    }
}

impl ExecutionKind {
    /// Wrap a type in this execution kind.
    pub fn wrap<T>(self, t: T) -> ExecutionType<T> {
        match self {
            ExecutionKind::Production => ExecutionTypes::Production(t),
            ExecutionKind::Validation => ExecutionTypes::Validation(t),
        }
    }
}

impl<T> core::ops::Deref for ExecutionType<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            ExecutionTypes::Production(p) => p,
            ExecutionTypes::Validation(v) => v,
        }
    }
}

impl<T> core::ops::DerefMut for ExecutionType<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            ExecutionTypes::Production(p) => p,
            ExecutionTypes::Validation(v) => v,
        }
    }
}

#[allow(missing_docs)]
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("Transaction id was already used: {0:#x}")]
    TransactionIdCollision(Bytes32),
    #[error("output already exists")]
    OutputAlreadyExists,
    #[error("The computed fee caused an integer overflow")]
    FeeOverflow,
    #[error("Not supported transaction: {0:?}")]
    NotSupportedTransaction(Box<Transaction>),
    #[error("The first transaction in the block is not `Mint` - coinbase.")]
    CoinbaseIsNotFirstTransaction,
    #[error("Coinbase should have one output.")]
    CoinbaseSeveralOutputs,
    #[error("Coinbase outputs is invalid.")]
    CoinbaseOutputIsInvalid,
    #[error("Coinbase amount mismatches with expected.")]
    CoinbaseAmountMismatch,
    #[error("Invalid transaction: {0}")]
    TransactionValidity(#[from] TransactionValidityError),
    #[error("corrupted block state")]
    CorruptedBlockState(Box<dyn StdError + Send + Sync>),
    #[error("Transaction({transaction_id:#x}) execution error: {error:?}")]
    VmExecution {
        error: InterpreterError,
        transaction_id: Bytes32,
    },
    #[error(transparent)]
    InvalidTransaction(#[from] CheckError),
    #[error("Execution error with backtrace")]
    Backtrace(Box<Backtrace>),
    #[error("Transaction doesn't match expected result: {transaction_id:#x}")]
    InvalidTransactionOutcome { transaction_id: Bytes32 },
    #[error("Transaction root is invalid")]
    InvalidTransactionRoot,
    #[error("The amount of charged fees is invalid")]
    InvalidFeeAmount,
    #[error("Block id is invalid")]
    InvalidBlockId,
    #[error("No matching utxo for contract id ${0:#x}")]
    ContractUtxoMissing(ContractId),
}

impl From<Backtrace> for Error {
    fn from(e: Backtrace) -> Self {
        Error::Backtrace(Box::new(e))
    }
}
