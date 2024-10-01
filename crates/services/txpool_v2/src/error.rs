use fuel_core_types::{
    fuel_tx::{
        Address,
        BlobId,
        ContractId,
        TxId,
        UtxoId,
    },
    fuel_types::Nonce,
    fuel_vm::checked_transaction::CheckError,
};

use crate::ports::WasmValidityError;

#[derive(Debug, derive_more::Display)]
pub enum Error {
    #[display(fmt = "Gas price not found for block height {_0}")]
    GasPriceNotFound(String),
    #[display(fmt = "Database error: {_0}")]
    Database(String),
    #[display(fmt = "Storage error: {_0}")]
    Storage(String),
    #[display(fmt = "Blacklisted error: {_0}")]
    Blacklisted(BlacklistedError),
    #[display(fmt = "Transaction collided: {_0}")]
    Collided(CollisionReason),
    #[display(fmt = "Transaction input validation failed: {_0}")]
    InputValidation(InputValidationError),
    #[display(fmt = "Transaction dependency error: {_0}")]
    Dependency(DependencyError),
    #[display(fmt = "Invalid transaction data: {_0:?}")]
    ConsensusValidity(CheckError),
    #[display(fmt = "Error with Wasm validity: {:?}", _0)]
    WasmValidity(WasmValidityError),
    #[display(fmt = "Mint transaction is not allowed")]
    MintIsDisallowed,
    #[display(fmt = "Pool limit is hit, try to increase gas_price")]
    NotInsertedLimitHit,
    #[display(fmt = "Transaction is removed: {_0}")]
    Removed(RemovedReason),
    #[display(fmt = "Too much transactions are in queue to be inserted. Can't add more")]
    TooManyQueuedTransactions,
}

#[derive(Debug, derive_more::Display)]
pub enum RemovedReason {
    #[display(
        fmt = "Transaction was removed because it was less worth than a new one (id: {_0}) that has been inserted"
    )]
    LessWorth(TxId),
    #[display(
        fmt = "Transaction expired because it exceeded the configured time to live `tx-pool-ttl`."
    )]
    Ttl,
}

#[derive(Debug, derive_more::Display)]
pub enum BlacklistedError {
    #[display(fmt = "The UTXO `{_0}` is blacklisted")]
    BlacklistedUTXO(UtxoId),
    #[display(fmt = "The owner `{_0}` is blacklisted")]
    BlacklistedOwner(Address),
    #[display(fmt = "The contract `{_0}` is blacklisted")]
    BlacklistedContract(ContractId),
    #[display(fmt = "The message `{_0}` is blacklisted")]
    BlacklistedMessage(Nonce),
}

#[derive(Debug, derive_more::Display)]
pub enum DependencyError {
    #[display(fmt = "Collision is also a dependency")]
    NotInsertedCollisionIsDependency,
    #[display(fmt = "Transaction chain dependency is already too big")]
    NotInsertedChainDependencyTooBig,
    #[display(fmt = "The dependent transaction creates a diamond problem, \
    causing cycles in the dependency graph.")]
    DependentTransactionIsADiamondDeath,
}

#[derive(Debug, derive_more::Display)]
pub enum InputValidationError {
    #[display(
        fmt = "Input output mismatch. Coin owner is different from expected input"
    )]
    NotInsertedIoWrongOwner,
    #[display(fmt = "Input output mismatch. Coin output does not match expected input")]
    NotInsertedIoWrongAmount,
    #[display(
        fmt = "Input output mismatch. Coin output asset_id does not match expected inputs"
    )]
    NotInsertedIoWrongAssetId,
    #[display(fmt = "Input message does not match the values from database")]
    NotInsertedIoMessageMismatch,
    #[display(fmt = "Input output mismatch. Expected coin but output is contract")]
    NotInsertedIoContractOutput,
    #[display(
        fmt = "Message id {_0:#x} does not match any received message from the DA layer."
    )]
    NotInsertedInputMessageUnknown(Nonce),
    #[display(fmt = "Input dependent on a Change or Variable output")]
    NotInsertedInputDependentOnChangeOrVariable,
    #[display(fmt = "UTXO input does not exist: {_0:#x}")]
    NotInsertedInputContractDoesNotExist(ContractId),
    #[display(fmt = "BlobId is already taken {_0:#x}")]
    NotInsertedBlobIdAlreadyTaken(BlobId),
    #[display(fmt = "Input coin does not match the values from database")]
    NotInsertedIoCoinMismatch,
    #[display(fmt = "Wrong number of outputs: {_0}")]
    WrongOutputNumber(String),
    #[display(fmt = "UTXO (id: {_0}) does not exist")]
    UtxoNotFound(UtxoId),
    #[display(fmt = "Max gas can't be 0")]
    MaxGasZero,
    #[display(fmt = "Transaction id already exists (id: {_0})")]
    DuplicateTxId(TxId),
}

#[derive(Debug, Clone, derive_more::Display)]
pub enum CollisionReason {
    #[display(
        fmt = "Transaction with the same UTXO (id: {_0}) already exists and is more worth it"
    )]
    Utxo(UtxoId),
    #[display(
        fmt = "Transaction that create the same contract (id: {_0}) already exists and is more worth it"
    )]
    ContractCreation(ContractId),
    #[display(
        fmt = "Transaction that use the same blob (id: {_0}) already exists and is more worth it"
    )]
    Blob(BlobId),
    #[display(
        fmt = "Transaction that use the same message (id: {_0}) already exists and is more worth it"
    )]
    Message(Nonce),
    #[display(fmt = "This transaction have an unknown collision")]
    Unknown,
    #[display(
        fmt = "This transaction have dependencies and is colliding with multiple transactions"
    )]
    MultipleCollisions,
}

impl From<CheckError> for Error {
    fn from(e: CheckError) -> Self {
        Error::ConsensusValidity(e)
    }
}
