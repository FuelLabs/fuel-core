use fuel_core_types::{
    fuel_tx::{
        Address,
        BlobId,
        ContractId,
        TxId,
        UtxoId,
        Word,
    },
    fuel_types::Nonce,
    fuel_vm::checked_transaction::CheckError,
};

use crate::ports::WasmValidityError;

#[derive(Clone, Debug, derive_more::Display)]
pub enum Error {
    #[display("Gas price not found for block height {_0}")]
    GasPriceNotFound(String),
    #[display("Database error: {_0}")]
    Database(String),
    #[display("Storage error: {_0}")]
    Storage(String),
    #[display("Blacklisted error: {_0}")]
    Blacklisted(BlacklistedError),
    #[display("Transaction collided: {_0}")]
    Collided(CollisionReason),
    #[display("Transaction input validation failed: {_0}")]
    InputValidation(InputValidationError),
    #[display("Transaction dependency error: {_0}")]
    Dependency(DependencyError),
    #[display("Invalid transaction data: {_0:?}")]
    ConsensusValidity(CheckError),
    #[display("Error with Wasm validity: {:?}", _0)]
    WasmValidity(WasmValidityError),
    #[display("Mint transaction is not allowed")]
    MintIsDisallowed,
    #[display("Pool limit is hit, try to increase gas_price")]
    NotInsertedLimitHit,
    #[display("Transaction is removed: {_0}")]
    Removed(RemovedReason),
    #[display("Transaction has been skipped during block insertion: {_0}")]
    SkippedTransaction(String),
    #[display("Too much transactions are in queue to be inserted. Can't add more")]
    TooManyQueuedTransactions,
    #[display("Unable send a request because service is closed")]
    ServiceCommunicationFailed,
    #[display("Request failed to be sent because service queue is full")]
    ServiceQueueFull,
    #[display(
        "The provided max fee can't cover the transaction cost. \
        The minimal gas price should be {minimal_gas_price:?}, \
        while it is {max_gas_price_from_fee:?}"
    )]
    InsufficientMaxFee {
        /// The max gas price from the fee.
        max_gas_price_from_fee: Word,
        /// The minimum gas price required by TxPool.
        minimal_gas_price: Word,
    },
}

#[derive(Clone, Debug, derive_more::Display)]
pub enum RemovedReason {
    #[display(
        "Transaction was removed because it was less worth than a new one (id: {_0}) that has been inserted"
    )]
    LessWorth(TxId),
    #[display(
        "Transaction expired because it exceeded the configured time to live `tx-pool-ttl`."
    )]
    Ttl,
}

#[derive(Clone, Debug, derive_more::Display)]
pub enum BlacklistedError {
    #[display("The UTXO `{_0}` is blacklisted")]
    BlacklistedUTXO(UtxoId),
    #[display("The owner `{_0}` is blacklisted")]
    BlacklistedOwner(Address),
    #[display("The contract `{_0}` is blacklisted")]
    BlacklistedContract(ContractId),
    #[display("The message `{_0}` is blacklisted")]
    BlacklistedMessage(Nonce),
}

#[derive(Clone, Debug, derive_more::Display)]
pub enum DependencyError {
    #[display("Collision is also a dependency")]
    NotInsertedCollisionIsDependency,
    #[display("Transaction chain dependency is already too big")]
    NotInsertedChainDependencyTooBig,
    #[display(
        "The dependent transaction creates a diamond problem, \
    causing cycles in the dependency graph."
    )]
    DependentTransactionIsADiamondDeath,
}

#[derive(Clone, Debug, derive_more::Display)]
pub enum InputValidationError {
    #[display("Input output mismatch. Coin owner is different from expected input")]
    NotInsertedIoWrongOwner,
    #[display("Input output mismatch. Coin output does not match expected input")]
    NotInsertedIoWrongAmount,
    #[display(
        "Input output mismatch. Coin output asset_id does not match expected inputs"
    )]
    NotInsertedIoWrongAssetId,
    #[display("Input message does not match the values from database")]
    NotInsertedIoMessageMismatch,
    #[display("Input output mismatch. Expected coin but output is contract")]
    NotInsertedIoContractOutput,
    #[display(
        "Message id {_0:#x} does not match any received message from the DA layer."
    )]
    NotInsertedInputMessageUnknown(Nonce),
    #[display("Input dependent on a Change or Variable output")]
    NotInsertedInputDependentOnChangeOrVariable,
    #[display("UTXO input does not exist: {_0:#x}")]
    NotInsertedInputContractDoesNotExist(ContractId),
    #[display("BlobId is already taken {_0:#x}")]
    NotInsertedBlobIdAlreadyTaken(BlobId),
    #[display("Input coin does not match the values from database")]
    NotInsertedIoCoinMismatch,
    #[display("Wrong number of outputs: {_0}")]
    WrongOutputNumber(String),
    #[display("UTXO (id: {_0}) does not exist")]
    UtxoNotFound(UtxoId),
    #[display("Max gas can't be 0")]
    MaxGasZero,
    #[display("Transaction id already exists (id: {_0})")]
    DuplicateTxId(TxId),
}

#[derive(Debug, Clone, derive_more::Display)]
pub enum CollisionReason {
    #[display(
        "Transaction with the same UTXO (id: {_0}) already exists and is more worth it"
    )]
    Utxo(UtxoId),
    #[display(
        "Transaction that create the same contract (id: {_0}) already exists and is more worth it"
    )]
    ContractCreation(ContractId),
    #[display(
        "Transaction that use the same blob (id: {_0}) already exists and is more worth it"
    )]
    Blob(BlobId),
    #[display(
        "Transaction that use the same message (id: {_0}) already exists and is more worth it"
    )]
    Message(Nonce),
    #[display("This transaction have an unknown collision")]
    Unknown,
    #[display(
        "This transaction have dependencies and is colliding with multiple transactions"
    )]
    MultipleCollisions,
}

impl From<CheckError> for Error {
    fn from(e: CheckError) -> Self {
        Error::ConsensusValidity(e)
    }
}
