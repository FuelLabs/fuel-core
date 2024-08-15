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

pub type Result<T> = core::result::Result<T, Error>;

#[allow(missing_docs)]
#[derive(thiserror::Error, Debug, Clone)]
#[non_exhaustive]
pub enum Error {
    #[error("Gas price not found for block height {0}")]
    GasPriceNotFound(String),
    #[error("Failed to calculate max block bytes: {0}")]
    MaxBlockBytes(String),
    #[error("TxPool required that transaction contains metadata")]
    NoMetadata,
    #[error("TxPool doesn't support this type of transaction.")]
    NotSupportedTransactionType,
    #[error("Transaction is not inserted. Hash is already known")]
    NotInsertedTxKnown,
    #[error("Transaction is not inserted. Pool limit is hit, try to increase gas_price")]
    NotInsertedLimitHit,
    #[error("Transaction is not inserted. Attempting to upgrade WASM bytecode when WASM support is not enabled.")]
    NotInsertedWasmNotEnabled,
    #[error("Transaction is not inserted. WASM bytecode matching the given root was not found.")]
    NotInsertedWasmNotFound,
    #[error("Transaction is not inserted. WASM bytecode contents are not valid.")]
    NotInsertedInvalidWasm,
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
        "Transaction is not inserted. More priced tx has uploaded the blob with BlobId {0:#x}"
    )]
    NotInsertedCollisionBlobId(BlobId),
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
    #[error("Transaction is not inserted. BlobId is already taken {0:#x}")]
    NotInsertedBlobIdAlreadyTaken(BlobId),
    #[error("Transaction is not inserted. UTXO does not exist: {0:#x}")]
    NotInsertedInputUtxoIdNotDoesNotExist(UtxoId),
    #[error("Transaction is not inserted. UTXO is spent: {0:#x}")]
    NotInsertedInputUtxoIdSpent(UtxoId),
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
    #[error("The UTXO `{0}` is blacklisted")]
    BlacklistedUTXO(UtxoId),
    #[error("The owner `{0}` is blacklisted")]
    BlacklistedOwner(Address),
    #[error("The contract `{0}` is blacklisted")]
    BlacklistedContract(ContractId),
    #[error("The message `{0}` is blacklisted")]
    BlacklistedMessage(Nonce),
    #[error("Database error: {0}")]
    Database(String),
    // TODO: We need it for now until channels are removed from TxPool.
    #[error("Got some unexpected error: {0}")]
    Other(String),
}

#[cfg(feature = "test-helpers")]
impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        self.to_string().eq(&other.to_string())
    }
}

impl From<CheckError> for Error {
    fn from(e: CheckError) -> Self {
        Error::ConsensusValidity(e)
    }
}
