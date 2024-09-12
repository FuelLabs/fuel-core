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
    fuel_vm::interpreter::Memory,
    tai64::Tai64,
};

use crate::{
    error::Error,
    GasPrice,
};

/// Provides an atomic view of the storage at the latest height at
/// the moment of view instantiation. All modifications to the storage
/// after this point will not affect the view.
pub trait AtomicView: Send + Sync {
    /// The type of the latest storage view.
    type LatestView;

    /// Returns current the view of the storage.
    fn latest_view(&self) -> StorageResult<Self::LatestView>;
}

pub trait TxPoolPersistentStorage: Send + Sync {
    fn utxo(&self, utxo_id: &UtxoId) -> StorageResult<Option<CompressedCoin>>;

    fn contract_exist(&self, contract_id: &ContractId) -> StorageResult<bool>;

    fn blob_exist(&self, blob_id: &BlobId) -> StorageResult<bool>;

    fn message(&self, message_id: &Nonce) -> StorageResult<Option<Message>>;
}

#[async_trait::async_trait]
/// Trait for getting gas price for the Tx Pool code to look up the gas price for a given block height
pub trait GasPriceProvider {
    /// Calculate gas price for the next block with a given size `block_bytes`.
    async fn next_gas_price(&self) -> Result<GasPrice, Error>;
}

/// Trait for getting VM memory.
#[async_trait::async_trait]
pub trait MemoryPool {
    type Memory: Memory + Send + Sync + 'static;

    /// Get the memory instance.
    async fn get_memory(&self) -> Self::Memory;
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
    Validity,
}

pub trait WasmChecker {
    fn validate_uploaded_wasm(
        &self,
        wasm_root: &Bytes32,
    ) -> Result<(), WasmValidityError>;
}
