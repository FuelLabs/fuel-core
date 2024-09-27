use std::sync::Arc;

use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    blockchain::header::ConsensusParametersVersion,
    entities::{
        coins::coin::CompressedCoin,
        relayer::message::Message,
    },
    fuel_tx::{
        BlobId,
        Bytes32,
        ConsensusParameters,
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

pub use fuel_core_storage::transactional::AtomicView;

/// Trait for getting the latest consensus parameters.
#[cfg_attr(feature = "test-helpers", mockall::automock)]
pub trait ConsensusParametersProvider {
    /// Get latest consensus parameters.
    fn latest_consensus_parameters(
        &self,
    ) -> (ConsensusParametersVersion, Arc<ConsensusParameters>);
}

pub trait TxPoolPersistentStorage: Send + Sync {
    /// Get the UTXO by its ID.
    fn utxo(&self, utxo_id: &UtxoId) -> StorageResult<Option<CompressedCoin>>;

    /// Check if the contract with the given ID exists.
    fn contract_exist(&self, contract_id: &ContractId) -> StorageResult<bool>;

    /// Check if the blob with the given ID exists.
    fn blob_exist(&self, blob_id: &BlobId) -> StorageResult<bool>;

    /// Get the message by its ID.
    fn message(&self, message_id: &Nonce) -> StorageResult<Option<Message>>;
}

#[async_trait::async_trait]
/// Trait for getting gas price for the Tx Pool code to look up the gas price for a given block height
pub trait GasPriceProvider {
    /// Calculate gas price for the next block.
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
