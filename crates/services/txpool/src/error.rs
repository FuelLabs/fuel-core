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

pub type Result<T> = core::result::Result<T, Error>;

#[allow(missing_docs)]
#[derive(Debug, Clone, derive_more::Display)]
#[non_exhaustive]
pub enum Error {
    #[display(fmt = "Gas price not found for block height {_0}")]
    GasPriceNotFound(String),
    #[display(fmt = "Failed to calculate max block bytes: {_0}")]
    MaxBlockBytes(String),
    #[display(fmt = "TxPool requires that the transaction contains metadata")]
    NoMetadata,
    #[display(fmt = "TxPool doesn't support this type of transaction.")]
    NotSupportedTransactionType,
    #[display(fmt = "Transaction is not inserted. Hash is already known")]
    NotInsertedTxKnown,
    #[display(
        fmt = "Transaction is not inserted. Pool limit is hit, try to increase gas_price"
    )]
    NotInsertedLimitHit,
    #[display(
        fmt = "Transaction is not inserted. Attempting to upgrade WASM bytecode when WASM support is not enabled."
    )]
    NotInsertedWasmNotEnabled,
    #[display(
        fmt = "Transaction is not inserted. WASM bytecode matching the given root was not found."
    )]
    NotInsertedWasmNotFound,
    #[display(
        fmt = "Transaction is not inserted. WASM bytecode contents are not valid."
    )]
    NotInsertedInvalidWasm,
    #[display(fmt = "Transaction is not inserted. The gas price is too low.")]
    NotInsertedGasPriceTooLow,
    #[display(
        fmt = "Transaction is not inserted. Higher priced tx {_0:#x} has already spent this UTXO output: {_1:#x}"
    )]
    NotInsertedCollision(TxId, UtxoId),
    #[display(
        fmt = "Transaction is not inserted. Higher priced tx has created contract with ContractId {_0:#x}"
    )]
    NotInsertedCollisionContractId(ContractId),
    #[display(
        fmt = "Transaction is not inserted. Higher priced tx has uploaded the blob with BlobId {_0:#x}"
    )]
    NotInsertedCollisionBlobId(BlobId),
    #[display(
        fmt = "Transaction is not inserted. A higher priced tx {_0:#x} is already spending this message: {_1:#x}"
    )]
    NotInsertedCollisionMessageId(TxId, Nonce),
    #[display(fmt = "Transaction is not inserted. UTXO input does not exist: {_0:#x}")]
    NotInsertedInputContractDoesNotExist(ContractId),
    #[display(fmt = "Transaction is not inserted. ContractId is already taken {_0:#x}")]
    NotInsertedContractIdAlreadyTaken(ContractId),
    #[display(fmt = "Transaction is not inserted. BlobId is already taken {_0:#x}")]
    NotInsertedBlobIdAlreadyTaken(BlobId),
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
        fmt = "Transaction is not inserted. Input coin does not match the values from database"
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
        fmt = "Transaction is not inserted. Input message does not match the values from database"
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

impl std::error::Error for Error {}

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

impl From<WasmValidityError> for Error {
    fn from(err: WasmValidityError) -> Self {
        match err {
            WasmValidityError::NotEnabled => Error::NotInsertedWasmNotEnabled,
            WasmValidityError::NotFound => Error::NotInsertedWasmNotFound,
            WasmValidityError::NotValid => Error::NotInsertedInvalidWasm,
        }
    }
}
