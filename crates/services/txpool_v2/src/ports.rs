use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    entities::{
        coins::coin::CompressedCoin,
        relayer::message::Message,
    },
    fuel_tx::{
        BlobId,
        Bytes32,
        ContractId,
        UtxoId,
    },
    fuel_types::Nonce,
    tai64::Tai64,
};

pub trait TxPoolDb: Send + Sync {
    fn utxo(&self, utxo_id: &UtxoId) -> StorageResult<Option<CompressedCoin>>;

    fn contract_exist(&self, contract_id: &ContractId) -> StorageResult<bool>;

    fn blob_exist(&self, blob_id: &BlobId) -> StorageResult<bool>;

    fn message(&self, message_id: &Nonce) -> StorageResult<Option<Message>>;
}

pub trait GetTime: Send + Sync {
    fn now(&self) -> Tai64;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WasmValidityError {
    /// Wasm support is not enabled.
    NotEnabled,
    /// The supposedly-uploaded wasm was not found.
    NotFound,
    /// The uploaded bytecode was found but it's is not valid wasm.
    NotValid,
}

pub trait WasmChecker {
    fn validate_uploaded_wasm(
        &self,
        wasm_root: &Bytes32,
    ) -> Result<(), WasmValidityError>;
}
