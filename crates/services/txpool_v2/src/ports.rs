use std::sync::Arc;

use fuel_core_services::stream::BoxStream;
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
        Transaction,
        TxId,
        UtxoId,
    },
    fuel_types::Nonce,
    fuel_vm::interpreter::Memory,
    services::{
        block_importer::SharedImportResult,
        p2p::{
            GossipsubMessageAcceptance,
            GossipsubMessageInfo,
            NetworkData,
            PeerId,
        },
    },
    tai64::Tai64,
};

use crate::{
    error::Error,
    GasPrice,
};

pub use fuel_core_storage::transactional::AtomicView;

pub trait BlockImporter: Send + Sync {
    /// Wait until the next block is available
    fn block_events(&self) -> BoxStream<SharedImportResult>;
}

/// Trait for getting the latest consensus parameters.
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

#[async_trait::async_trait]
pub trait P2P: Send + Sync {
    type GossipedTransaction: NetworkData<Transaction>;

    // Gossip broadcast a transaction inserted via API.
    fn broadcast_transaction(&self, transaction: Arc<Transaction>) -> anyhow::Result<()>;

    /// Creates a stream that is filled with the peer_id when they subscribe to
    /// our transactions gossip.
    fn subscribe_new_peers(&self) -> BoxStream<PeerId>;

    /// Creates a stream of next transactions gossiped from the network.
    fn gossiped_transaction_events(&self) -> BoxStream<Self::GossipedTransaction>;

    // Report the validity of a transaction received from the network.
    fn notify_gossip_transaction_validity(
        &self,
        message_info: GossipsubMessageInfo,
        validity: GossipsubMessageAcceptance,
    ) -> anyhow::Result<()>;

    // Asks the network to gather all tx ids of a specific peer
    async fn request_tx_ids(&self, peer_id: PeerId) -> anyhow::Result<Vec<TxId>>;

    // Asks the network to gather specific transactions from a specific peer
    async fn request_txs(
        &self,
        peer_id: PeerId,
        tx_ids: Vec<TxId>,
    ) -> anyhow::Result<Vec<Option<Transaction>>>;
}
