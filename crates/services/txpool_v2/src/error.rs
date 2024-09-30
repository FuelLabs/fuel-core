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
    #[display(fmt = "Transaction not found: {_0}")]
    TransactionNotFound(String),
    #[display(fmt = "Wrong number of outputs: {_0}")]
    WrongOutputNumber(String),
    #[display(
        fmt = "Transaction is not inserted. Input coin does not match the values from database"
    )]
    NotInsertedIoCoinMismatch,
    #[display(
        fmt = "Transaction is not inserted. Transaction chain dependency is already too big"
    )]
    NotInsertedChainDependencyTooBig,
    #[display(fmt = "Transaction collided: {_0}")]
    Collided(CollisionReason),
    #[display(fmt = "Transaction is not inserted. Collision is also a dependency")]
    NotInsertedCollisionIsDependency,
    #[display(fmt = "The dependent transaction creates a diamond problem, \
        causing cycles in the dependency graph.")]
    DependentTransactionIsADiamondDeath,
    #[display(fmt = "Utxo not found: {_0}")]
    UtxoNotFound(UtxoId),
    #[display(fmt = "The UTXO `{_0}` is blacklisted")]
    BlacklistedUTXO(UtxoId),
    #[display(fmt = "The owner `{_0}` is blacklisted")]
    BlacklistedOwner(Address),
    #[display(fmt = "The contract `{_0}` is blacklisted")]
    BlacklistedContract(ContractId),
    #[display(fmt = "The message `{_0}` is blacklisted")]
    BlacklistedMessage(Nonce),
    #[display(fmt = "The transaction type is not supported")]
    NotSupportedTransactionType,
    #[display(fmt = "Invalid transaction data: {_0:?}")]
    ConsensusValidity(CheckError),
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
        fmt = "Transaction is not inserted. Input message does not match the values from database"
    )]
    NotInsertedIoMessageMismatch,
    #[display(
        fmt = "Transaction is not inserted. Input output mismatch. Expected coin but output is contract"
    )]
    NotInsertedIoContractOutput,
    #[display(
        fmt = "Transaction is not inserted. Message id {_0:#x} does not match any received message from the DA layer."
    )]
    NotInsertedInputMessageUnknown(Nonce),
    #[display(
        fmt = "Transaction is not inserted. Input dependent on a Change or Variable output"
    )]
    NotInsertedInputDependentOnChangeOrVariable,
    #[display(fmt = "Transaction is not inserted. UTXO input does not exist: {_0:#x}")]
    NotInsertedInputContractDoesNotExist(ContractId),
    #[display(fmt = "Transaction is not inserted. BlobId is already taken {_0:#x}")]
    NotInsertedBlobIdAlreadyTaken(BlobId),
    #[display(
        fmt = "Transaction is not inserted. Pool limit is hit, try to increase gas_price"
    )]
    NotInsertedLimitHit,
    #[display(fmt = "Storage error: {_0}")]
    Storage(String),
    #[display(fmt = "Error with Wasm validity: {:?}", _0)]
    WasmValidity(WasmValidityError),
    #[display(fmt = "Transaction is not inserted. Mint transaction is not allowed")]
    MintIsDisallowed,
    #[display(fmt = "Don't allow transactions with zero max gas")]
    ZeroMaxGas,
    #[display(fmt = "Transaction with the same tx_id(_0) already exists in the pool")]
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
