use crate::types::GasPrice;
use fuel_core_services::stream::BoxStream;
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    entities::{
        coins::coin::CompressedCoin,
        relayer::message::Message,
    },
    fuel_tx::{
        ConsensusParameters,
        Transaction,
        UtxoId,
    },
    fuel_types::{
        ContractId,
        Nonce,
    },
    fuel_vm::interpreter::Memory,
    services::{
        block_importer::SharedImportResult,
        p2p::{
            GossipsubMessageAcceptance,
            GossipsubMessageInfo,
            NetworkData,
        },
        txpool::Result as TxPoolResult,
    },
};
use std::{
    ops::Deref,
    sync::Arc,
};

pub trait PeerToPeer: Send + Sync {
    type GossipedTransaction: NetworkData<Transaction>;

    // Gossip broadcast a transaction inserted via API.
    fn broadcast_transaction(&self, transaction: Arc<Transaction>) -> anyhow::Result<()>;

    /// Creates a stream of next transactions gossiped from the network.
    fn gossiped_transaction_events(&self) -> BoxStream<Self::GossipedTransaction>;

    // Report the validity of a transaction received from the network.
    fn notify_gossip_transaction_validity(
        &self,
        message_info: GossipsubMessageInfo,
        validity: GossipsubMessageAcceptance,
    ) -> anyhow::Result<()>;
}

pub trait BlockImporter: Send + Sync {
    /// Wait until the next block is available
    fn block_events(&self) -> BoxStream<SharedImportResult>;
}

pub trait TxPoolDb: Send + Sync {
    fn utxo(&self, utxo_id: &UtxoId) -> StorageResult<Option<CompressedCoin>>;

    fn contract_exist(&self, contract_id: &ContractId) -> StorageResult<bool>;

    fn message(&self, message_id: &Nonce) -> StorageResult<Option<Message>>;
}

#[async_trait::async_trait]
/// Trait for getting gas price for the Tx Pool code to look up the gas price for a given block height
pub trait GasPriceProvider {
    /// Calculate gas price for the next block with a given size `block_bytes`.
    async fn last_gas_price(&self) -> TxPoolResult<GasPrice>;
}

/// Trait for getting VM memory.
#[async_trait::async_trait]
pub trait MemoryPool {
    type Memory: Memory + Send + Sync + 'static;

    /// Get the memory instance.
    async fn get_memory(&self) -> Self::Memory;
}

/// Trait for getting the latest consensus parameters.
#[cfg_attr(feature = "test-helpers", mockall::automock)]
pub trait ConsensusParametersProvider {
    /// Get latest consensus parameters.
    fn latest_consensus_parameters(&self) -> Arc<ConsensusParameters>;
}

#[async_trait::async_trait]
impl<T> GasPriceProvider for Arc<T>
where
    T: GasPriceProvider + Send + Sync,
{
    async fn last_gas_price(&self) -> TxPoolResult<GasPrice> {
        self.deref().last_gas_price().await
    }
}
