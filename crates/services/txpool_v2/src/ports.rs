use std::sync::Arc;

use fuel_core_services::stream::BoxStream;
use fuel_core_storage::{
    PredicateStorageRequirements,
    Result as StorageResult,
};
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
    services::{
        block_importer::SharedImportResult,
        p2p::{
            GossipsubMessageAcceptance,
            GossipsubMessageInfo,
            NetworkData,
            PeerId,
        },
    },
};

use crate::{
    error::Error,
    GasPrice,
};

pub use fuel_core_storage::transactional::AtomicView;

pub trait BlockImporter {
    /// Wait until the next block is available
    fn block_events(&self) -> BoxStream<SharedImportResult>;
}

/// Trait for getting the latest consensus parameters.
pub trait ConsensusParametersProvider: Send + Sync + 'static {
    /// Get latest consensus parameters.
    fn latest_consensus_parameters(
        &self,
    ) -> (ConsensusParametersVersion, Arc<ConsensusParameters>);
}

pub trait TxPoolPersistentStorage:
    Clone + PredicateStorageRequirements + Send + Sync + 'static
{
    /// Get the UTXO by its ID.
    fn utxo(&self, utxo_id: &UtxoId) -> StorageResult<Option<CompressedCoin>>;

    /// Check if the contract with the given ID exists.
    fn contract_exist(&self, contract_id: &ContractId) -> StorageResult<bool>;

    /// Check if the blob with the given ID exists.
    fn blob_exist(&self, blob_id: &BlobId) -> StorageResult<bool>;

    /// Get the message by its ID.
    fn message(&self, message_id: &Nonce) -> StorageResult<Option<Message>>;
}

/// Trait for getting gas price for the Tx Pool code to look up the gas price for a given block height
pub trait GasPriceProvider: Send + Sync + 'static {
    /// Calculate gas price for the next block.
    fn next_gas_price(&self) -> GasPrice;
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

pub trait WasmChecker: Send + Sync + 'static {
    fn validate_uploaded_wasm(
        &self,
        wasm_root: &Bytes32,
    ) -> Result<(), WasmValidityError>;
}

pub trait P2PSubscriptions {
    type GossipedTransaction: NetworkData<Transaction>;

    /// Creates a stream that is filled with the peer_id when they subscribe to
    /// our transactions gossip.
    fn subscribe_new_peers(&self) -> BoxStream<PeerId>;

    /// Creates a stream of next transactions gossiped from the network.
    fn gossiped_transaction_events(&self) -> BoxStream<Self::GossipedTransaction>;
}

pub trait NotifyP2P {
    /// Gossip broadcast a transaction inserted via API.
    fn broadcast_transaction(&self, transaction: Arc<Transaction>) -> anyhow::Result<()>;

    /// Report the validity of a transaction received from the network.
    fn notify_gossip_transaction_validity(
        &self,
        message_info: GossipsubMessageInfo,
        validity: GossipsubMessageAcceptance,
    ) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
pub trait P2PRequests: NotifyP2P + Send + Sync + 'static {
    /// Asks the network to gather all tx ids of a specific peer
    async fn request_tx_ids(&self, peer_id: PeerId) -> anyhow::Result<Vec<TxId>>;

    /// Asks the network to gather specific transactions from a specific peer
    async fn request_txs(
        &self,
        peer_id: PeerId,
        tx_ids: Vec<TxId>,
    ) -> anyhow::Result<Vec<Option<Transaction>>>;
}
